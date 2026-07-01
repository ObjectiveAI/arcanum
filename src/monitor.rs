//! The daemon's per-agent token-usage monitor.
//!
//! For each watched agent instance hierarchy (AIH) it loops
//! `agents logs token-usage subscribe`; whenever the agent's `total_tokens`
//! grows past its `token_repeat` since the last injection, it re-enqueues the
//! loaded skill as a `<arcanum>…</arcanum>` message. It keeps subscribing even
//! with no skill loaded (advancing the baseline quietly) and stops when the
//! instance goes inactive.

use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use futures::StreamExt;
use objectiveai_sdk::cli::command::agents::enqueue;
use objectiveai_sdk::cli::command::agents::logs::token_usage::{get as tu_get, subscribe as tu_subscribe};
use objectiveai_sdk::cli::command::agents::message::RequestMessage;
use objectiveai_sdk::cli::command::agents::selector::AgentSelector;
use objectiveai_sdk::cli::command::plugin::PluginExecutor;
use tokio::task::JoinHandle;

use crate::db::Db;

/// Idempotency key for the re-injected skill message: a later injection replaces
/// any still-queued earlier one for the same agent.
const ENQUEUE_KEY: &str = "arcanum-skill";

/// Runs the per-AIH token-usage monitor loops in the daemon. One loop per AIH.
pub struct MonitorService {
    db: Db,
    executor: PluginExecutor,
    running: DashMap<String, JoinHandle<()>>,
}

impl MonitorService {
    pub fn new(db: Db, executor: PluginExecutor) -> Arc<Self> {
        Arc::new(Self {
            db,
            executor,
            running: DashMap::new(),
        })
    }

    /// Start monitoring `aih`, but only if a baseline already exists (the
    /// begin / reconnect path). No-op if already running or no baseline yet.
    pub async fn ensure(self: &Arc<Self>, aih: &str) {
        if self.running.contains_key(aih) {
            return;
        }
        if matches!(self.db.last_total_tokens(aih).await, Ok(Some(_))) {
            self.spawn(aih.to_string());
        }
    }

    /// Start monitoring `aih` unconditionally (the `load_skill` first-load path).
    pub fn start(self: &Arc<Self>, aih: &str) {
        self.spawn(aih.to_string());
    }

    /// Spawn the loop for `aih` iff one isn't already running (atomic via the
    /// DashMap entry lock). The task removes itself from the registry on exit.
    fn spawn(self: &Arc<Self>, aih: String) {
        if let Entry::Vacant(slot) = self.running.entry(aih.clone()) {
            let this = self.clone();
            let handle = tokio::spawn(async move {
                this.run_loop(&aih).await;
                this.running.remove(&aih);
            });
            slot.insert(handle);
        }
    }

    async fn run_loop(&self, aih: &str) {
        // `base` is the persisted injection baseline; `seen` is the subscribe
        // cursor (advances every tick so the loop never busy-spins).
        let mut base = self.db.last_total_tokens(aih).await.ok().flatten();
        let mut seen = base;
        loop {
            let new = match self.subscribe(aih, seen).await {
                Some(Some(total)) => total,
                Some(None) => {
                    // agents_inactive — the instance is done.
                    let _ = self.db.delete(aih).await;
                    break;
                }
                None => break, // executor error / stream ended
            };
            seen = Some(new);
            let repeat = self.db.token_repeat(aih).await.ok().flatten().unwrap_or(i64::MAX);
            match self.db.skill_content(aih).await.ok().flatten() {
                // A skill is loaded and usage grew past the threshold → inject.
                Some(skill) if base.map_or(true, |b| new - b > repeat) => {
                    tokio::join!(
                        self.enqueue(aih, &skill),
                        async { let _ = self.db.set_last_total_tokens(aih, new).await; },
                    );
                    base = Some(new);
                }
                // No skill loaded → advance the baseline quietly (no injection).
                None => {
                    let _ = self.db.set_last_total_tokens(aih, new).await;
                    base = Some(new);
                }
                // Skill loaded but below threshold → keep accumulating.
                Some(_) => {}
            }
        }
    }

    /// Read the agent's current stored `total_tokens` (no waiting).
    pub async fn token_usage_get(&self, aih: &str) -> Option<i64> {
        tu_get::execute(
            &self.executor,
            tu_get::Request {
                path_type: tu_get::Path::AgentsLogsTokenUsageGet,
                agent_instance_hierarchy: aih.to_string(),
                base: Default::default(),
            },
            None,
        )
        .await
        .ok()?
        .total_tokens
    }

    /// One-shot subscribe. `Some(Some(total))` = a new snapshot,
    /// `Some(None)` = agents_inactive, `None` = executor error / no item.
    async fn subscribe(&self, aih: &str, previous: Option<i64>) -> Option<Option<i64>> {
        let mut stream = tu_subscribe::execute(
            &self.executor,
            tu_subscribe::Request {
                path_type: tu_subscribe::Path::AgentsLogsTokenUsageSubscribe,
                agent_instance_hierarchy: aih.to_string(),
                previous,
                base: Default::default(),
            },
            None,
        )
        .await
        .ok()?;
        let item = stream.next().await?.ok()?;
        Some(match item {
            tu_subscribe::ResponseItem::Item(tu) => Some(tu.total_tokens),
            tu_subscribe::ResponseItem::AgentsInactive(_) => None,
        })
    }

    /// Enqueue the loaded skill as a `<arcanum>…</arcanum>` message to `aih`.
    pub async fn enqueue(&self, aih: &str, skill_content: &str) {
        let (parent, instance) = match aih.rsplit_once('/') {
            Some((p, i)) => (Some(p.to_string()), i.to_string()),
            None => (None, aih.to_string()),
        };
        let message = format!("<arcanum>\n{skill_content}\n</arcanum>");
        let _ = enqueue::execute(
            &self.executor,
            enqueue::Request {
                path_type: enqueue::Path::AgentsEnqueue,
                agent: AgentSelector::Instance {
                    parent_agent_instance_hierarchy: parent,
                    agent_instance: instance,
                },
                message: RequestMessage::Simple(message),
                key: Some(ENQUEUE_KEY.to_string()),
                base: Default::default(),
            },
            None,
        )
        .await;
    }
}
