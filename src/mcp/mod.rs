//! The arcanum agent-facing MCP server.
//!
//! A streamable-HTTP `rmcp` server whose tool routers expose the plugin's
//! capabilities to ObjectiveAI agents. Only the `hello` toolset is wired in for
//! now; add more routers with the `+` operator in [`ArcanumMcp::new`].

mod hello;
mod run;

use std::sync::Arc;

use rmcp::{
    ServerHandler, tool_handler,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
};

use crate::context::Context;

pub use run::run;

/// The MCP server handler. Cheap to `clone` (the service factory clones one per
/// session); shared state lives behind `Arc`.
#[derive(Clone)]
pub struct ArcanumMcp {
    pub tool_router: ToolRouter<Self>,
    /// The runtime context (config + plugin executor), shared across all session
    /// clones. Retained for future tools.
    #[allow(dead_code)]
    context: Arc<Context>,
}

impl ArcanumMcp {
    pub fn new(context: Arc<Context>) -> Self {
        Self {
            tool_router: Self::hello_tools(),
            context,
        }
    }
}

// `#[tool_handler]` generates `get_tool`, `call_tool`, and `list_tools` from the
// router; we override only `get_info` to set our server name.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for ArcanumMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        // The host's MCP proxy prefixes agent-visible tool names with this
        // `serverInfo.name` (so `hello` surfaces as `arcanum_hello`).
        info.server_info.name = "arcanum".into();
        info
    }
}
