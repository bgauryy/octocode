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
        let typed = DomainResult::from(crate::response::result::ToolError::new(code, "failure"));
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
    let note = crate::response::result::Remaining {
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
        (ErrorCode::HttpStatus, Some(408), true, "Retry once"),
        (
            ErrorCode::HttpStatus,
            Some(404),
            false,
            "provider configuration",
        ),
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
