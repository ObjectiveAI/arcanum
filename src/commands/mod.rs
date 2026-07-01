//! The top-level CLI: parse argv and dispatch to command groups.

mod daemon;
mod mcp;

use std::sync::Arc;

use clap::{Parser, Subcommand};

use crate::context::Context;

/// Skills for ObjectiveAI agents: an MCP server exposing the plugin's tools.
#[derive(Parser)]
#[command(name = "arcanum", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Daemon commands.
    #[command(subcommand)]
    Daemon(daemon::Commands),
    /// MCP server commands.
    #[command(subcommand)]
    Mcp(mcp::Commands),
}

impl Cli {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        match self.command {
            Commands::Daemon(command) => command.run(ctx).await,
            Commands::Mcp(command) => command.run(ctx).await,
        }
    }
}
