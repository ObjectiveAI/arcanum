//! The `hello` toolset: a single hello-world tool demonstrating the
//! `#[tool_router]` / `#[tool]` / `Parameters` wiring.

use rmcp::{
    ErrorData, tool, tool_router,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::ArcanumMcp;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HelloRequest {
    /// Who to greet. Defaults to "world".
    pub name: Option<String>,
}

#[tool_router(router = hello_tools, vis = "pub")]
impl ArcanumMcp {
    #[tool(name = "hello", description = "A hello-world tool; returns a greeting.")]
    async fn hello(
        &self,
        Parameters(req): Parameters<HelloRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = req.name.as_deref().unwrap_or("world");
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Hello, {who}!"
        ))]))
    }
}
