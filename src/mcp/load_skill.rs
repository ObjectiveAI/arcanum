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

        // Read the skill's SKILL.md over the laboratory connection.
        let content = common::read_skill_md(
            &self.context.executor,
            &response_id,
            &req.laboratory_id,
            &req.path,
        )
        .await
        .ok_or_else(|| {
            ErrorData::invalid_params(
                format!("no SKILL.md at {} in laboratory {}", req.path, req.laboratory_id),
                None,
            )
        })?;

        // Register the loaded skill's reference + this response id (the monitor
        // re-reads the content fresh on each injection). On the FIRST load (no
        // baseline yet) inject immediately and establish the baseline, then start
        // the monitor loop.
        let db = self
            .context
            .db()
            .await
            .map_err(|e| ErrorData::internal_error(format!("db: {e}"), None))?;
        let had_baseline = db.last_total_tokens(&aih).await.ok().flatten().is_some();
        db.set_skill(&aih, &req.laboratory_id, &req.path, &response_id)
            .await
            .map_err(|e| ErrorData::internal_error(format!("db: {e}"), None))?;

        if !had_baseline {
            self.monitor.enqueue(&aih, &content).await;
            let baseline = self.monitor.token_usage_get(&aih).await.unwrap_or(0);
            let _ = db.set_last_total_tokens(&aih, baseline).await;
        }
        self.monitor.start(&aih, token_repeat);

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }
}
