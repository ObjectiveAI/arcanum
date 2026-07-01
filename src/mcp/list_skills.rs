//! The `list_skills` toolset: enumerate every `SKILL.md` folder across the
//! caller's laboratories.
//!
//! arcanum can't read a laboratory's filesystem directly, so for each attached
//! laboratory (an MCP server exposing a `Bash` tool) we shell out via the plugin
//! executor's `agents mcp tools call` and run a `find`, then parse the result.

use futures::future::join_all;
use objectiveai_sdk::cli::command::agents::mcp::servers::list as servers_list;
use objectiveai_sdk::cli::command::agents::mcp::tools::call as tools_call;
use objectiveai_sdk::cli::command::plugin::PluginExecutor;
use objectiveai_sdk::laboratories::Laboratory;
use objectiveai_sdk::mcp::tool::ContentBlock;
use rmcp::{
    ErrorData, RoleServer, tool, tool_router,
    model::{CallToolResult, Content, Extensions},
    service::RequestContext,
};
use serde::{Deserialize, Serialize};

use super::ArcanumMcp;

/// Header carrying the caller's response id (scopes the executor tool calls).
const RESPONSE_ID_HEADER: &str = "x-objectiveai-response-id";

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

/// Just the `stdout` field of the laboratory Bash tool's JSON result.
#[derive(Deserialize)]
struct BashOut {
    #[serde(default)]
    stdout: String,
}

/// Read a required header off the request extensions, erroring if absent/empty.
fn required_header(extensions: &Extensions, name: &str) -> Result<String, ErrorData> {
    let parts = extensions
        .get::<http::request::Parts>()
        .ok_or_else(|| ErrorData::invalid_params("missing request parts", None))?;
    parts
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ErrorData::invalid_params(format!("missing required header: {name}"), None))
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
        let response_id = required_header(&ctx.extensions, RESPONSE_ID_HEADER)?;

        // 1. List the agent's connected MCP servers.
        let servers = servers_list::execute(
            &self.context.executor,
            servers_list::Request {
                path_type: servers_list::Path::AgentsMcpServersList,
                response_id: response_id.clone(),
                base: Default::default(),
            },
            None,
        )
        .await
        .map_err(|e| ErrorData::internal_error(format!("servers list: {e}"), None))?
        .servers;

        // 2. Keep only laboratories; capture (lab id, its Bash tool name). Tools
        //    surface through the proxy as `<server.name>_<tool>`, so the
        //    laboratory's `Bash` tool is `<server.name>_Bash`.
        let labs: Vec<(String, String)> = servers
            .into_iter()
            .filter_map(|s| match s.laboratory {
                Some(Laboratory::Client(c)) => Some((c.id, format!("{}_Bash", s.name))),
                None => None,
            })
            .collect();

        // 3. Concurrently run the find in each laboratory and collect items. A
        //    laboratory that errors or returns unparseable output contributes
        //    nothing (it's skipped, not fatal).
        let futures = labs.iter().map(|(lab_id, tool)| {
            let executor = &self.context.executor;
            let response_id = response_id.as_str();
            async move {
                let stdout = run_find(executor, response_id, tool).await?;
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

/// Call a laboratory's `Bash` tool with [`FIND_CMD`] and return its stdout, or
/// `None` on any failure (executor error, no text block, unparseable JSON).
async fn run_find(executor: &PluginExecutor, response_id: &str, tool: &str) -> Option<String> {
    let params: objectiveai_sdk::mcp::tool::CallToolRequestParams = serde_json::from_value(
        serde_json::json!({ "name": tool, "arguments": { "command": FIND_CMD } }),
    )
    .ok()?;
    let result = tools_call::execute(
        executor,
        tools_call::Request {
            path_type: tools_call::Path::AgentsMcpToolsCall,
            response_id: response_id.to_string(),
            params,
            base: Default::default(),
        },
        None,
    )
    .await
    .ok()?;
    let text = result.content.into_iter().find_map(|b| match b {
        ContentBlock::Text(t) => Some(t.text),
        _ => None,
    })?;
    let parsed: BashOut = serde_json::from_str(&text).ok()?;
    Some(parsed.stdout)
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
