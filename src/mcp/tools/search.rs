//! Cross-content search tools: find_attachment, search_courses_content.
//! Descriptions copied verbatim from server.mjs's tool() calls.

use super::{ensure_safe_id, fail};
use super::tree::{find_attachment_tree, search_content_tree};
use crate::mcp::EvusMcpServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

fn clamp_depth(max_depth: Option<u32>, default: u32) -> u32 {
    max_depth.unwrap_or(default).min(4)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindAttachmentParams {
    pub course_id: String,
    /// Case-insensitive substring to match against fileName or content title
    pub query: String,
    /// How deep to recurse into folders
    pub max_depth: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchCoursesContentParams {
    /// Case-insensitive substring to search for in content titles
    pub query: String,
    /// How deep to recurse into folders
    pub max_depth: Option<u32>,
}

#[tool_router(router = tool_router_search, vis = "pub(crate)")]
impl EvusMcpServer {
    #[rmcp::tool(
        description = "Find a file attachment by name or containing content item's title, searching \
            a course's content tree (folders/children) so you don't have to manually \
            walk list_content_children/list_attachments yourself. Returns attachmentId \
            + fileName ready to pass to download_attachment/read_attachment_text.",
        annotations(read_only_hint = true)
    )]
    pub async fn find_attachment(
        &self,
        Parameters(p): Parameters<FindAttachmentParams>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        let max_depth = clamp_depth(p.max_depth, 3);
        let contents = self
            .bb
            .get(&format!("/learn/api/public/v1/courses/{}/contents", p.course_id))
            .await
            .map_err(fail)?;
        let items = contents
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        let query = p.query.to_lowercase();
        let matches = Arc::new(Mutex::new(Vec::new()));
        find_attachment_tree(&self.bb, &p.course_id, items, &query, 0, max_depth, matches.clone()).await;
        let matches = Arc::try_unwrap(matches).unwrap().into_inner();
        serde_json::to_string_pretty(&matches).map_err(fail)
    }

    #[rmcp::tool(
        description = "Search content item titles (documents, folders, lessons) across all enrolled \
            courses for a keyword. Use instead of manually walking list_contents/ \
            list_content_children across every course.",
        annotations(read_only_hint = true)
    )]
    pub async fn search_courses_content(
        &self,
        Parameters(p): Parameters<SearchCoursesContentParams>,
    ) -> Result<String, String> {
        let max_depth = clamp_depth(p.max_depth, 2);
        let user_id = self.bb.user_id().await.map_err(fail)?;
        let courses = self
            .bb
            .get(&format!("/learn/api/public/v1/users/{user_id}/courses"))
            .await
            .map_err(fail)?;
        let enrollments = courses
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        let query = p.query.to_lowercase();
        let matches = Arc::new(Mutex::new(Vec::new()));

        super::concurrency::map_limit(enrollments, 8, |enrollment| {
            let matches = matches.clone();
            let query = query.clone();
            let bb = &self.bb;
            async move {
                let Some(course_id) = enrollment.get("courseId").and_then(|v| v.as_str()) else {
                    return;
                };
                let Ok(contents) = bb
                    .get(&format!("/learn/api/public/v1/courses/{course_id}/contents"))
                    .await
                else {
                    return;
                };
                let items = contents
                    .get("results")
                    .and_then(|r| r.as_array())
                    .cloned()
                    .unwrap_or_default();
                if !items.is_empty() {
                    search_content_tree(bb, course_id, items, &query, 0, max_depth, matches).await;
                }
            }
        })
        .await;

        let matches = Arc::try_unwrap(matches).unwrap().into_inner();
        serde_json::to_string_pretty(&matches).map_err(fail)
    }
}
