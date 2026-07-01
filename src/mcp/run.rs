//! Boots the streamable-HTTP MCP server and publishes its connect URL to the
//! daemon lockfile. Run by `daemon begin`; the single daemon-hosted server is
//! shared by every agent.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use super::ArcanumMcp;
use crate::context::Context;

/// Run the MCP server until process death.
///
/// Binds an OS-assigned port on loopback (`127.0.0.1:0`), publishes the
/// resulting `http://<addr>` into `<state_dir>/locks` under key `"mcp"` for the
/// launcher (`mcp arcanum begin`) to discover, then serves. Produces no
/// stdout/stderr — the lockfile is the only side channel.
pub async fn run(ctx: Arc<Context>) -> std::io::Result<()> {
    let lock_dir = ctx.config.state_dir().join("locks");

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

    // Publish the connect URL for the launcher, mapping an unspecified bind to
    // loopback. The `LockClaim` is held until process death (it leaks on drop by
    // design); we only check for a conflicting live holder.
    let addr = listener.local_addr()?;
    let connect_ip = match addr.ip() {
        std::net::IpAddr::V4(v4) if v4.is_unspecified() => {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        }
        std::net::IpAddr::V6(v6) if v6.is_unspecified() => {
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        }
        ip => ip,
    };
    let connect_url = format!("http://{}", std::net::SocketAddr::new(connect_ip, addr.port()));
    if objectiveai_sdk::lockfile::try_acquire(&lock_dir, "mcp", &connect_url)
        .await
        .is_none()
    {
        return Err(std::io::Error::other(
            "another arcanum instance already holds the mcp lock for this state",
        ));
    }

    axum::serve(listener, router).await
}
