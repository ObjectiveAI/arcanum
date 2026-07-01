mod commands;
mod config;
mod context;
mod mcp;

use std::sync::Arc;

use clap::Parser;

use commands::Cli;
use context::Context;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenv::dotenv();
    // Parse first so `--help`/`--version`/parse errors exit before we build the
    // context (which loads config and constructs the plugin executor).
    let cli = Cli::parse();
    let ctx = Arc::new(Context::new());
    cli.run(ctx).await
}
