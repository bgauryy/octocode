//! Domain dispatch. Interfaces never select security or execution policy.
use super::FailureKind;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::outcome::{DomainResult, Upstream};
use crate::response::result::failure_kind;
use crate::security::ContentSecurity;
use crate::tools::ast_search::{AstSearchQuery, execute_ast};
use crate::tools::local_fetch::LocalFetchQuery;
use crate::tools::local_search::{LocalSearchQuery, SearchStatus, execute_local_search};
use crate::tools::structure_search::{StructureSearchQuery, execute_structure};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// The runtime-owned services a local row runs against.
pub(super) struct LocalEnv<'a> {
    pub paths: &'a PathPolicy,
    pub security: &'a ContentSecurity,
    pub views: &'a crate::security::scan::SanitizedViewMemo,
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
            || http_status.is_some_and(|status| status == 408 || status == 429 || status >= 500)
    });
    // An invalid input's message names the rejected field or the registry's
    // own reason; a static hint would only restate it.
    if hints.is_empty() && code != ErrorCode::InvalidInput {
        hints.push(
            match code {
                ErrorCode::Authentication => "Verify registry credentials and access, then retry.",
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
mod provider_failure_tests {
    use super::*;
    use serde_json::json;

    /// A tool's typed remaining notes survive dispatch; a bare value has none.
    #[test]
    fn payload_and_tool_errors_share_failure_classification() {
        for code in [
            ErrorCode::ClassificationRateLimited,
            ErrorCode::ClassificationProviderError,
            ErrorCode::NotFound,
        ] {
            let payload = DomainResult::payload(json!({"errorCode":code}), Some("error"));
            let typed =
                DomainResult::from(crate::response::result::ToolError::new(code, "failure"));
            assert_eq!(payload.failure, typed.failure, "{code}");
        }
        assert_eq!(
            DomainResult::payload(json!({"error":"failed"}), Some("error")).failure,
            Some(FailureKind::Execution)
        );
        assert!(
            DomainResult::payload(json!({"errorCode":"classificationRateLimited"}), None)
                .failure
                .is_none()
        );
    }

    #[test]
    fn tool_data_keeps_its_remaining_notes() {
        let note = crate::response::pages::Remaining {
            page: "nextPage".into(),
            left: "4900 more reachable".into(),
        };
        let data = crate::response::result::ToolData {
            remaining: vec![note.clone()],
            ..crate::response::result::ToolData::from(serde_json::json!({"artifacts": []}))
        };
        assert_eq!(data_result(data).remaining, vec![note]);
        assert!(data_result(serde_json::json!({})).remaining.is_empty());
    }

    #[test]
    fn tool_error_rows_take_their_failure_kind_from_the_error_code() {
        let unresolved = value_result(json!({"status":"error","errorCode":"anchorUnresolved"}));
        assert_eq!(unresolved.failure, Some(FailureKind::NotFound));
        let missing = value_result(json!({"status":"error","errorCode":"pathNotFound"}));
        assert_eq!(missing.failure, Some(FailureKind::NotFound));
        let other = value_result(json!({"status":"error","errorCode":"serverUnavailable"}));
        assert_eq!(other.failure, Some(FailureKind::Execution));
        assert_eq!(
            value_result(json!({"status":"error"})).failure,
            Some(FailureKind::Execution)
        );
    }

    #[test]
    fn typed_parse_failure_is_a_row_error_not_a_worker_failure() {
        let row = parse_query::<std::collections::BTreeMap<String, usize>>(json!({"offset": 1.5}))
            .expect_err("fraction");
        assert_eq!(row.status, Some("error"));
        assert_eq!(row.data["errorCode"], "invalidInput");
    }

    #[test]
    fn every_error_row_has_one_shape() {
        let next = json!({"retry": {"tool": "localSearch", "query": {"path": "/r"}}});
        let row = DomainResult::failure(
            ErrorCode::PathNotFound,
            "Path does not exist: /r",
            vec!["Verify the path exists.".into()],
            Some(next.clone()),
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

        let bare = DomainResult::failure(ErrorCode::ExecutionFailed, "failed", vec![], None);
        assert_eq!(
            bare.data,
            json!({"error": "failed", "errorCode": "executionFailed"})
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
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        let views = crate::security::scan::SanitizedViewMemo::default();
        for (tool, query) in [
            (ToolId::LocalFetch, json!({"path": 5})),
            (ToolId::LocalSearch, json!({"path": path, "matchString": 5})),
            (
                ToolId::StructureSearch,
                json!({"operation": "syntaxTree", "path": path}),
            ),
            (ToolId::AstSearch, json!({"path": path})),
        ] {
            let row = execute_local(
                tool,
                &query,
                &context,
                &LocalEnv {
                    paths: &paths,
                    security: &security,
                    views: &views,
                    selection: None,
                },
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
    fn a_non_local_tool_is_a_routing_error() {
        let root = tempfile::tempdir().expect("fixture directory");
        let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("path policy");
        let context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        for tool in [ToolId::GhSearchCode, ToolId::LspSearch, ToolId::Clasify] {
            let routed = execute_local(
                tool,
                &json!({}),
                &context,
                &LocalEnv {
                    paths: &paths,
                    security: &ContentSecurity::new(),
                    views: &crate::security::scan::SanitizedViewMemo::default(),
                    selection: None,
                },
            );
            assert!(
                matches!(routed, Err(ExecutionError::UnroutedTool)),
                "{tool}"
            );
        }
    }

    /// Every provider/transport row states each fact once, in one order, with
    /// `retryable` always present and absent facts omitted.
    #[test]
    fn provider_rows_have_one_shape() {
        let next = json!({"retry": {"tool": "artifactSearch", "query": {"queries": []}}});
        let row = DomainResult::provider(
            ErrorCode::RateLimited,
            "slow down",
            vec!["Wait.".into()],
            Some(next.clone()),
            Upstream {
                retryable: true,
                http_status: Some(429),
                rate_limit: Some(json!({"resource": "core", "retryAfterSeconds": 60})),
            },
        );
        assert_eq!(row.status, Some("error"));
        assert_eq!(row.failure, Some(FailureKind::RateLimited));
        let keys: Vec<&str> = row
            .data
            .as_object()
            .expect("record")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "error",
                "errorCode",
                "retryable",
                "httpStatus",
                "rateLimit",
                "hints",
                "next"
            ]
        );
        let bare = DomainResult::provider(
            ErrorCode::Transport,
            "Network connection failed",
            vec![],
            None,
            Upstream::default(),
        );
        assert_eq!(
            bare.data,
            json!({"error": "Network connection failed", "errorCode": "transport"}),
            "retryable is stated only when true"
        );
        // Registry and language-server rows use the same builder; an
        // emitter's own retry verdict wins over the derived one.
        let lsp = provider_failure(
            "crashed".into(),
            ErrorCode::ServerCrashed,
            vec!["h".into()],
            None,
            Some(true),
        );
        assert_eq!(
            lsp.data,
            json!({"error": "crashed", "errorCode": "serverCrashed", "retryable": true, "hints": ["h"]})
        );
    }

    #[test]
    fn http_status_is_carried_when_present_and_absent_when_not() {
        let with = provider_failure(
            "upstream said no".into(),
            ErrorCode::ProviderError,
            vec![],
            Some(429),
            None,
        );
        assert_eq!(with.data["httpStatus"], serde_json::json!(429));
        assert_eq!(with.data["errorCode"], "providerError");
        assert_eq!(with.data["retryable"], true);
        assert!(with.data["hints"][0].is_string());
        let without = provider_failure(
            "client-side".into(),
            ErrorCode::InvalidInput,
            vec![],
            None,
            None,
        );
        assert!(without.data.get("httpStatus").is_none(), "absence is valid");
        assert!(without.data.get("retryable").is_none(), "{}", without.data);
        // The message states the rejected field or the registry's reason; a
        // static hint would only restate it.
        assert!(without.data.get("hints").is_none(), "{}", without.data);

        for (code, status, retryable, hint_fragment) in [
            (ErrorCode::Authentication, Some(401), false, "credentials"),
            (ErrorCode::RateLimited, Some(429), true, "rate-limit reset"),
            (ErrorCode::Timeout, None, true, "Retry once"),
            (
                ErrorCode::CapabilityUnavailable,
                None,
                false,
                "exact package lookup",
            ),
        ] {
            let failure = provider_failure(format!("{code} failure"), code, vec![], status, None);
            assert_eq!(
                failure.data.get("retryable"),
                retryable.then_some(&serde_json::json!(true)),
                "{code}"
            );
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
