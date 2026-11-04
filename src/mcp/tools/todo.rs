//! To-do calendar tool: get_todo_items. Description copied verbatim from
//! server.mjs's tool() call.

use super::{fail, json_text};

/// since/until land unescaped in the request's query string; restrict to the
/// ISO 8601 charset so a crafted value can't inject extra query params.
fn ensure_iso8601(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '+'));
    if ok {
        Ok(())
    } else {
        Err(format!("Error: invalid date \"{value}\""))
    }
}
use crate::mcp::EvusMcpServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct TodoRangeParams {
    /// ISO 8601 date
    pub since: String,
    /// ISO 8601 date
    pub until: String,
}

#[tool_router(router = tool_router_todo, vis = "pub(crate)")]
impl EvusMcpServer {
    #[rmcp::tool(description = "Get the current user's to-do calendar items", annotations(read_only_hint = true))]
    pub async fn get_todo_items(
        &self,
        Parameters(p): Parameters<TodoRangeParams>,
    ) -> Result<String, String> {
        ensure_iso8601(&p.since)?;
        ensure_iso8601(&p.until)?;
        self.bb
            .get(&format!(
                "/learn/api/v1/calendars/todo/studentItems?since={}&until={}",
                p.since, p.until
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }
}
