//! Domain dispatch. Interfaces never select security or execution policy.
use crate::runtime::tool_output::ToolOutputOf;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::outcome::{DomainResult, Upstream};

use octocode_contract_types::{
    AstSearchQuery, ErrorCode, LocalFetchQuery, LocalSearchQuery, StructureSearchQuery,
};
use octocode_io::retry::classify_status;
use octocode_runtime_base::security::ContentSecurity;
use octocode_tool_ast::ast_search::execute_ast;
use octocode_tool_local::local_search::{SearchStatus, execute_local_search};
use octocode_tool_local::structure_search::execute_structure;
use serde_json::Value;

/// The runtime-owned services a local row runs against.
pub(super) struct LocalEnv<'a> {
    pub memory_only: bool,
    /// `local.graphHome`: the graph snapshot home in place of the workspace's.
    pub graph_home: Option<&'a std::path::Path>,
    /// Language-server discovery settings: where a graph build reads the
    /// user's consent to a SCIP index.
    pub lsp: Option<&'a octocode_engine::lsp::config::LspDiscoveryOptions>,
    pub paths: &'a PathPolicy,
    pub security: &'a ContentSecurity,
    pub cargo: Option<&'a str>,
    /// clasify is available: a refused image names its route there.
    pub clasify: bool,
    /// The retained rows graph continuation pages replay; `None`: they compute.
    pub graph_rows: Option<&'a std::sync::Arc<octocode_tool_graph::ast_graph::store::SavedRows>>,
}

/// Execute one local-filesystem row. The caller routes by [`ToolId`]; a tool
/// outside the local family is a routing bug, not a row error.
pub(super) fn execute_local(
    tool: ToolId,
    query: &Value,
    context: &ExecutionContext,
    env: &LocalEnv<'_>,
) -> Result<DomainResult, ExecutionError> {
    let &LocalEnv {
        memory_only,
        graph_home,
        lsp,
        paths,
        security,
        clasify,
        cargo,
        graph_rows,
    } = env;
    // Generated query types carry the meta fields, so every tool parses the
    // validated row as-is through `parse_query`.
    macro_rules! parsed {
        ($type:ty) => {
            match parse_query::<$type>(query) {
                Ok(request) => request,
                Err(row) => return Ok(*row),
            }
        };
    }
    match tool {
        ToolId::GraphBuild => {
            let request = parsed!(octocode_contract_types::GraphBuildQuery);
            octocode_tool_graph::ast_graph::store::tools::execute_build(
                &request,
                paths,
                security,
                context,
                cargo,
                &octocode_tool_graph::ast_graph::store::tools::Store {
                    memory_only,
                    home: graph_home,
                    lsp,
                },
            )
        }
        ToolId::GraphQuery => {
            let _ = parsed!(octocode_contract_types::GraphQueryQuery);
            octocode_tool_graph::ast_graph::store::tools::execute_query(
                query,
                paths,
                context,
                graph_rows.cloned(),
                graph_home,
            )
        }
        ToolId::LocalFetch => {
            let request = parsed!(LocalFetchQuery);
            let result = octocode_tool_local::local_fetch::execute_local_fetch(
                &request,
                paths,
                security,
                context,
                context.response_window,
                clasify,
            );
            let status = result.status.row_status();
            let source_digest = result.source_sha256.clone();
            let data = serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
            let mut row = DomainResult::payload(data, status);
            row.source_digest = source_digest;
            row.judged = result.judged_leaves();
            Ok(row)
        }
        ToolId::LocalSearch => {
            let request = parsed!(LocalSearchQuery);
            let (security, judged) = security.for_request();
            let result = execute_local_search(
                crate::build_identity::BUILD_IDENTITY,
                &request,
                paths,
                &security,
                context,
                context.response_window,
            );
            match result {
                Ok(result) => {
                    let status = (result.status == SearchStatus::Empty).then_some("empty");
                    let source_digest = result.source_snapshot.clone();
                    let data = result.to_row().map_err(|_| ExecutionError::WorkerFailed)?;
                    let mut row = DomainResult::payload(data, status);
                    row.source_digest = source_digest;
                    row.judged = judged.leaves();
                    Ok(row)
                }
                Err(error) => Ok(error.into()),
            }
        }
        ToolId::StructureSearch => {
            let request = parsed!(StructureSearchQuery);
            Ok(
                match execute_structure(&request, paths, security, context, context.response_window)
                {
                    Ok(data) => value_result(data),
                    Err(error) => error.into(),
                },
            )
        }
        ToolId::AstSearch => {
            let request = parsed!(AstSearchQuery);
            let (security, judged) = security.for_request();
            Ok(
                match execute_ast(
                    crate::build_identity::BUILD_IDENTITY,
                    &request,
                    paths,
                    &security,
                    context,
                    context.response_window,
                ) {
                    Ok(data) => {
                        let mut row = value_result(data);
                        row.judged = judged.leaves();
                        row
                    }
                    Err(error) => error.into(),
                },
            )
        }
        ToolId::GhSearchRepo
        | ToolId::GhSearchCode
        | ToolId::GhStructure
        | ToolId::GhGetFileContent
        | ToolId::GhSearchHistory
        | ToolId::GhGetHistoryItem
        | ToolId::GhCloneRepo
        | ToolId::ArtifactSearch
        | ToolId::SecurityCheck
        | ToolId::LspSearch
        | ToolId::Clasify => Err(ExecutionError::UnroutedTool),
    }
}

