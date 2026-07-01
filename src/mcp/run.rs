//! Boots the streamable-HTTP MCP server in-process and announces its connect URL
//! on stdout.

use std::sync::Arc;

use objectiveai_sdk::cli::command::plugins::run::{Mcp, McpType};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use super::ArcanumMcp;
use crate::context::Context;

/// Run the MCP server until process death.
///
/// Binds an OS-assigned port on loopback (`127.0.0.1:0`), prints one JSONL line
/// carrying `http://<addr>` on stdout (the host parses it as `Output::Mcp`),
/// then serves.
pub async fn run(ctx: Arc<Context>) -> std::io::Result<()> {
    let server = ArcanumMcp::new(ctx);
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        {
            // `StreamableHttpServerConfig` is `#[non_exhaustive]`, so mutate a
            // default rather than constructing it with a struct literal.
            // Stateful: the host's MCP proxy requires the `Mcp-Session-Id` header.
            let mut cfg = StreamableHttpServerConfig::default();
            cfg.stateful_mode = true;
            cfg.sse_keep_alive = None;
            cfg
        },
    );

    let router = axum::Router::new().fallback_service(service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;

    // Announce the connect URL; the host parses this stdout line as `Output::Mcp`.
    let addr = listener.local_addr()?;
    let announcement = Mcp {
        r#type: McpType::Mcp,
        url: format!("http://{addr}"),
    };
    println!(
        "{}",
        serde_json::to_string(&announcement).expect("Mcp serializes")
    );

    axum::serve(listener, router).await
}
