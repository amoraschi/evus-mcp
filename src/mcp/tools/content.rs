//! Content/attachment tools: list_contents, list_content_children,
//! list_attachments, download_attachment, read_attachment_text, get_content,
//! list_embedded_files, read_embedded_file_text. Descriptions copied
//! verbatim from server.mjs's tool() calls.

use super::{ensure_safe_id, fail, json_text};
use crate::bb_client::BbClient;
use crate::extract;
use crate::extract::text_query::text_or_require_query;
use crate::mcp::EvusMcpServer;
use anyhow::Result;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use scraper::{Html, Selector};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseContentId {
    pub course_id: String,
    pub content_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    pub course_id: String,
    pub content_id: String,
    pub attachment_id: String,
    /// fileName from list_attachments, used as the saved file's name
    pub file_name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentTextRef {
    pub course_id: String,
    pub content_id: String,
    pub attachment_id: String,
    /// If set, return only matching lines (+context) instead of the full text
    pub query: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct EmbeddedFileTextRef {
    /// url from list_embedded_files
    pub url: String,
    /// If set, return only matching lines (+context) instead of the full text
    pub query: Option<String>,
}

/// `file_name` is written into a fixed temp dir (see download_attachment), so
/// strip any directory components and reject anything that isn't a plain
/// file name (no "..", no path separators, no absolute path) before it's
/// ever joined onto that dir.
fn sanitize_file_name(file_name: &str) -> Result<String, String> {
    let base = std::path::Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if base.is_empty() || base == "." || base == ".." || base != file_name {
        return Err(format!("Error: invalid fileName \"{file_name}\""));
    }
    Ok(base.to_string())
}

async fn fetch_attachment(
    bb: &BbClient,
    course_id: &str,
    content_id: &str,
    attachment_id: &str,
) -> Result<(Vec<u8>, Option<String>)> {
    fetch_by_url(
        bb,
        &format!(
            "/learn/api/public/v1/courses/{course_id}/contents/{content_id}/attachments/{attachment_id}/download"
        ),
    )
    .await
}

async fn fetch_by_url(bb: &BbClient, path_or_url: &str) -> Result<(Vec<u8>, Option<String>)> {
    let res = bb.get_raw(path_or_url).await?;
    let mime_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let buf = res.bytes().await?.to_vec();
    Ok((buf, mime_type))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedFile {
    file_name: Option<String>,
    mime_type: Option<String>,
    url: Option<String>,
}

/// Ultra documents embed files inline in their `body` HTML instead of as
/// regular attachments (list_attachments/children 400 on these). Parse the
/// `data-bbfile` JSON off each <a> to recover the file's resourceUrl.
fn parse_embedded_files(body_html: &str) -> Vec<EmbeddedFile> {
    let html = Html::parse_document(body_html);
    let Ok(selector) = Selector::parse("a[data-bbfile]") else {
        return Vec::new();
    };
    html.select(&selector)
        .filter_map(|el| {
            let raw = el.value().attr("data-bbfile")?;
            let meta: serde_json::Value = serde_json::from_str(raw).ok()?;
            Some(EmbeddedFile {
                file_name: meta.get("linkName").and_then(|v| v.as_str()).map(str::to_string),
                mime_type: meta.get("mimeType").and_then(|v| v.as_str()).map(str::to_string),
                url: meta.get("resourceUrl").and_then(|v| v.as_str()).map(str::to_string),
            })
        })
        .collect()
}

#[tool_router(router = tool_router_content, vis = "pub(crate)")]
impl EvusMcpServer {
    #[rmcp::tool(description = "List top-level content items in a course", annotations(read_only_hint = true))]
    pub async fn list_contents(&self, Parameters(p): Parameters<PlainCourseIdOnly>) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        self.bb
            .get(&format!("/learn/api/public/v1/courses/{}/contents", p.course_id))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(
        description = "List child content items under a course content item (folder/lesson)",
        annotations(read_only_hint = true)
    )]
    pub async fn list_content_children(
        &self,
        Parameters(p): Parameters<CourseContentId>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        self.bb
            .get(&format!(
                "/learn/api/public/v1/courses/{}/contents/{}/children",
                p.course_id, p.content_id
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(
        description = "List attachments on a SPECIFIC content item whose exact contentId you already \
            confirmed wraps a single file (contentHandler resource/x-bb-file). If you're \
            searching for a file by name/topic and don't already have that exact \
            contentId confirmed, use find_attachment instead, it searches the whole \
            course tree for you.",
        annotations(read_only_hint = true)
    )]
    pub async fn list_attachments(
        &self,
        Parameters(p): Parameters<CourseContentId>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        self.bb
            .get(&format!(
                "/learn/api/public/v1/courses/{}/contents/{}/attachments",
                p.course_id, p.content_id
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(
        description = "Download a course content attachment to a local temp file and return its path. \
            fileName should come from list_attachments' result for this attachment."
    )]
    pub async fn download_attachment(
        &self,
        Parameters(p): Parameters<AttachmentRef>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        ensure_safe_id(&p.attachment_id)?;
        let file_name = sanitize_file_name(&p.file_name)?;
        let (buf, mime_type) = fetch_attachment(&self.bb, &p.course_id, &p.content_id, &p.attachment_id)
            .await
            .map_err(fail)?;
        let dir = std::env::temp_dir().join("evus-mcp");
        tokio::fs::create_dir_all(&dir).await.map_err(fail)?;
        let file_path = dir.join(&file_name);
        tokio::fs::write(&file_path, &buf).await.map_err(fail)?;
        Ok(json_text(serde_json::json!({
            "filePath": file_path.to_string_lossy(),
            "bytes": buf.len(),
            "mimeType": mime_type,
        })))
    }

    #[rmcp::tool(
        description = "Download a course content attachment and return its text content directly \
            (PDF, plain text, docx, pptx, and legacy doc/ppt). Use this instead of \
            download_attachment when you need to read the content, not just save the \
            file. Legacy .doc/.ppt extraction is noisy and can be very large, pass \
            query to search it instead of getting the full dump.",
        annotations(read_only_hint = true)
    )]
    pub async fn read_attachment_text(
        &self,
        Parameters(p): Parameters<AttachmentTextRef>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        ensure_safe_id(&p.attachment_id)?;
        let (buf, mime_type) = fetch_attachment(&self.bb, &p.course_id, &p.content_id, &p.attachment_id)
            .await
            .map_err(fail)?;
        let text = extract::extract_text(buf, mime_type.as_deref()).await.map_err(fail)?;
        text_or_require_query(&text, p.query.as_deref()).map_err(fail)
    }

    #[rmcp::tool(
        description = "Get a single content item's details, including its raw body HTML for Ultra \
            documents (contentHandler resource/x-bb-document). Use when list_attachments \
            or list_content_children returns 400, that content item likely embeds files \
            inline instead; use list_embedded_files on it.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_content(&self, Parameters(p): Parameters<CourseContentId>) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        self.bb
            .get(&format!(
                "/learn/api/public/v1/courses/{}/contents/{}",
                p.course_id, p.content_id
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(
        description = "List files embedded inline in an Ultra document's body (not regular attachments). \
            Use when list_attachments returns 400 for a content item.",
        annotations(read_only_hint = true)
    )]
    pub async fn list_embedded_files(
        &self,
        Parameters(p): Parameters<CourseContentId>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        ensure_safe_id(&p.content_id)?;
        let item = self
            .bb
            .get(&format!(
                "/learn/api/public/v1/courses/{}/contents/{}",
                p.course_id, p.content_id
            ))
            .await
            .map_err(fail)?;
        let body = item.get("body").and_then(|v| v.as_str()).unwrap_or_default();
        let files = parse_embedded_files(body);
        serde_json::to_value(files).map(json_text).map_err(fail)
    }

    #[rmcp::tool(
        description = "Download an embedded file (from list_embedded_files' url) and return its text \
            content directly. Legacy .doc/.ppt extraction is noisy and can be very \
            large, pass query to search it instead of getting the full dump.",
        annotations(read_only_hint = true)
    )]
    pub async fn read_embedded_file_text(
        &self,
        Parameters(p): Parameters<EmbeddedFileTextRef>,
    ) -> Result<String, String> {
        let (buf, mime_type) = fetch_by_url(&self.bb, &p.url).await.map_err(fail)?;
        let text = extract::extract_text(buf, mime_type.as_deref()).await.map_err(fail)?;
        text_or_require_query(&text, p.query.as_deref()).map_err(fail)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlainCourseIdOnly {
    pub course_id: String,
}
