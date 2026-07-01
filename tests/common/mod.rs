//! Integration-test harness for arcanum.
//!
//! Every test drives the prebuilt `objectiveai` host in the repo's
//! `.objectiveai/` (staged by `build.sh`) through the SDK [`BinaryExecutor`].
//! A test gets an isolated state by setting `OBJECTIVEAI_STATE` to its own name;
//! the host bootstraps a fresh per-state postgres on first command.
//!
//! The `list_skills` tool discovers `SKILL.md` folders across the agent's
//! laboratories, so a test creates laboratories with host folders mounted,
//! attaches them to a GROUPED mock-agent tag, spawns the agent (whose scripted
//! `calls` invoke `arcanum_list_skills`), waits, and reads the tool result text
//! back out of postgres.
#![allow(dead_code)]

use std::path::PathBuf;

use futures::StreamExt;
use objectiveai_sdk::agent::InlineAgentBaseWithFallbacksOrRemoteCommitOptional;
use objectiveai_sdk::cli::command::agents::laboratories::attach as labs_attach;
use objectiveai_sdk::cli::command::agents::message::RequestMessage;
use objectiveai_sdk::cli::command::agents::selector::AgentSelector;
use objectiveai_sdk::cli::command::agents::spawn as agents_spawn;
use objectiveai_sdk::cli::command::agents::tags::apply as tags_apply;
use objectiveai_sdk::cli::command::agents::wait as agents_wait;
use objectiveai_sdk::cli::command::binary::BinaryExecutor;
use objectiveai_sdk::cli::command::db::query as db_query;
use objectiveai_sdk::cli::command::laboratories::create as labs_create;
use objectiveai_sdk::cli::command::{CommandExecutor, CommandRequest, CommandResponse};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use labs_create::{EnvVar, Mount};

/// Base image the test laboratories run. `bash:latest` ships `find` + `bash`.
pub const BASE_IMAGE: &str = "docker.io/library/bash:latest";

fn objectiveai_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".objectiveai")
}

/// One scripted mock tool-call turn: call `name` with `arguments` (the mock
/// wants arguments as a JSON *string*).
pub fn tool_call(name: &str, arguments: Value) -> Value {
    json!({ "tool_calls": [ { "name": name, "arguments": arguments.to_string() } ], "content": "" })
}

/// Build the arcanum mock-agent spec running the given scripted tool-call turns.
/// Puts the arcanum plugin in the arsenal (with a large `token-repeat`, so the
/// growth-driven re-injection never fires mid-test) and appends a terminating
/// content-only turn so the mock completion ends cleanly.
pub fn arcanum_agent_with_calls(calls: Vec<Value>) -> Value {
    let mut calls = calls;
    calls.push(json!({ "tool_calls": [], "content": "done" }));
    json!({
        "upstream": "mock",
        "output_mode": "instruction",
        "instruction": "done",
        "client_objectiveai_mcp": { "plugins": [
            {
                "owner": "ObjectiveAI", "name": "arcanum", "version": "0.1.0",
                "executable": false,
                "mcp_servers": [ { "name": "arcanum", "arguments": { "token-repeat": "1000000" } } ],
            }
        ]},
        "calls": calls,
    })
}

/// The default agent: a single `arcanum_list_skills` call. Laboratories are
/// attached to the tag separately (see [`Host::attach_lab`]).
pub fn arcanum_agent() -> Value {
    arcanum_agent_with_calls(vec![tool_call("arcanum_list_skills", json!({}))])
}

/// Drives arcanum against one isolated objectiveai state.
pub struct Host {
    executor: BinaryExecutor,
    state: String,
}

impl Host {
    /// Build a handle for a test; pass the test's own name as `state`.
    pub fn new(state: &str) -> Self {
        let dir = objectiveai_dir();
        let executor = BinaryExecutor::new(Some(dir.clone()))
            .env("OBJECTIVEAI_DIR", dir.to_string_lossy().into_owned())
            .env("OBJECTIVEAI_STATE", state)
            .kill_on_drop(true);
        Self {
            executor,
            state: state.to_string(),
        }
    }

    async fn execute_one<R, T>(&self, req: R) -> T
    where
        R: CommandRequest + Send + Serialize,
        T: CommandResponse + Serialize + DeserializeOwned + Send + 'static,
    {
        self.executor
            .execute_one(req, None)
            .await
            .unwrap_or_else(|e| panic!("[{}] execute_one: {e:?}", self.state))
    }

