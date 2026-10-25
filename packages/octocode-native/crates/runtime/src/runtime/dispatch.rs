//! Domain dispatch. Interfaces never select security or execution policy.
use super::FailureKind;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::outcome::{DomainResult, Upstream};
use crate::response::result::failure_kind;
use crate::security::ContentSecurity;
use crate::tools::ast_search::execute_ast;
use crate::tools::local_search::{SearchStatus, execute_local_search};
use crate::tools::structure_search::execute_structure;
use octocode_contract_types::{
    AstSearchQuery, ErrorCode, LocalFetchQuery, LocalSearchQuery, StructureSearchQuery,
};
use octocode_io::retry::classify_status;
use serde_json::Value;

/// The runtime-owned services a local row runs against.
pub(super) struct LocalEnv<'a> {
    pub paths: &'a PathPolicy,
    pub security: &'a ContentSecurity,
    pub views: &'a crate::security::scan::SanitizedViewMemo,
    pub cargo: Option<&'a str>,
    pub selection: Option<&'a crate::tools::selection::Runtime>,
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
        paths,
        security,
        views,
        selection,
        cargo,
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
        ToolId::GraphBuild | ToolId::GraphQuery => {
            if tool == ToolId::GraphBuild {
                let _ = parsed!(octocode_contract_types::GraphBuildQuery);
            } else {
                let _ = parsed!(octocode_contract_types::GraphQueryQuery);
            }
            crate::tools::ast_graph::store::tools::execute(
                tool, query, paths, security, context, cargo,
            )
        }
        ToolId::LocalFetch => {
            let request = parsed!(LocalFetchQuery);
            let scan = crate::security::scan::MemoizedScan::new(security, views);
            let result = crate::tools::local_fetch::execute_local_fetch(
                &request,
                paths,
                &scan,
                context,
                context.response_window,
                selection,
            );
            let status = result.status.row_status();
            let missing = result.resource_missing;
            let source_digest = result.source_sha256.clone();
            let data = serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
            let mut row = DomainResult::payload(data, status);
            row.source_digest = source_digest;
            if missing && row.failure.is_some() {
                row.failure = Some(FailureKind::NotFound);
            }
            Ok(row)
        }
        ToolId::LocalSearch => {
            let request = parsed!(LocalSearchQuery);
            let result = execute_local_search(
                &request,
                paths,
                security,
                context,
                context.walk_threads,
                context.response_window,
                selection,
            );
            match result {
                Ok(result) => {
                    let status = (result.status == SearchStatus::Empty).then_some("empty");
                    let source_digest = result.source_snapshot.clone();
                    let data =
                        serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
                    let mut row = DomainResult::payload(data, status);
                    row.source_digest = source_digest;
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
            Ok(
                match execute_ast(&request, paths, security, context, context.response_window) {
                    Ok(data) => value_result(data),
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

/// A tool's [`crate::response::result::ToolData`] (a bare `Value` converts):
/// its payload as [`value_result`], keeping the typed notes of what its pages
/// leave to read.
pub(crate) fn data_result(data: impl Into<crate::response::result::ToolData>) -> DomainResult {
    let data = data.into();
    let mut result = value_result(data.data);
    result.remaining = data.remaining;
    result
}

/// A tool payload whose own `status` field selects the row status.
pub(crate) fn value_result(data: Value) -> DomainResult {
    let status = match data.get("status").and_then(Value::as_str) {
        Some("error") => Some("error"),
        Some("empty") => Some("empty"),
        _ => None,
    };
    let kind = crate::id::error_code_of(&data).map(failure_kind);
    let mut result = DomainResult::payload(data, status);
    if result.failure.is_some()
        && let Some(kind) = kind
    {
        result.failure = Some(kind);
    }
    result
}

/// A failure a retry can fix (throttling, an outage, a timeout, a dropped
/// connection) offers `next.retry`: the same row (a page, so an exact
/// replay) with the provider's delay when it named one. A page keeps a batch
/// that ran other rows incomplete.
pub(super) fn offer_retry(tool: ToolId, query: &Value, data: &mut Value) {
    if data["retryable"] != serde_json::json!(true) {
        return;
    }
    let retry = crate::response::result::Continuation::continued(tool, query.clone()).build();
    data["next"]["retry"] = retry;
}

/// A registry or language-server failure row. `retryable` is the emitter's
/// own verdict when it has one, else derived from the code and status.
pub(super) fn provider_failure(
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
    if hints.is_empty() && code != ErrorCode::InvalidInput {
        hints.push(
            match code {
                ErrorCode::Authentication | ErrorCode::Permission => {
                    "Verify registry credentials and access, then retry."
                }
                ErrorCode::RateLimited => "Wait for the provider rate-limit reset before retrying.",
                ErrorCode::Timeout => "Retry once; if it persists, verify registry availability.",
                ErrorCode::CapabilityUnavailable => {
                    "Use the exact package lookup supported by this ecosystem."
                }
                _ if retryable => "Retry once; if it persists, verify registry availability.",
                _ => "Verify provider configuration and the requested package coordinate.",
            }
            .into(),
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
