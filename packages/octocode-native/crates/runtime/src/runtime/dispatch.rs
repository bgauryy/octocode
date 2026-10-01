//! Domain dispatch. Interfaces never select security or execution policy.
use super::{ExecutionContext, ExecutionError, FailureKind};
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::{AstTopologyQuery, execute_topology};
use crate::tools::ast_rewrite::{
    AstRewriteRuntimeOptions, RewriteRequest, execute_ast_rewrite_with_options,
};
use crate::tools::ast_search::{AstSearchQuery, execute_ast};
use crate::tools::id::ToolId;
use crate::tools::local_fetch::{LocalFetchQuery, LocalFetchRegex, execute_local_fetch_with_regex};
use crate::tools::local_search::{LocalSearchQuery, SearchStatus, execute_local_search};
use crate::tools::structure_search::{StructureSearchQuery, execute_structure};
use serde_json::{Value, json};

pub(super) struct DomainResult {
    pub diagnostics: crate::tools::result::ToolDiagnostics,
    pub data: Value,
    pub cache: bool,
    pub status: Option<&'static str>,
    pub source_digest: Option<String>,
    pub failure: Option<FailureKind>,
}

impl DomainResult {
    /// A tool's own payload. A payload that reports `status: "error"` is an
    /// execution failure, the same rule for every tool.
    pub(super) fn payload(data: Value, status: Option<&'static str>) -> Self {
        Self {
            diagnostics: Default::default(),
            cache: false,
            data,
            status,
            source_digest: None,
            failure: (status == Some("error")).then_some(FailureKind::Execution),
        }
    }

    /// The one error row: `error`, `errorCode`, then `hints` (when any) and
    /// `next` (when any). Arms add context fields to `data` afterwards.
    pub(super) fn failure(
        code: impl Into<String>,
        message: impl Into<String>,
        hints: Vec<String>,
        next: Option<Value>,
        kind: FailureKind,
    ) -> Self {
        let mut data = json!({"error": message.into(), "errorCode": code.into()});
        if !hints.is_empty() {
            data["hints"] = json!(hints);
        }
        if let Some(next) = next {
            data["next"] = next;
        }
        Self {
            diagnostics: Default::default(),
            cache: false,
            data,
            status: Some("error"),
            source_digest: None,
            failure: Some(kind),
        }
    }
}