    async fn collect_stream<R, T>(&self, req: R) -> Vec<T>
    where
        R: CommandRequest + Send + Serialize,
        T: CommandResponse + Serialize + DeserializeOwned + Send + 'static,
    {
        let mut stream = self
            .executor
            .execute::<R, T>(req, None)
            .await
            .unwrap_or_else(|e| panic!("[{}] execute: {e:?}", self.state));
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            out.push(item.unwrap_or_else(|e| panic!("[{}] stream item: {e:?}", self.state)));
        }
        out
    }

    /// Create a client laboratory with the given mounts/env/cwd on [`BASE_IMAGE`].
    pub async fn create_lab(&self, id: &str, mounts: Vec<Mount>, env: Vec<EnvVar>, cwd: &str) {
        let _: labs_create::Response = self
            .execute_one(labs_create::Request {
                path_type: labs_create::Path::LaboratoriesCreate,
                kind: labs_create::Kind::Client,
                id: id.to_string(),
                image: BASE_IMAGE.to_string(),
                mounts,
                env,
                cwd: cwd.to_string(),
                base: Default::default(),
            })
            .await;
    }

    /// Apply `tag` as a GROUPED tag carrying the mock `agent_spec`.
    pub async fn apply_tag(&self, tag: &str, agent_spec: Value) {
        let spec: InlineAgentBaseWithFallbacksOrRemoteCommitOptional =
            serde_json::from_value(agent_spec).expect("agent spec deserializes");
        let _: tags_apply::Response = self
            .execute_one(tags_apply::Request {
                path_type: tags_apply::Path::AgentsTagsApply,
                name: tag.to_string(),
                target: tags_apply::Target::Agent {
                    agent_spec: spec,
                    parent_agent_instance_hierarchy: None,
                },
                base: Default::default(),
            })
            .await;
    }

    /// Attach a laboratory to `tag` so it resolves into the tag's sessions.
    pub async fn attach_lab(&self, tag: &str, laboratory_id: &str) {
        let _: labs_attach::Response = self
            .execute_one(labs_attach::Request {
                path_type: labs_attach::Path::AgentsLaboratoriesAttach,
                selector: AgentSelector::Tag {
                    agent_tag: tag.to_string(),
                },
                laboratory_id: laboratory_id.to_string(),
                base: Default::default(),
            })
            .await;
    }

    /// Spawn the tag (streaming) and return `(agent_instance_hierarchy, response_id)`.
    pub async fn spawn_tag(&self, tag: &str) -> (String, String) {
        let items: Vec<agents_spawn::ResponseItem> = self
            .collect_stream(agents_spawn::Request {
                path_type: agents_spawn::Path::AgentsSpawn,
                message: RequestMessage::Simple("go".to_string()),
                agent: AgentSelector::Tag {
                    agent_tag: tag.to_string(),
                },
                dangerous_advanced: Some(agents_spawn::RequestDangerousAdvanced {
                    stream: Some(true),
                    seed: Some(1),
                }),
                base: Default::default(),
            })
            .await;
        let aih = items
            .iter()
            .find_map(|i| match i {
                agents_spawn::ResponseItem::Chunk(c) if !c.agent_instance_hierarchy.is_empty() => {
                    Some(c.agent_instance_hierarchy.clone())
                }
                _ => None,
            })
            .expect("spawn emits an agent_instance_hierarchy");
        let response_id = items
            .iter()
            .find_map(|i| match i {
                agents_spawn::ResponseItem::Chunk(c) if !c.id.is_empty() => Some(c.id.clone()),
                _ => None,
            })
            .expect("spawn emits a response id");
        (aih, response_id)
    }

    /// Block until agent instance `aih` has fully finalized (`agents wait`).
    pub async fn wait(&self, aih: &str) {
        let (parent, instance) = aih.rsplit_once('/').expect("aih contains a '/'");
        let _: Vec<agents_wait::Response> = self
            .collect_stream(agents_wait::Request {
                path_type: agents_wait::Path::AgentsWait,
                agent: AgentSelector::Instance {
                    parent_agent_instance_hierarchy: Some(parent.to_string()),
                    agent_instance: instance.to_string(),
                },
                base: Default::default(),
            })
            .await;
    }

    /// Every tool-result text for `response_id`, in order (the strings the agent
    /// received back from its tool calls).
    pub async fn tool_texts(&self, response_id: &str) -> Vec<String> {
        let sql = format!(
            "SELECT text FROM objectiveai.tool_response_content_text \
             WHERE response_id = '{}' ORDER BY \"index\", part_index",
            response_id.replace('\'', "''"),
        );
        let resp: db_query::Response = self
            .execute_one(db_query::Request {
                path_type: db_query::Path::DbQuery,
                query: sql,
                base: Default::default(),
            })
            .await;
        resp.rows
            .into_iter()
            .filter_map(|mut row| row.pop())
            .filter_map(|v| match v {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// Every queued message text in this state (e.g. arcanum's skill injections).
    pub async fn message_texts(&self) -> Vec<String> {
        let resp: db_query::Response = self
            .execute_one(db_query::Request {
                path_type: db_query::Path::DbQuery,
                query: "SELECT text FROM objectiveai.message_queue_texts".to_string(),
                base: Default::default(),
            })
            .await;
        resp.rows
            .into_iter()
            .filter_map(|mut row| row.pop())
            .filter_map(|v| match v {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect()
    }
}
