//! The `mcp arcanum` command group.

mod begin;

use std::sync::Arc;

use clap::Subcommand;

use crate::context::Context;

#[derive(Subcommand)]
pub enum Commands {
    /// Run the MCP server in-process and announce its URL on stdout.
    Begin(begin::Args),
}

impl Commands {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        match self {
            Commands::Begin(args) => args.run(ctx).await,
        }
    }
}
