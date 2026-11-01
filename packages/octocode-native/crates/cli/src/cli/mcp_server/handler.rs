//! The MCP handler: `tools/list` answers the composed listing and
//! `tools/call` runs the shared runtime. Validation, execution, response
//! shaping and error projection are the runtime's; this adapter only
//! carries requests in and results out.
use super::listing::Listing;
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::error::{McpResult, mcp_result};
use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use octocode_native::security::scrub_error_text;
use rmcp::ErrorData;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, MetaObject, PaginatedRequestParams, ProtocolVersion, ResultType,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The `_meta` key carrying the server's identity on 2026-07-28 responses.
const SERVER_INFO_META_KEY: &str = "io.modelcontextprotocol/serverInfo";

pub(super) struct Handler {
    runtime: Arc<ToolRuntime>,
    tools: Vec<Tool>,
    config: ServerConfig,
    next_request: AtomicU64,
}

impl Handler {
    pub(super) fn new(runtime: Arc<ToolRuntime>, listing: Listing) -> Result<Self, String> {
        let instructions = listing.instructions;
        let tools = listing
            .tools
            .into_iter()
            .map(|tool| {
                serde_json::from_value::<Tool>(tool)
                    .map_err(|error| format!("Invalid tool definition: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Server identity is interface-owned: the contract carries tool
        // guidance, not the server's name, title or version.
        // Tools never change while connected: advertise `listChanged: false`.
        let mut capabilities = ServerCapabilities::builder().enable_tools().build();
        if let Some(tools) = capabilities.tools.as_mut() {
            tools.list_changed = Some(false);
        }
        let config = ServerConfig::new(capabilities)
            .with_server_info(
                Implementation::new("octocode-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("Octocode MCP"),
            )
            .with_instructions(instructions);
        Ok(Self {
            runtime,
            tools,
            config,
            next_request: AtomicU64::new(1),
        })
    }

    /// Run one call to completion; a cancelled call is cancelled in the
    /// runtime, which unwinds its work.
    async fn run(
        &self,
        tool: &str,
        input: Value,
        cancelled: tokio_util::sync::CancellationToken,
    ) -> Result<McpResult, RuntimeError> {
        let request = format!("mcp-{}", self.next_request.fetch_add(1, Ordering::Relaxed));
        let admission = self.runtime.admit_with(request.clone(), None);
        let result = match admission {
            Ok(admission) => {
                let execution =
                    self.runtime
                        .execute(admission, tool.to_owned(), input, Channel::Mcp);
                tokio::pin!(execution);
                tokio::select! {
                    result = &mut execution => result,
                    () = cancelled.cancelled() => {
                        self.runtime.requests.cancel(&request);
                        execution.await
                    }
                }
            }
            Err(error) => Err(error),
        };
        mcp_result(tool, result)
    }
}

/// A cancelled request or a closing runtime is the only call that fails as
/// a protocol error; its message is scrubbed like every other.
fn protocol_error(tool: &str, error: &RuntimeError) -> ErrorData {
    let message = if error.code == ErrorCode::Cancelled {
        "Request cancelled".to_owned()
    } else {
        format!(
            "Tool {tool} failed to execute: {}",
            scrub_error_text(&error.message)
        )
    };
    ErrorData::internal_error(message, None)
}

impl Handler {
    /// The response `_meta` a 2026-07-28 request is owed: the server's
    /// identity, which that era carries on every response instead of in an
    /// `initialize` handshake.
    fn response_meta(&self, context: &RequestContext<RoleServer>) -> Option<MetaObject> {
        let current = context
            .protocol_version()
            .is_some_and(|version| version.as_str() >= ProtocolVersion::V_2026_07_28.as_str());
        let info = serde_json::to_value(&self.config.server_info).ok()?;
        current.then(|| {
            MetaObject(JsonObject::from_iter([(
                SERVER_INFO_META_KEY.to_owned(),
                info,
            )]))
        })
    }
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        self.config.clone()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result = ListToolsResult::with_all_items(self.tools.clone());
        result.meta = self.response_meta(&context);
        Ok(result)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // Only the enabled, individually listed tools are callable.
        if !self.tools.iter().any(|tool| tool.name == request.name) {
            return Err(ErrorData::invalid_params(
                format!("Tool {} not found", request.name),
                None,
            ));
        }
        let meta = self.response_meta(&context);
        let tool = request.name;
        let input = request.arguments.map_or(Value::Null, Value::Object);
        // A request that names a progress token is told its call's progress.
        let token = context.meta.get_progress_token();
        let McpResult {
            text,
            structured_content,
            is_error,
        } = super::progress::relayed(token, &context.peer, self.run(&tool, input, context.ct))
            .await
            .map_err(|error| protocol_error(&tool, &error))?;
        // Built from its parts: the envelope is not walked again into a
        // result value before rmcp serializes it.
        let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
        result.structured_content = Some(structured_content);
        result.is_error = Some(is_error);
        // rmcp completes list results itself; a call result carries its type
        // from here (it strips it again for a 2025-era peer).
        result.result_type = Some(ResultType::COMPLETE);
        result.meta = meta;
        Ok(result.into())
    }
}
