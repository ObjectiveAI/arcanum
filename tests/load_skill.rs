//! E2E: `load_skill` reads a laboratory's SKILL.md and injects it on first load.
//!
//! Requires podman + network + the staged `.objectiveai` host (real laboratory
//! containers). Intended for Linux/CI.

mod common;

use common::{Host, Mount, arcanum_agent_with_calls, tool_call};
use serde_json::json;

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn skills_dir(sub: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-skills")
        .join(sub)
        .to_string_lossy()
        .into_owned()
}

/// A first `load_skill` (no prior baseline) should (a) return the SKILL.md
/// content to the caller and (b) immediately enqueue it wrapped in
/// `<arcanum>…</arcanum>` for the agent — the deterministic first-load path
/// (no token growth required).
#[tokio::test(flavor = "multi_thread")]
async fn load_skill_enqueues_on_first_load() {
    let host = Host::new("load_skill_enqueues_on_first_load");
    let n = nanos();
    let lab = format!("load-{n}");
    let tag = format!("load-tag-{n}");

    host.create_lab(
        &lab,
        vec![Mount {
            host: skills_dir("lab-one"),
            container: "/skills".to_string(),
        }],
        Vec::new(),
        "/",
    )
    .await;

    let agent = arcanum_agent_with_calls(vec![tool_call(
        "arcanum_load_skill",
        json!({ "laboratory_id": lab.clone(), "path": "/skills/greeting" }),
    )]);
    host.apply_tag(&tag, agent).await;
    host.attach_lab(&tag, &lab).await;

    let (aih, response_id) = host.spawn_tag(&tag).await;
    host.wait(&aih).await;

    // (a) load_skill returns the SKILL.md content.
    let tool = host.tool_texts(&response_id).await.join("\n");
    assert!(
        tool.contains("Greeting skill"),
        "load_skill should return the SKILL.md content; got: {tool}"
    );

    // (b) the first load enqueues the skill wrapped in <arcanum>…</arcanum>.
    let msgs = host.message_texts().await.join("\n---\n");
    assert!(
        msgs.contains("<arcanum>") && msgs.contains("Greeting skill") && msgs.contains("</arcanum>"),
        "expected an <arcanum> skill injection in the queue; got: {msgs}"
    );
}
