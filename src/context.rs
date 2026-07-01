//! Per-process context threaded as `Arc<Context>` through every command handler.
//! Holds the env-derived [`Config`](crate::config::Config) and the ObjectiveAI
//! plugin executor.

use objectiveai_sdk::cli::command::plugin::PluginExecutor;

/// The env-derived runtime context, threaded through every command handler.
pub struct Context {
    /// The env-derived runtime config.
    pub config: crate::config::Config,
    /// Executor for issuing ObjectiveAI CLI commands back to the host over the
    /// plugin's stdin/stdout protocol. Cheap to clone (every field is `Arc`).
    pub executor: PluginExecutor,
}

impl Context {
    /// Build the context from the process environment. Constructs the
    /// [`PluginExecutor`], which captures the process stdin/stdout and spawns
    /// its demuxer task — so it must be called exactly once, from within the
    /// tokio runtime.
    pub fn new() -> Self {
        Self {
            config: crate::config::load_config(),
            executor: PluginExecutor::new(),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
