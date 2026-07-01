//! The `load_skill` toolset: read a laboratory's `SKILL.md`, register it as the
//! agent's loaded skill, inject it immediately, and start token-usage monitoring
//! so it gets re-injected as the agent's context grows.

use rmcp::{
    ErrorData, RoleServer, tool, tool_router,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    service::RequestContext,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::ArcanumMcp;
use super::common::{self, AIH_HEADER, RESPONSE_ID_HEADER};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoadSkillRequest {
    /// The laboratory id (from `list_skills`) that contains the skill.
    pub laboratory_id: String,
    /// The skill folder path within the laboratory (from `list_skills`), e.g.
    /// `/skills/greeting`. Its `SKILL.md` is read and loaded.
    pub path: String,
}

#[tool_router(router = load_skill_tools, vis = "pub")]
impl ArcanumMcp {
    #[tool(
        name = "load_skill",
        description = "Load a skill by its laboratory id and path (from list_skills): reads its SKILL.md and keeps it re-injected into your context as you work."
    )]
    async fn load_skill(
        &self,
        Parameters(req): Parameters<LoadSkillRequest>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let response_id = common::required_header(&ctx.extensions, RESPONSE_ID_HEADER)?;
        let aih = common::required_header(&ctx.extensions, AIH_HEADER)?;
        let token_repeat = common::token_repeat(&ctx.extensions)? as i64;

        // Resolve the laboratory's MCP server, then read its SKILL.md.
        let servers = common::list_servers(&self.context.executor, &response_id).await?;
        let server = servers
            .iter()
            .find(|s| common::laboratory_id(s) == Some(req.laboratory_id.as_str()))
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("no laboratory with id {}", req.laboratory_id),
                    None,
                )
            })?;
        let bash = common::bash_tool(server);
        // Locate the SKILL.md (case-insensitive) directly under `path` and cat it.
        let command = format!(
            "f=$(find {path} -maxdepth 1 -iname 'SKILL.md' 2>/dev/null | head -1); [ -n \"$f\" ] && cat \"$f\"",
            path = shell_single_quote(&req.path),
        );
        let content = common::lab_bash(&self.context.executor, &response_id, &bash, &command)
            .await
            .ok_or_else(|| {
                ErrorData::internal_error("failed to read SKILL.md over the laboratory", None)
            })?;
        let content = content.trim_end_matches('\n').to_string();
        if content.is_empty() {
            return Err(ErrorData::invalid_params(
                format!("no SKILL.md found at {}", req.path),
                None,
            ));
        }

        // Register the loaded skill; on the FIRST load (no baseline yet) inject
        // immediately and establish the baseline, then start the monitor loop.
        let db = self
            .context
            .db()
            .await
            .map_err(|e| ErrorData::internal_error(format!("db: {e}"), None))?;
        let had_baseline = db.last_total_tokens(&aih).await.ok().flatten().is_some();
        db.set_skill_content(&aih, token_repeat, &content)
            .await
            .map_err(|e| ErrorData::internal_error(format!("db: {e}"), None))?;

        if !had_baseline {
            self.monitor.enqueue(&aih, &content).await;
            let baseline = self.monitor.token_usage_get(&aih).await.unwrap_or(0);
            let _ = db.set_last_total_tokens(&aih, baseline).await;
        }
        self.monitor.start(&aih);

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }
}

/// Single-quote a string for safe embedding in a bash command.
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