/// Contract validation already passed, so a typed-parse failure means core and
/// native disagree on a field's shape (e.g. a fractional offset). Report it on
/// this row instead of failing every row in the batch.
pub(super) fn parse_query<'de, T: serde::Deserialize<'de>>(
    query: impl serde::Deserializer<'de, Error = serde_json::Error>,
) -> Result<T, Box<DomainResult>> {
    T::deserialize(query).map_err(|error| Box::new(invalid_query(&error)))
}

pub(super) fn invalid_query(error: &serde_json::Error) -> DomainResult {
    DomainResult::failure(
        ErrorCode::InvalidInput,
        "Check the query fields.",
        vec![format!("Query does not match the runtime type: {error}.")],
        None,
    )
}

/// A tool's [`octocode_runtime_base::response::result::ToolData`] (a bare `Value` converts):
/// its payload as [`value_result`], keeping the typed notes of what its pages
/// leave to read.
pub(crate) fn data_result(
    data: impl Into<octocode_runtime_base::response::result::ToolData>,
) -> DomainResult {
    value_result(data.into().data)
}

/// A tool payload whose own `status` field selects the row status.
pub(crate) fn value_result(data: Value) -> DomainResult {
    let status = match data.get("status").and_then(Value::as_str) {
        Some("error") => Some("error"),
        Some("empty") => Some("empty"),
        _ => None,
    };
    DomainResult::payload(data, status)
}

/// A failure a retry can fix (throttling, an outage, a timeout, a dropped
/// connection) offers `next.retry`: the same row (a page, so an exact
/// replay) with the provider's delay when it named one. A page keeps a batch
/// that ran other rows incomplete.
pub(super) fn offer_retry(tool: ToolId, query: &Value, data: &mut Value) {
    if data["retryable"] != serde_json::json!(true) {
        return;
    }
    let retry =
        octocode_runtime_base::response::result::Continuation::continued(tool, query.clone())
            .build();
    octocode_runtime_base::response::result::set_next(data, "retry", retry);
}

/// A registry or language-server failure row of `tool`, whose own recovery
/// ([`octocode_runtime_base::response::output::ToolOutput::error_hint`]) decides a shared
/// code. `retryable` is the emitter's own verdict when it has one, else
/// derived from the code and status.
pub(super) fn provider_failure(
    tool: ToolId,
    message: String,
    code: ErrorCode,
    mut hints: Vec<String>,
    http_status: Option<u16>,
    retryable: Option<bool>,
) -> DomainResult {
    let retryable = retryable.unwrap_or_else(|| {
        matches!(code, ErrorCode::Timeout | ErrorCode::ProviderError)
            || http_status.is_some_and(|status| classify_status(status).is_retryable())
    });
    // An invalid input's message names the rejected field or the registry's
    // own reason; a static hint would only restate it.
    // Only a registry's own recoveries are written here; every other code
    // takes the shared recovery table's one wording (a retryable HTTP status
    // reads as the transient failure it is).
    if hints.is_empty() && code != ErrorCode::InvalidInput {
        let own = match code {
            ErrorCode::Authentication | ErrorCode::PermissionDenied => Some(
                "Registry denied access: verify the registry credentials and access, then retry.",
            ),
            ErrorCode::CapabilityUnavailable => {
                Some("Use the exact package lookup supported by this ecosystem.")
            }
            _ => None,
        };
        let shared = || {
            let situation = if retryable && code == ErrorCode::HttpStatus {
                ErrorCode::Transport
            } else {
                code
            };
            octocode_runtime_base::response::rows::error_code_hint(
                tool.output(),
                situation,
                &Value::Null,
            )
        };
        hints.extend(
            own.or_else(shared)
                .filter(|hint| !hint.is_empty())
                .map(str::to_owned),
        );
    }
    // The upstream HTTP status distinguishes e.g. a registry 404 from a 429
    // without parsing prose (absent when the upstream never answered).
    DomainResult::provider(
        code,
        message,
        hints,
        None,
        Upstream {
            retryable,
            http_status,
            rate_limit: None,
        },
    )
}

#[cfg(test)]
mod provider_failure_tests;
