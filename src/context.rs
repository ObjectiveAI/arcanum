//! Per-process context threaded as `Arc<Context>` through every command handler.
//! Holds the env-derived [`Config`](crate::config::Config), the ObjectiveAI
//! plugin executor, and a lazily-connected Postgres store.

use objectiveai_sdk::cli::command::plugin::PluginExecutor;
use tokio::sync::OnceCell;

use crate::db::Db;

/// The env-derived runtime context, threaded through every command handler.
pub struct Context {
    /// The env-derived runtime config.
    pub config: crate::config::Config,
    /// Executor for issuing ObjectiveAI CLI commands back to the host over the
    /// plugin's stdin/stdout protocol. Cheap to clone (every field is `Arc`).
    pub executor: PluginExecutor,
    /// The Postgres store, connected on first use. Kept private: only the paths
    /// that need it (the daemon, `mcp arcanum begin`, `load_skill`) call
    /// [`Context::db`]; `--help`, `list_skills`, and the launcher-less paths
    /// never touch Postgres.
    db: OnceCell<Db>,
}

impl Context {
    /// Build the context from the process environment. Constructs the
    /// [`PluginExecutor`], which captures the process stdin/stdout and spawns
    /// its demuxer task — so it must be called exactly once, from within the
    /// tokio runtime. Does NOT connect Postgres (that happens lazily).
    pub fn new() -> Self {
        Self {
            config: crate::config::load_config(),
            executor: PluginExecutor::new(),
            db: OnceCell::new(),
        }
    }

    /// The Postgres store, connecting (and applying the schema) on first use.
    pub async fn db(&self) -> std::io::Result<&Db> {
        self.db
            .get_or_try_init(|| async {
                Db::connect(&self.config.postgres_url)
                    .await
                    .map_err(std::io::Error::other)
            })
            .await
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
