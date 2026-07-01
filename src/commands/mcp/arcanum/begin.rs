//! `mcp arcanum begin` — run the MCP server in-process.
//!
//! Binds a loopback port, prints one JSONL line carrying the connect URL to
//! stdout (the host parses it as `Output::Mcp`), and serves until the process
//! is killed. Unlike quas-wex-exort's launcher, there is no daemon: this *is*
//! the server.

use std::sync::Arc;

use crate::context::Context;

#[derive(clap::Args)]
pub struct Args {}

impl Args {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        crate::mcp::run(ctx).await
    }
}
