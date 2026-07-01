//! The `list_skills` toolset: enumerate every `SKILL.md` folder across the
//! caller's laboratories.
//!
//! arcanum can't read a laboratory's filesystem directly, so for each attached
//! laboratory (an MCP server exposing a `Bash` tool) we shell out via the plugin
//! executor's `agents mcp tools call` and run a `find`, then parse the result.

use futures::future::join_all;
use rmcp::{
    ErrorData, RoleServer, tool, tool_router,
    model::{CallToolResult, Content},
    service::RequestContext,
};
use serde::Serialize;

use super::ArcanumMcp;
use super::common::{self, RESPONSE_ID_HEADER};

/// Bash command run inside each laboratory to locate every `SKILL.md`
/// (case-insensitive), pruning pseudo-filesystems for speed.
const FIND_CMD: &str = "find / \\( -path /proc -o -path /sys -o -path /dev \\) -prune -o -type f -iname 'SKILL.md' -print 2>/dev/null";

/// One discovered skill: which laboratory it lives in, the folder name (the
/// skill's name), and that folder's path within the laboratory.
#[derive(Serialize)]
struct SkillItem {
    laboratory_id: String,
    name: String,
    path: String,
}

#[tool_router(router = list_skills_tools, vis = "pub")]
impl ArcanumMcp {
    #[tool(
        name = "list_skills",
        description = "List all skills (SKILL.md folders) across the agent's laboratories."
    )]
    async fn list_skills(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let response_id = common::required_header(&ctx.extensions, RESPONSE_ID_HEADER)?;

        // Laboratories the agent is connected to, and each one's `Bash` tool name.
        let servers = common::list_servers(&self.context.executor, &response_id).await?;
        let labs: Vec<(String, String)> = servers
            .iter()
            .filter_map(|s| common::laboratory_id(s).map(|id| (id.to_string(), common::bash_tool(s))))
            .collect();

        // Concurrently run the find in each laboratory and collect items. A
        // laboratory that errors or returns unparseable output contributes
        // nothing (it's skipped, not fatal).
        let futures = labs.iter().map(|(lab_id, tool)| {
            let executor = &self.context.executor;
            let response_id = response_id.as_str();
            async move {
                let stdout = common::lab_bash(executor, response_id, tool, FIND_CMD).await?;
                Some(parse_skill_paths(lab_id, &stdout))
            }
        });
        let items: Vec<SkillItem> = join_all(futures)
            .await
            .into_iter()
            .flatten() // drop skipped laboratories (None)
            .flatten() // flatten each lab's Vec<SkillItem>
            .collect();

        let body = serde_json::to_string(&items)
            .map_err(|e| ErrorData::internal_error(format!("serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }
}

/// Parse `find` stdout into skill items. Each line is a `SKILL.md` path; the
/// skill name is its containing folder and the path is that folder. A `SKILL.md`
/// at the filesystem root (`/SKILL.md`) is excluded. Duplicates are kept.
///
/// Split on `/` manually rather than via `std::path` so paths are parsed with
/// Linux semantics even when arcanum runs on a Windows host.
fn parse_skill_paths(lab_id: &str, stdout: &str) -> Vec<SkillItem> {
    let mut items = Vec::new();
    for line in stdout.lines() {
        let p = line.trim();
        if p.is_empty() {
            continue;
        }
        let i = match p.rfind('/') {
            // Root-level `/SKILL.md` (slash at index 0) or a relative bare name.
            Some(0) | None => continue,
            Some(i) => i,
        };
        let dir = &p[..i];
        let name = dir.rsplit('/').next().unwrap_or("");
        if dir.is_empty() || name.is_empty() {
            continue;
        }
        items.push(SkillItem {
            laboratory_id: lab_id.to_string(),
            name: name.to_string(),
            path: dir.to_string(),
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_and_excludes_root() {
        let out = "/a/b/SKILL.md\n/x/skill.md\n/SKILL.md\nSKILL.md\n\n";
        let items = parse_skill_paths("lab1", out);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "b");
        assert_eq!(items[0].path, "/a/b");
        assert_eq!(items[0].laboratory_id, "lab1");
        assert_eq!(items[1].name, "x"); // case-insensitive filename still counts
        assert_eq!(items[1].path, "/x");
    }

    #[test]
    fn keeps_duplicates() {
        let out = "/a/b/SKILL.md\n/a/b/SKILL.md\n";
        assert_eq!(parse_skill_paths("lab", out).len(), 2);
    }
}
