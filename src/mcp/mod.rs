//! MCP server (stdio transport). stdout is the protocol channel: never
//! `println!`, log to stderr only (`eprintln!`/`tracing`).
//!
//! Tool registration is split by logical group into `tools::*` instead of
//! one flat server.mjs: each submodule defines its own `#[tool_router]` over
//! `EvusMcpServer`, and the routers are summed here.

pub mod tools;

use crate::bb_client::BbClient;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool_handler};

pub struct EvusMcpServer {
    pub(crate) bb: BbClient,
    tool_router: ToolRouter<EvusMcpServer>,
}

impl Default for EvusMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl EvusMcpServer {
    pub fn new() -> Self {
        // Each tools::* submodule's #[tool_router(router = tool_router_x)] generates
        // an inherent associated fn on EvusMcpServer itself (not a free fn in that
        // submodule), since the impl block it decorates targets this type.
        let tool_router = Self::tool_router_courses()
            + Self::tool_router_content()
            + Self::tool_router_search()
            + Self::tool_router_todo();
        Self { bb: BbClient::new(), tool_router }
    }
}

const INSTRUCTIONS: &str =
    "Access to ev.us.es, EVUS (Ensenanza Virtual, Universidad de Sevilla), the \
     Blackboard Learn instance for Universidad de Sevilla (UVUS). Use for anything \
     about this user's enrolled courses, course content, documents, announcements, \
     grades, or assignments at the University of Seville (Universidad de Sevilla). \
     Spanish course/subject references (a course code, 'asignatura', 'curso de la \
     universidad/uni') are a strong signal to check here. Tool calls re-login once \
     on their own if the session expired, so a single failure needs no action. If \
     a tool call fails again right after that, or SAML/TOTP auth keeps failing \
     across repeated calls, run `evus-mcp login` (the standalone login CLI) to see \
     the raw SAML error, then restart Claude so this MCP server reconnects with a \
     fresh session, and report the error to the user instead of retrying blindly.";

#[tool_handler(router = self.tool_router.clone())]
impl ServerHandler for EvusMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("evus-mcp", "0.1.0"))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Run the MCP server over stdio until the client disconnects.
pub async fn run() -> anyhow::Result<()> {
    let server = EvusMcpServer::new();
    let service = rmcp::ServiceExt::serve(server, rmcp::transport::io::stdio()).await?;
    eprintln!("[evus-mcp] server started");
    service.waiting().await?;
    Ok(())
}