/// Execute one local-filesystem row. The caller routes by [`ToolId`]; a tool
/// outside the local family is a routing bug, not a row error.
pub(super) fn execute_local(
    tool: ToolId,
    query: &Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    regex: &LocalFetchRegex,
    views: &crate::security::scan::SanitizedViewMemo,
    allow_ast_rewrite_apply: bool,
) -> Result<DomainResult, ExecutionError> {
    // Generated query types carry the meta fields, so every tool parses the
    // validated row as-is through `parse_query`.
    macro_rules! parsed {
        ($type:ty) => {
            match parse_query::<$type>(query.clone()) {
                Ok(request) => request,
                Err(row) => return Ok(*row),
            }
        };
    }
    match tool {
        ToolId::LocalFetch => {
            let request = parsed!(LocalFetchQuery);
            let scan = crate::security::scan::MemoizedScan::new(security, views);
            let result = execute_local_fetch_with_regex(&request, paths, &scan, context, regex);
            let status = match result.status.as_str() {
                "error" => Some("error"),
                "empty" => Some("empty"),
                _ => None,
            };
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
            match execute_local_search(&request, paths, security, context, context.walk_threads) {
                Ok(mut result) => {
                    for file in &mut result.files {
                        file.path = result
                            .source_root
                            .join(&file.path)
                            .to_string_lossy()
                            .into_owned();
                    }
                    let status = (result.status == SearchStatus::Empty).then_some("empty");
                    let source_digest = result.source_snapshot.clone();
                    let data =
                        serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
                    let mut row = DomainResult::payload(data, status);
                    row.source_digest = source_digest;
                    Ok(row)
                }
                Err(error) => Ok(DomainResult::failure(
                    error.code,
                    error.message,
                    error.hints,
                    error.next.map(|next| *next),
                    error_failure(error.code),
                )),
            }
        }
        ToolId::StructureSearch => {
            let request = parsed!(StructureSearchQuery);
            Ok(
                match execute_structure(&request, paths, security, context) {
                    Ok(data) => value_result(data),
                    Err(error) => {
                        let kind = error_failure(&error.code);
                        DomainResult::failure(error.code, error.message, Vec::new(), None, kind)
                    }
                },
            )
        }
        ToolId::AstSearch => {
            let request = parsed!(AstSearchQuery);
            Ok(match execute_ast(&request, paths, security, context) {
                Ok(data) => value_result(data),
                Err(error) => {
                    let kind = error_failure(&error.code);
                    DomainResult::failure(
                        error.code,
                        error.message,
                        error.hints,
                        error.next.map(|next| *next),
                        kind,
                    )
                }
            })
        }
        ToolId::AstTopology => {
            let request = parsed!(AstTopologyQuery);
            Ok(match execute_topology(&request, paths, security, context) {
                Ok(data) => value_result(data),
                Err(error) => {
                    let kind = error_failure(&error.code);
                    DomainResult::failure(
                        error.code,
                        error.message,
                        error.hints,
                        error.next.map(|next| *next),
                        kind,
                    )
                }
            })
        }
        ToolId::AstRewrite => {
            let request = parsed!(RewriteRequest);
            let data = execute_ast_rewrite_with_options(
                request,
                paths,
                security,
                context,
                &AstRewriteRuntimeOptions {
                    allow_apply: allow_ast_rewrite_apply,
                    ..Default::default()
                },
            );
            Ok(value_result(data))
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
        | ToolId::Clasify => Err(ExecutionError::WorkerFailed),
    }
}

/// Contract validation already passed, so a typed-parse failure means core and
/// native disagree on a field's shape (e.g. a fractional offset). Report it on
/// this row instead of failing every row in the batch.
pub(super) fn parse_query<T: serde::de::DeserializeOwned>(
    value: Value,
) -> Result<T, Box<DomainResult>> {
    serde_json::from_value(value).map_err(|error| Box::new(invalid_query(&error)))
}

pub(super) fn invalid_query(error: &serde_json::Error) -> DomainResult {
    let mut row = DomainResult::failure(
        "invalidInput",
        "Check the query fields.",
        vec![format!("Query does not match the runtime type: {error}.")],
        None,
        FailureKind::Execution,
    );
    row.data["retryable"] = json!(false);
    row
}

/// A tool payload whose own `status` field selects the row status.
pub(super) fn value_result(data: Value) -> DomainResult {
    let status = match data.get("status").and_then(Value::as_str) {
        Some("error") => Some("error"),
        Some("empty") => Some("empty"),
        _ => None,
    };
    let kind = data
        .get("errorCode")
        .and_then(Value::as_str)
        .map(error_failure);
    let mut result = DomainResult::payload(data, status);
    if result.failure.is_some()
        && let Some(kind) = kind
    {
        result.failure = Some(kind);
    }
    result
}

pub(super) fn provider_failure(
    message: String,
    code: String,
    mut hints: Vec<String>,
    http_status: Option<u16>,
) -> DomainResult {
    let retryable = matches!(code.as_str(), "timeout" | "provider_error")
        || http_status.is_some_and(|status| status == 408 || status == 429 || status >= 500);
    if hints.is_empty() {
        hints.push(
            match code.as_str() {
                "authentication" => "Verify registry credentials and access, then retry.",
                "rate_limit" => "Wait for the provider rate-limit reset before retrying.",
                "timeout" => "Retry once; if it persists, verify registry availability.",
                "invalid_query" => "Correct the package coordinate or query fields.",
                "unsupported_capability" => {
                    "Use the exact package lookup supported by this ecosystem."
                }
                _ if retryable => "Retry once; if it persists, verify registry availability.",
                _ => "Verify provider configuration and the requested package coordinate.",
            }
            .into(),
        );
    }
    let kind = error_failure(&code);
    let mut row = DomainResult::failure(code, message, hints, None, kind);
    row.data["retryable"] = json!(retryable);
    // Structured callers need the upstream HTTP status to distinguish e.g. a
    // registry 404 from a 429 without parsing prose (optional — absence is
    // valid).
    if let Some(status) = http_status {
        row.data["httpStatus"] = json!(status);
    }
    row
}

/// A missing local path, registry package, or unresolved LSP anchor is
/// not-found (like a GitHub 404), not an execution failure; every other
/// domain error stays an execution failure.
fn error_failure(code: &str) -> FailureKind {
    if matches!(code, "notFound" | "versionNotFound" | "lsp.anchorUnresolved")
        || super::response::is_not_found_code(code)
    {
        FailureKind::NotFound
    } else {
        FailureKind::Execution
    }
}

#[cfg(test)]
mod provider_failure_tests {
    use super::*;

    #[test]
    fn missing_local_path_is_not_found_other_errors_are_execution() {
        assert_eq!(
            error_failure("structure.policy.notFound"),
            FailureKind::NotFound
        );
        assert_eq!(error_failure("pathNotFound"), FailureKind::NotFound);
        assert_eq!(error_failure("notFound"), FailureKind::NotFound);
        assert_eq!(error_failure("versionNotFound"), FailureKind::NotFound);
        assert_eq!(error_failure("fileAccessFailed"), FailureKind::Execution);
    }

    #[test]
    fn tool_error_rows_take_their_failure_kind_from_the_error_code() {
        let unresolved = value_result(json!({"status":"error","errorCode":"lsp.anchorUnresolved"}));
        assert_eq!(unresolved.failure, Some(FailureKind::NotFound));
        let missing =
            value_result(json!({"status":"error","errorCode":"structure.policy.notFound"}));
        assert_eq!(missing.failure, Some(FailureKind::NotFound));
        let other = value_result(json!({"status":"error","errorCode":"lsp.serverUnavailable"}));
        assert_eq!(other.failure, Some(FailureKind::Execution));
        assert_eq!(
            value_result(json!({"status":"error"})).failure,
            Some(FailureKind::Execution)
        );
    }

    #[test]
    fn typed_parse_failure_is_a_row_error_not_a_worker_failure() {
        #[derive(serde::Deserialize, Debug)]
        struct Offset {
            #[allow(dead_code)]
            offset: usize,
        }
        let row = parse_query::<Offset>(json!({"offset": 1.5})).expect_err("fraction");
        assert_eq!(row.status, Some("error"));
        assert_eq!(row.data["errorCode"], "invalidInput");
    }

    #[test]
    fn every_error_row_has_one_shape() {
        let next = json!({"retry": {"tool": "localSearch", "query": {"path": "/r"}}});
        let row = DomainResult::failure(
            "pathNotFound",
            "Path does not exist: /r",
            vec!["Verify the path exists.".into()],
            Some(next.clone()),
            FailureKind::NotFound,
        );
        assert_eq!(row.status, Some("error"));
        assert_eq!(row.failure, Some(FailureKind::NotFound));
        let keys: Vec<&str> = row
            .data
            .as_object()
            .expect("record")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["error", "errorCode", "hints", "next"]);
        assert_eq!(row.data["next"], next);

        let bare =
            DomainResult::failure("x.failed", "failed", vec![], None, FailureKind::Execution);
        assert_eq!(
            bare.data,
            json!({"error": "failed", "errorCode": "x.failed"})
        );
        assert_eq!(bare.failure, Some(FailureKind::Execution));
    }

    /// A tool payload that reports `status: "error"` is an execution failure
    /// on every path, including the GitHub history arms.
    #[test]
    fn error_status_payload_is_an_execution_failure() {
        let failed = value_result(json!({"status": "error", "error": "compare failed"}));
        assert_eq!(failed.status, Some("error"));
        assert_eq!(failed.failure, Some(FailureKind::Execution));
        let empty = value_result(json!({"status": "empty"}));
        assert_eq!((empty.status, empty.failure), (Some("empty"), None));
        let ok = value_result(json!({"items": []}));
        assert_eq!((ok.status, ok.failure), (None, None));
    }

    /// Contract validation already passed, so a row the typed parse rejects
    /// is core/native drift. Every local tool reports it with one code.
    #[test]
    fn typed_shape_mismatch_is_invalid_input_for_every_local_tool() {
        let root = tempfile::tempdir().expect("fixture directory");
        let path = root.path().to_string_lossy().into_owned();
        let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("path policy");
        let security = ContentSecurity::new();
        let context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            output_bytes: 16_000,
            walk_threads: None,
        };
        let views = crate::security::scan::SanitizedViewMemo::default();
        for (tool, query) in [
            (ToolId::LocalFetch, json!({"path": 5})),
            (ToolId::LocalSearch, json!({"path": path, "searchText": 5})),
            (
                ToolId::StructureSearch,
                json!({"operation": "syntaxTree", "path": path}),
            ),
            (ToolId::AstSearch, json!({"path": path})),
            (ToolId::AstTopology, json!({"path": 5})),
            (
                ToolId::AstRewrite,
                json!({"path": path, "langType": "typescript", "ruleKind": "pattern",
                       "pattern": "a($A)", "rewrite": "b($A)", "allowSyntaxRegression": true}),
            ),
        ] {
            let row = execute_local(
                tool,
                &query,
                &paths,
                &security,
                &context,
                &LocalFetchRegex::default(),
                &views,
                false,
            )
            .unwrap_or_else(|_| panic!("{tool} returns a row"));
            assert_eq!(row.status, Some("error"), "{tool}");
            assert_eq!(
                row.data["errorCode"], "invalidInput",
                "{tool}: {}",
                row.data
            );
            assert_eq!(row.failure, Some(FailureKind::Execution), "{tool}");
        }
    }

    #[test]
    fn http_status_is_carried_when_present_and_absent_when_not() {
        let with = provider_failure(
            "upstream said no".into(),
            "provider_error".into(),
            vec![],
            Some(429),
        );
        assert_eq!(with.data["httpStatus"], serde_json::json!(429));
        assert_eq!(with.data["errorCode"], "provider_error");
        assert_eq!(with.data["retryable"], true);
        assert!(with.data["hints"][0].is_string());
        let without = provider_failure("client-side".into(), "invalid_query".into(), vec![], None);
        assert!(without.data.get("httpStatus").is_none(), "absence is valid");
        assert_eq!(without.data["retryable"], false);
        assert!(
            without.data["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("Correct the package coordinate")),
            "{}",
            without.data
        );

        for (code, status, retryable, hint_fragment) in [
            ("authentication", Some(401), false, "credentials"),
            ("rate_limit", Some(429), true, "rate-limit reset"),
            ("timeout", None, true, "Retry once"),
            (
                "unsupported_capability",
                None,
                false,
                "exact package lookup",
            ),
        ] {
            let failure = provider_failure(format!("{code} failure"), code.into(), vec![], status);
            assert_eq!(failure.data["retryable"], retryable, "{code}");
            assert!(
                failure.data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| hint.contains(hint_fragment)),
                "{code}: {}",
                failure.data
            );
        }
    }
}
