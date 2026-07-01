//! The `mcp` command group.

mod arcanum;

use std::sync::Arc;

use clap::Subcommand;

use crate::context::Context;

#[derive(Subcommand)]
pub enum Commands {
    /// arcanum MCP server commands. Nested under the server's name so the
    /// host's `mcp <name> begin` plugin-launch convention resolves here
    /// (`<name>` = `arcanum`).
    #[command(name = "arcanum")]
    Arcanum {
        #[command(subcommand)]
        command: arcanum::Commands,
    },
}

impl Commands {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        match self {
            Commands::Arcanum { command } => command.run(ctx).await,
        }
    }
}
