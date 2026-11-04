//! Tool registration, split into logical groups instead of server.mjs's one
//! flat file. Each submodule's `impl EvusMcpServer` block is its own
//! `#[tool_router]`; `mcp::EvusMcpServer::new` sums them.

pub mod concurrency;
pub mod content;
pub mod courses;
pub mod search;
pub mod todo;
mod tree;

/// Pretty-print a JSON value the way server.mjs's textResult did
/// (`JSON.stringify(data, null, 2)`).
pub(crate) fn json_text(value: serde_json::Value) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
}

/// Mirrors server.mjs's errorResult: `Error: ${err.message}`.
pub(crate) fn fail(err: impl std::fmt::Display) -> String {
    format!("Error: {err}")
}

/// Blackboard REST ids (course/content/attachment) get interpolated straight
/// into request paths built with `format!`. Reject anything that could
/// change the path shape (`/`, `?`, `#`, whitespace, ...) so a crafted id
/// can't redirect the request to a different endpoint.
pub(crate) fn ensure_safe_id(id: &str) -> Result<(), String> {
    let safe = !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'));
    if safe {
        Ok(())
    } else {
        Err(format!("Error: invalid id \"{id}\""))
    }
}
