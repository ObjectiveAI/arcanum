//! Env-driven runtime config (3-struct pattern; mirrors objectiveai-cli).
//!
//! [`EnvConfigBuilder`] is the raw `Envconfig`-derived reader: every field is an
//! `Option<String>` straight from the environment. It lowers into
//! [`ConfigBuilder`] (still all-optional, so `init*` can never fail on a missing
//! var), which finally [`build`](ConfigBuilder::build)s into the [`Config`] the
//! rest of the program uses. Unlike the sibling plugins we default missing vars
//! rather than panicking, so `--help` and local runs work without the full
//! ObjectiveAI environment.

use std::path::PathBuf;

use envconfig::Envconfig;

#[derive(Envconfig)]
struct EnvConfigBuilder {
    #[envconfig(from = "OBJECTIVEAI_STATE_DIR")]
    state_dir: Option<String>,
    #[envconfig(from = "OBJECTIVEAI_AGENT_INSTANCE_HIERARCHY")]
    objectiveai_agent_instance_hierarchy: Option<String>,
}

impl EnvConfigBuilder {
    pub fn build(self) -> ConfigBuilder {
        ConfigBuilder {
            state_dir: self.state_dir,
            objectiveai_agent_instance_hierarchy: self.objectiveai_agent_instance_hierarchy,
        }
    }
}

#[derive(Default)]
pub struct ConfigBuilder {
    pub state_dir: Option<String>,
    pub objectiveai_agent_instance_hierarchy: Option<String>,
}

impl Envconfig for ConfigBuilder {
    #[allow(deprecated)]
    fn init() -> Result<Self, envconfig::Error> {
        EnvConfigBuilder::init().map(|e| e.build())
    }

    fn init_from_env() -> Result<Self, envconfig::Error> {
        EnvConfigBuilder::init_from_env().map(|e| e.build())
    }

    fn init_from_hashmap(
        h: &std::collections::HashMap<String, String>,
    ) -> Result<Self, envconfig::Error> {
        EnvConfigBuilder::init_from_hashmap(h).map(|e| e.build())
    }
}

impl ConfigBuilder {
    pub fn build(self) -> Config {
        Config {
            state_dir: PathBuf::from(self.state_dir.unwrap_or_else(|| ".".to_string())),
            objectiveai_agent_instance_hierarchy: self
                .objectiveai_agent_instance_hierarchy
                .unwrap_or_else(|| "arcanum".to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Root of the CLI's filesystem state tree (env `OBJECTIVEAI_STATE_DIR`).
    /// Defaults to the current directory.
    pub state_dir: PathBuf,
    /// This agent instance's hierarchy (env
    /// `OBJECTIVEAI_AGENT_INSTANCE_HIERARCHY`). Defaults to `"arcanum"`.
    pub objectiveai_agent_instance_hierarchy: String,
}

impl Config {
    /// The state root (env `OBJECTIVEAI_STATE_DIR`).
    pub fn state_dir(&self) -> PathBuf {
        self.state_dir.clone()
    }
}

/// Build the runtime config from the process environment.
pub fn load_config() -> Config {
    ConfigBuilder::init_from_env().unwrap_or_default().build()
}
