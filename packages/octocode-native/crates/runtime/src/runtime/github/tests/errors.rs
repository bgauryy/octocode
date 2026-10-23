use super::*;

/// A file read and a patch window fill the row's share of the call's
/// response page; only a call without one uses the automatic page.
#[test]
fn github_rows_page_by_the_calls_response_window() {
    let context = |response_window| ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + Duration::from_secs(5),
        walk_threads: None,
        response_window,
        github_credential: None,
    };
    assert_eq!(context(Some(5_000)).row_page_chars(20_000), 5_000);
    assert_eq!(context(Some(50_000)).row_page_chars(20_000), 50_000);
    assert_eq!(context(None).row_page_chars(20_000), 20_000);
}

#[test]
fn authentication_errors_explain_canonical_login_and_environment_precedence() {
    let error = || ProviderError {
        kind: ProviderErrorKind::Authentication,
        message: "Bad credentials".into(),
        status: Some(401),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: None,
        sso_url: None,
    };
    for (tool, result) in [
        (ToolId::GhSearchCode, search_row(error())),
        (ToolId::GhSearchHistory, history_error(error(), true, None)),
        (
            ToolId::GhGetHistoryItem,
            history_error(error(), false, None),
        ),
        (
            ToolId::GhGetFileContent,
            file_row(error(), json!({"owner":"a","repo":"b","path":"x"})),
        ),
        (ToolId::GhCloneRepo, provider_error(error())),
    ] {
        let rendered = staged(tool, &result).to_string();
        assert!(rendered.contains("octocode auth login"), "{rendered}");
        assert!(rendered.contains("GH_TOKEN"), "{rendered}");
        assert!(rendered.contains("invalid env token"), "{rendered}");
        assert!(!rendered.contains("octocode login"), "{rendered}");
    }
}

#[test]
fn legal_block_is_not_rendered_as_a_network_failure() {
    let error = || ProviderError {
        kind: ProviderErrorKind::Unavailable,
        message: "GitHub resource blocked for legal reasons (HTTP 451)".into(),
        status: Some(451),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: None,
        sso_url: None,
    };
    for result in [
        search_row(error()),
        history_error(error(), true, None),
        history_error(error(), false, None),
        file_row(error(), json!({"owner":"a","repo":"b","path":"x"})),
        provider_error(error()),
    ] {
        let text = result.data["error"].as_str().unwrap_or_default();
        assert!(text.contains("legal reasons"), "{}", result.data);
        assert!(result.data.get("retryable").is_none(), "{}", result.data);
        assert_eq!(result.data["errorCode"], "unavailable");
        let rendered = result.data.to_string();
        assert!(
            !rendered.contains("Network connection failed"),
            "{rendered}"
        );
        assert!(!rendered.contains("Retry the request"), "{rendered}");
        assert!(!rendered.contains("internet connection"), "{rendered}");
    }
}

#[test]
fn search_validation_keeps_github_detail_and_tool_specific_window_hint() {
    let error = |message: &str, reason| ProviderError {
        kind: ProviderErrorKind::Validation,
        message: message.into(),
        status: Some(422),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason,
        sso_url: None,
    };
    let result = search_row(error("\"abc\" is not a numeric value", None));
    assert_eq!(
        result.data["error"],
        "Invalid search query or request parameters: \"abc\" is not a numeric value"
    );
    let generic = search_row(error("Validation Failed", None));
    assert_eq!(
        generic.data["error"],
        "Invalid search query or request parameters"
    );
    // A page past the search window states no static hint: its repair is
    // the tool's runnable `next.repair` (`gh_shared::with_search_repair`).
    let window = Some(ProviderErrorReason::SearchWindowExceeded);
    let paged = search_row(error("window", window));
    assert!(
        paged
            .data
            .get("hints")
            .is_none_or(|hints| hints.as_array().is_some_and(Vec::is_empty)),
        "{}",
        paged.data
    );
}

#[test]
fn permission_errors_keep_the_provider_reason_across_github_tools() {
    let reason = "Resource protected by organization SAML SSO authorization";
    let error = || ProviderError {
        kind: ProviderErrorKind::Permission,
        message: reason.into(),
        status: Some(403),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: None,
        sso_url: None,
    };
    for result in [
        search_row(error()),
        history_error(error(), true, None),
        history_error(error(), false, None),
        file_row(error(), json!({"owner":"a","repo":"b","path":"src/lib.rs"})),
    ] {
        assert_eq!(result.data["error"], reason);
        assert_eq!(result.data["httpStatus"], 403);
        assert_eq!(result.failure, Some(FailureKind::Permission));
    }
}

