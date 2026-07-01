//! The arcanum agent-facing MCP server.
//!
//! A streamable-HTTP `rmcp` server whose tool routers expose the plugin's
//! capabilities to ObjectiveAI agents. Add more routers with the `+` operator
//! in [`ArcanumMcp::new`].

pub(crate) mod common;
mod list_skills;
mod load_skill;
mod run;

use std::sync::Arc;

use rmcp::{
    ServerHandler, tool_handler,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
};

use crate::context::Context;
use crate::monitor::MonitorService;

pub use run::run;

/// The MCP server handler. Cheap to `clone` (the service factory clones one per
/// session); shared state lives behind `Arc`.
#[derive(Clone)]
pub struct ArcanumMcp {
    pub tool_router: ToolRouter<Self>,
    /// The runtime context (config + plugin executor + lazy DB), shared across
    /// all session clones. `list_skills`/`load_skill` read `context.executor`.
    context: Arc<Context>,
    /// The daemon's token-usage monitor, shared with the LISTEN task.
    /// `load_skill` registers loaded skills and starts monitor loops through it.
    monitor: Arc<MonitorService>,
}

impl ArcanumMcp {
    pub fn new(context: Arc<Context>, monitor: Arc<MonitorService>) -> Self {
        Self {
            tool_router: Self::list_skills_tools() + Self::load_skill_tools(),
            context,
            monitor,
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
        // `serverInfo.name` (so `list_skills` surfaces as `arcanum_list_skills`).
        info.server_info.name = "arcanum".into();
        info
    }
}
