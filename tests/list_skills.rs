//! E2E: `list_skills` across multiple laboratories with mounted SKILL.md folders.
//!
//! Requires podman + network (to pull the base image) and the staged
//! `.objectiveai/` host — it starts real laboratory containers. Intended for
//! Linux/CI; it can't run on a machine without podman.

mod common;

use common::{Host, Mount, arcanum_agent};
use serde_json::Value;

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// Absolute path to a committed fixture dir under `test-skills/`.
fn skills_dir(sub: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-skills")
        .join(sub)
        .to_string_lossy()
        .into_owned()
}

/// Two laboratories, each mounting a different `test-skills/<lab>` folder at
/// `/skills`. `list_skills` should return one item per SKILL.md folder, tagged
/// with the owning laboratory id — including the `greeting` folder that exists
/// in BOTH labs (duplicates kept) and the nested `deep-skill` in lab-two.
#[tokio::test(flavor = "multi_thread")]
async fn skills_listed_across_laboratories() {
    let host = Host::new("skills_listed_across_laboratories");
    let n = nanos();
    let lab_one = format!("skills-one-{n}");
    let lab_two = format!("skills-two-{n}");
    let tag = format!("skills-tag-{n}");

    host.create_lab(
        &lab_one,
        vec![Mount {
            host: skills_dir("lab-one"),
            container: "/skills".to_string(),
        }],
        Vec::new(),
        "/",
    )
    .await;
    host.create_lab(
        &lab_two,
        vec![Mount {
            host: skills_dir("lab-two"),
            container: "/skills".to_string(),
        }],
        Vec::new(),
        "/",
    )
    .await;

    host.apply_tag(&tag, arcanum_agent()).await;
    host.attach_lab(&tag, &lab_one).await;
    host.attach_lab(&tag, &lab_two).await;

    let (aih, response_id) = host.spawn_tag(&tag).await;
    host.wait(&aih).await;

    // `list_skills` returns one text block: a JSON array of { laboratory_id,
    // name, path }. Pick the first tool-result row that parses as such an array
    // (other rows may be unrelated mock tool calls).
    let texts = host.tool_texts(&response_id).await;
    let items: Vec<Value> = texts
        .iter()
        .find_map(|t| {
            let t = t.trim();
            if !t.starts_with('[') {
                return None;
            }
            serde_json::from_str::<Vec<Value>>(t)
                .ok()
                .filter(|arr| arr.iter().all(|v| v.get("laboratory_id").is_some()))
        })
        .unwrap_or_else(|| panic!("no list_skills array in tool results: {texts:?}"));

    let has = |lab: &str, name: &str, path: &str| {
        items.iter().any(|it| {
            it.get("laboratory_id").and_then(Value::as_str) == Some(lab)
                && it.get("name").and_then(Value::as_str) == Some(name)
                && it.get("path").and_then(Value::as_str) == Some(path)
        })
    };

    // lab-one: greeting, farewell.
    assert!(has(&lab_one, "greeting", "/skills/greeting"), "missing lab-one greeting: {items:?}");
    assert!(has(&lab_one, "farewell", "/skills/farewell"), "missing lab-one farewell: {items:?}");
    // lab-two: greeting (duplicate name, different lab), summarize, nested deep-skill.
    assert!(has(&lab_two, "greeting", "/skills/greeting"), "missing lab-two greeting: {items:?}");
    assert!(has(&lab_two, "summarize", "/skills/summarize"), "missing lab-two summarize: {items:?}");
    assert!(
        has(&lab_two, "deep-skill", "/skills/nested/deep-skill"),
        "missing lab-two nested deep-skill: {items:?}"
    );

    // Exactly the five SKILL.md folders across both labs (root-level excluded,
    // duplicates kept).
    assert_eq!(items.len(), 5, "unexpected skill count: {items:?}");
}
