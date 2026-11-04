//! Recursive content-tree walk shared by find_attachment and
//! search_courses_content. Direct port of server.mjs's searchContentTree /
//! findAttachmentTree.

use super::concurrency::map_limit;
use crate::bb_client::BbClient;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMatch {
    pub course_id: String,
    pub content_id: String,
    pub title: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMatch {
    pub course_id: String,
    pub content_id: String,
    pub title: Option<String>,
    pub attachment_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
}

fn item_id(item: &Value) -> Option<String> {
    item.get("id").and_then(|v| v.as_str()).map(str::to_string)
}

fn item_title(item: &Value) -> Option<String> {
    item.get("title").and_then(|v| v.as_str()).map(str::to_string)
}

fn results_of(value: Value) -> Vec<Value> {
    value
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default()
}

pub fn search_content_tree<'a>(
    bb: &'a BbClient,
    course_id: &'a str,
    items: Vec<Value>,
    query: &'a str,
    depth: u32,
    max_depth: u32,
    matches: Arc<Mutex<Vec<ContentMatch>>>,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        for item in &items {
            if let Some(title) = item_title(item) {
                if title.to_lowercase().contains(query) {
                    if let Some(id) = item_id(item) {
                        matches.lock().await.push(ContentMatch {
                            course_id: course_id.to_string(),
                            content_id: id,
                            title,
                        });
                    }
                }
            }
        }
        if depth < max_depth {
            map_limit(items, 8, |item| {
                let matches = matches.clone();
                async move {
                    let Some(id) = item_id(&item) else { return };
                    let path = format!("/learn/api/public/v1/courses/{course_id}/contents/{id}/children");
                    if let Ok(children) = bb.get(&path).await {
                        let results = results_of(children);
                        if !results.is_empty() {
                            search_content_tree(bb, course_id, results, query, depth + 1, max_depth, matches)
                                .await;
                        }
                    }
                }
            })
            .await;
        }
    })
}

pub fn find_attachment_tree<'a>(
    bb: &'a BbClient,
    course_id: &'a str,
    items: Vec<Value>,
    query: &'a str,
    depth: u32,
    max_depth: u32,
    matches: Arc<Mutex<Vec<AttachmentMatch>>>,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        map_limit(items, 8, |item| {
            let matches = matches.clone();
            async move {
                let Some(id) = item_id(&item) else { return };
                let title = item_title(&item);
                let att_path =
                    format!("/learn/api/public/v1/courses/{course_id}/contents/{id}/attachments");
                if let Ok(attachments) = bb.get(&att_path).await {
                    for att in results_of(attachments) {
                        let file_name = att.get("fileName").and_then(|v| v.as_str()).map(str::to_string);
                        let matches_filename = file_name
                            .as_deref()
                            .map(|f| f.to_lowercase().contains(query))
                            .unwrap_or(false);
                        let matches_title = title
                            .as_deref()
                            .map(|t| t.to_lowercase().contains(query))
                            .unwrap_or(false);
                        if matches_filename || matches_title {
                            let Some(att_id) = att.get("id").and_then(|v| v.as_str()) else { continue };
                            matches.lock().await.push(AttachmentMatch {
                                course_id: course_id.to_string(),
                                content_id: id.clone(),
                                title: title.clone(),
                                attachment_id: att_id.to_string(),
                                file_name,
                                mime_type: att.get("mimeType").and_then(|v| v.as_str()).map(str::to_string),
                            });
                        }
                    }
                }
                if depth < max_depth {
                    let children_path =
                        format!("/learn/api/public/v1/courses/{course_id}/contents/{id}/children");
                    if let Ok(children) = bb.get(&children_path).await {
                        let results = results_of(children);
                        if !results.is_empty() {
                            find_attachment_tree(bb, course_id, results, query, depth + 1, max_depth, matches)
                                .await;
                        }
                    }
                }
            }
        })
        .await;
    })
}