/// Regression: a provider failure whose body decoded into a structured
/// error (here modelled as a Validation 422 carrying rate-limit metadata)
/// must still shape `data.error` as a plain string. Before the fix
/// `history_error` nested the diagnostic object under `error`, producing
/// `results.0.data.error: Expected string` (`outputContractViolation`) and
/// masking the real 422.
#[test]
fn history_error_shapes_error_as_string() {
    let error = ProviderError {
        kind: ProviderErrorKind::Validation,
        message: "Validation Failed: repository rename not followed".into(),
        status: Some(422),
        request_id: None,
        documentation_url: None,
        rate_limit: Some(Box::new(RateLimit {
            remaining: Some(11),
            reset_epoch_seconds: Some(1_700_000_000),
            retry_after_seconds: None,
            resource: None,
        })),
        retryable: false,
        reason: None,
        sso_url: None,
    };

    let result = history_error(error, true, None);
    let data = &result.data;

    assert_eq!(result.status, Some("error"));
    // The contract requires a string here; a nested object regresses it.
    assert!(
        data["error"].is_string(),
        "data.error must be a string, got {}",
        data["error"]
    );
    // GitHub's own 422 detail names the bad input.
    assert_eq!(
        data["error"].as_str(),
        Some("Invalid search query or request parameters: repository rename not followed"),
    );
    // One row shape: each fact once, no `status`/`type`/
    // `rateLimitRemaining`/`scopesSuggestion` siblings, and no hint
    // restating that the named field is invalid.
    assert_eq!(
        data,
        &json!({
            "error": "Invalid search query or request parameters: repository rename not followed",
            "errorCode": "invalidInput",
            "httpStatus": 422,
            "rateLimit": {"remaining": 11, "resetEpochSeconds": 1_700_000_000_u64}
        })
    );
}

/// A ghGetFileContent request with an explicit ref that GitHub rejects
/// ("No commit found for the ref …", canned 404 here) must name the
/// missing branch/tag/SHA and hint at checking the ref instead of the
/// generic "Invalid search query or request parameters" message.
#[test]
fn file_error_names_the_missing_ref_for_explicit_branches() {
    // The transport types GitHub's "No commit found" as a missing ref.
    let error = ProviderError {
        kind: ProviderErrorKind::NotFound,
        message: "No commit found for the ref no-such-branch".into(),
        status: Some(404),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: Some(ProviderErrorReason::RefNotFound),
        sso_url: None,
    };
    let query = json!({
        "owner": "a",
        "repo": "b",
        "path": "src/lib.rs",
        "ref": "no-such-branch"
    });

    let result = file_row(error, query.clone());
    let data = &result.data;

    assert_eq!(result.status, Some("error"));
    assert_eq!(result.failure, Some(FailureKind::NotFound));
    assert_eq!(data["errorCode"], json!("notFound"));
    assert_eq!(data["httpStatus"], json!(404));
    assert!(data.get("retryable").is_none(), "{data}");
    assert_eq!(
        data["error"].as_str(),
        Some("Branch, tag, or SHA not found for a/b: \"no-such-branch\"")
    );
    // The error names the ref and the `viewRefs` lead lists the refs:
    // the lead is the fix, so no hint text beside it.
    assert!(data.get("hints").is_none(), "{data}");
    assert!(data["next"]["viewRefs"].is_object(), "{data}");

    // The commits-endpoint flavor (422) maps the same way.
    let error = ProviderError {
        kind: ProviderErrorKind::NotFound,
        message: "No commit found for SHA: no-such-branch".into(),
        status: Some(422),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: Some(ProviderErrorReason::RefNotFound),
        sso_url: None,
    };
    let result = file_row(error, query.clone());
    assert_eq!(
        result.data["error"].as_str(),
        Some("Branch, tag, or SHA not found for a/b: \"no-such-branch\"")
    );

    // Without an explicit ref the existing not-found shaping is unchanged.
    let error = ProviderError {
        kind: ProviderErrorKind::NotFound,
        message: "Not Found".into(),
        status: Some(404),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: None,
        sso_url: None,
    };
    let result = file_row(error, json!({"owner":"a","repo":"b","path":"src/lib.rs"}));
    assert_eq!(
        result.data["error"].as_str(),
        Some("Repository, resource, or path not found")
    );
}

#[test]
fn file_error_directory_and_binary_get_accurate_recovery() {
    let query = json!({"owner":"a","repo":"b","path":"src","ref":"main"});
    let error = ProviderError::new(
        ProviderErrorKind::Validation,
        "Path \"src\" is a directory, not a file; list it with ghStructure.",
    )
    .with_reason(ProviderErrorReason::PathIsDirectory);
    let result = file_row(error, query.clone());
    let data = &result.data;
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("is a directory"),
        "{data}"
    );
    assert_eq!(
        data["next"]["viewTree"]["query"]["queries"][0]["path"],
        "src"
    );
    assert_eq!(
        data["next"]["viewTree"]["query"]["queries"][0]["ref"],
        "main"
    );

    let error = ProviderError::new(
        ProviderErrorKind::Validation,
        "Binary file (1234 bytes, blob abc); ghGetFileContent returns text only.",
    )
    .with_reason(ProviderErrorReason::BinaryFile);
    let result = file_row(error, json!({"owner":"a","repo":"b","path":"x.png"}));
    let hint = result.data["hints"][0].as_str().unwrap_or_default();
    assert!(!hint.contains("Retry once"), "{hint}");
    assert!(hint.contains("Binary"), "{hint}");
    // The caller's request cannot be served (exit 2), with the
    // file's size and blob SHA, never a decode/execution failure.
    assert_eq!(result.data["errorCode"], "invalidInput", "{}", result.data);
    let message = result.data["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("1234 bytes") && message.contains("blob abc"),
        "{message}"
    );
    assert!(result.data.get("next").is_none(), "{}", result.data);
}

/// Recovery keys on the typed reason: the same text without the reason
/// gets no directory continuation.
#[test]
fn directory_recovery_keys_on_the_typed_reason_not_the_message() {
    let query = json!({"owner":"a","repo":"b","path":"src","ref":"main"});
    let untyped = ProviderError::new(ProviderErrorKind::Validation, "src is a directory");
    assert!(
        file_row(untyped, query.clone()).data["next"]
            .get("viewTree")
            .is_none()
    );
}
