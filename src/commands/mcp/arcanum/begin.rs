//! `mcp arcanum begin` — ensure the daemon is up, wait for its MCP server to
//! publish its URL, and announce that URL to the host.
//!
//! This is a thin launcher, not the server itself: it spawns the daemon (which
//! runs our `daemon begin` MCP server), subscribe-reads the server's URL from
//! the `"mcp"` lockfile, prints it, and exits — the server persists in the
//! daemon and is shared by every agent.

use std::sync::Arc;

use futures::StreamExt;
use objectiveai_sdk::cli::command::daemon::spawn as daemon_spawn;
use objectiveai_sdk::cli::command::plugins::run::{Mcp, McpType};

use crate::context::Context;

#[derive(clap::Args)]
pub struct Args {}

impl Args {
    pub async fn run(self, ctx: Arc<Context>) -> std::io::Result<()> {
        // 1. Ensure the daemon is up. The SDK daemon launches our `daemon begin`
        //    (per the plugin manifest's `daemon: true`), which runs the MCP
        //    server and publishes its URL to the `"mcp"` lockfile.
        let mut stream = daemon_spawn::execute(
            &ctx.executor,
            daemon_spawn::Request {
                path_type: daemon_spawn::Path::DaemonSpawn,
                dangerous_advanced: None,
                base: Default::default(),
            },
            None,
        )
        .await
        .map_err(std::io::Error::other)?;
        if let Some(item) = stream.next().await {
            item.map_err(std::io::Error::other)?;
        }

        // 2. Wait for the MCP server to publish its connect URL.
        let lock_dir = ctx.config.state_dir().join("locks");
        let url = objectiveai_sdk::lockfile::wait_read(&lock_dir, "mcp").await?;

        // 3. Announce it; the host parses this stdout line as `Output::Mcp`.
        let response = Mcp {
            r#type: McpType::Mcp,
            url,
        };
        println!(
            "{}",
            serde_json::to_string(&response).expect("Mcp serializes")
        );
        Ok(())
    }
}
