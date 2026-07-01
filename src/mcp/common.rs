//! Shared helpers for the MCP tools: request headers, per-session arguments, and
//! invoking a laboratory's `Bash` tool through the plugin executor.

use indexmap::IndexMap;
use objectiveai_sdk::cli::command::agents::mcp::servers::list as servers_list;
use objectiveai_sdk::cli::command::agents::mcp::tools::call as tools_call;
use objectiveai_sdk::cli::command::plugin::PluginExecutor;
use objectiveai_sdk::laboratories::Laboratory;
use objectiveai_sdk::mcp::server::Server;
use objectiveai_sdk::mcp::tool::{CallToolRequestParams, ContentBlock};
use rmcp::{ErrorData, model::Extensions};

/// Header carrying the caller's response id (scopes the executor tool calls).
pub const RESPONSE_ID_HEADER: &str = "x-objectiveai-response-id";
/// Header carrying the caller's agent instance hierarchy.
pub const AIH_HEADER: &str = "x-objectiveai-agent-instance-hierarchy";
/// Header carrying the per-session arguments JSON (e.g. `token-repeat`).
pub const ARGUMENTS_HEADER: &str = "x-objectiveai-arguments";

/// Read a required header off the request extensions, erroring if absent/empty.
pub fn required_header(extensions: &Extensions, name: &str) -> Result<String, ErrorData> {
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

/// Parse `token-repeat` (u64) from the `x-objectiveai-arguments` JSON header.
/// The host encodes argument values as strings, so accept a number or a string.
pub fn token_repeat(extensions: &Extensions) -> Result<u64, ErrorData> {
    let raw = required_header(extensions, ARGUMENTS_HEADER)?;
    parse_token_repeat(&raw)
        .ok_or_else(|| ErrorData::invalid_params("token-repeat must be a u64", None))
}

/// Pull `token-repeat` out of the arguments JSON object (number or string form).
fn parse_token_repeat(raw: &str) -> Option<u64> {
    let args: serde_json::Value = serde_json::from_str(raw).ok()?;
    match args.get("token-repeat")? {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_token_repeat;

    #[test]
    fn token_repeat_accepts_string_and_number() {
        assert_eq!(parse_token_repeat(r#"{"token-repeat":"5000"}"#), Some(5000));
        assert_eq!(parse_token_repeat(r#"{"token-repeat":5000}"#), Some(5000));
    }

    #[test]
    fn token_repeat_rejects_missing_or_bad() {
        assert_eq!(parse_token_repeat(r#"{}"#), None);
        assert_eq!(parse_token_repeat(r#"{"token-repeat":"abc"}"#), None);
        assert_eq!(parse_token_repeat(r#"not json"#), None);
    }
}

/// List the agent's connected MCP servers (scoped by response id).
pub async fn list_servers(
    executor: &PluginExecutor,
    response_id: &str,
) -> Result<Vec<Server>, ErrorData> {
    let result = servers_list::execute(
        executor,
        servers_list::Request {
            path_type: servers_list::Path::AgentsMcpServersList,
            response_id: response_id.to_string(),
            base: Default::default(),
        },
        None,
    )
    .await
    .map_err(|e| ErrorData::internal_error(format!("servers list: {e}"), None))?;
    Ok(result.servers)
}

/// The laboratory id of a server, if it is a client laboratory.
pub fn laboratory_id(server: &Server) -> Option<&str> {
    match &server.laboratory {
        Some(Laboratory::Client(c)) => Some(c.id.as_str()),
        None => None,
    }
}

/// The `Bash` tool name for a server. Tools surface through the proxy as
/// `<server.name>_<tool>`, so the laboratory's `Bash` tool is `<server.name>_Bash`.
pub fn bash_tool(server: &Server) -> String {
    format!("{}_Bash", server.name)
}

/// Just the `stdout` field of the laboratory `Bash` tool's JSON result.
#[derive(serde::Deserialize)]
struct BashOut {
    #[serde(default)]
    stdout: String,
}

/// Run `command` in a laboratory via its `Bash` tool (named `tool`); return
/// stdout, or `None` on any failure (executor error, no text block, unparseable
/// JSON).
pub async fn lab_bash(
    executor: &PluginExecutor,
    response_id: &str,
    tool: &str,
    command: &str,
) -> Option<String> {
    let params = CallToolRequestParams {
        name: tool.to_string(),
        arguments: Some(IndexMap::from([(
            "command".to_string(),
            serde_json::Value::String(command.to_string()),
        )])),
        _meta: None,
        task: None,
    };
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
