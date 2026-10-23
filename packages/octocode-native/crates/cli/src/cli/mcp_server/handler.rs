//! The MCP handler: `tools/list` answers the composed listing and
//! `tools/call` runs the shared runtime. Validation, execution, response
//! shaping and error projection are the runtime's; this adapter only
//! carries requests in and results out.
use super::listing::Listing;
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::error::mcp_result;
use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use octocode_native::security::scrub_error_text;
use rmcp::ErrorData;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListToolsResult, MetaObject, PaginatedRequestParams, ProtocolVersion, ResultType,
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
    /// The tool that runs deferred tools: `{tool, query}`, verbatim.
    dispatcher: Option<String>,
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
            dispatcher: listing.dispatcher,
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
    ) -> Result<Value, RuntimeError> {
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

    /// The tool and query a listed tool name runs: itself with its
    /// arguments, or, for the dispatcher, the `{tool, query}` it names,
    /// verbatim.
    fn target(&self, name: &str, arguments: Option<JsonObject>) -> (String, Value) {
        if self.dispatcher.as_deref() != Some(name) {
            return (
                name.to_owned(),
                arguments.map_or(Value::Null, Value::Object),
            );
        }
        let mut arguments = arguments.unwrap_or_default();
        let tool = match arguments.remove("tool") {
            Some(Value::String(tool)) => tool,
            Some(other) => other.to_string(),
            None => String::new(),
        };
        (tool, arguments.remove("query").unwrap_or(Value::Null))
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
        // Only listed tools are callable; a deferred tool runs through the
        // dispatcher, as a client that never saw it listed would have to.
        if !self.tools.iter().any(|tool| tool.name == request.name) {
            return Err(ErrorData::invalid_params(
                format!("Tool {} not found", request.name),
                None,
            ));
        }
        let meta = self.response_meta(&context);
        let (tool, input) = self.target(&request.name, request.arguments);
        let response = self
            .run(&tool, input, context.ct)
            .await
            .map_err(|error| protocol_error(&tool, &error))?;
        let mut result: CallToolResult = serde_json::from_value(response)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        // rmcp completes list results itself; a call result carries its type
        // from here (it strips it again for a 2025-era peer).
        result.result_type = Some(ResultType::COMPLETE);
        result.meta = meta;
        Ok(result.into())
    }
}
