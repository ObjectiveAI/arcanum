//! `daemon begin` — launch the MCP server.

use std::sync::Arc;

use crate::context::Context;

/// Launch the MCP server: bind a loopback port, publish the connect URL to the
/// daemon lockfile, and serve until the process exits.
#[derive(clap::Args)]
pub struct Args {}

impl Args {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        crate::mcp::run(ctx).await
    }
}
