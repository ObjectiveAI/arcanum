//! The `daemon` command group.

mod begin;

use std::sync::Arc;

use clap::Subcommand;

use crate::context::Context;

#[derive(Subcommand)]
pub enum Commands {
    /// Launch the MCP server.
    Begin(begin::Args),
}

impl Commands {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        match self {
            Commands::Begin(args) => args.run(ctx).await,
        }
    }
}
