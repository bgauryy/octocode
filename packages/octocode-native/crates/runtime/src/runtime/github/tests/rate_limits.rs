use super::*;

fn rate_limited(remaining: Option<u64>, reset: Option<u64>, resource: &str) -> ProviderError {
    let mut error = ProviderError::new(ProviderErrorKind::RateLimited, "slow down");
    error.status = Some(403);
    error.retryable = true;
    error.rate_limit = Some(Box::new(RateLimit {
        remaining,
        reset_epoch_seconds: reset,
        retry_after_seconds: Some(60),
        resource: Some(resource.into()),
    }));
    error
}

/// A rate-limited row states each fact once, in seconds, with
/// unknown members absent, the same on every GitHub tool; the one
/// recovery hint is the response stage's, keyed by the code.
#[test]
fn rate_limit_rows_have_one_shape_on_every_github_tool() {
    let primary = rate_limited(Some(0), Some(1_700_000_000), "core");
    let secondary = rate_limited(None, None, "search");
    for (error, rate_limit) in [
        (
            primary,
            json!({"resource": "core", "remaining": 0, "resetEpochSeconds": 1_700_000_000_u64, "retryAfterSeconds": 60}),
        ),
        (
            secondary,
            json!({"resource": "search", "retryAfterSeconds": 60}),
        ),
    ] {
        let expected = json!({
            "error": "slow down",
            "errorCode": "rateLimited",
            "retryable": true,
            "httpStatus": 403,
            "rateLimit": rate_limit,
        });
        for (_tool, result) in [
            (ToolId::GhSearchCode, search_row(error.clone())),
            (
                ToolId::GhSearchHistory,
                history_error(error.clone(), true, None),
            ),
            (
                ToolId::GhGetHistoryItem,
                history_error(error.clone(), false, None),
            ),
            (ToolId::GhCloneRepo, provider_error(error.clone())),
        ] {
            assert_eq!(result.data, expected);
            let keys: Vec<&str> = result
                .data
                .as_object()
                .expect("row")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                keys,
                ["error", "errorCode", "retryable", "httpStatus", "rateLimit"]
            );
            assert_eq!(result.failure, Some(FailureKind::RateLimited));
        }
        // A file read adds only the file's identity.
        let file = file_row(error.clone(), json!({"owner":"a","repo":"b","path":"x"}));
        let mut with_identity = expected.clone();
        with_identity["owner"] = json!("a");
        with_identity["repo"] = json!("b");
        with_identity["path"] = json!("x");
        assert_eq!(file.data, with_identity);
    }
}

/// Only an anonymous caller whose primary quota is spent is told that
/// logging in helps; a secondary limit or an authenticated caller keeps
/// the wait hint.
#[test]
fn login_advice_needs_an_anonymous_caller_and_a_spent_primary_quota() {
    let advised = |error: ProviderError, anonymous: bool| {
        let mut data = history_error(error, true, None).data;
        advise_login_for_anonymous_quota(&mut data, anonymous);
        data["hints"][0]
            .as_str()
            .unwrap_or_default()
            .contains("auth login")
    };
    let primary = || rate_limited(Some(0), Some(1_700_000_000), "core");
    let secondary = || rate_limited(None, None, "search");
    let secondary_with_quota = || rate_limited(Some(21), Some(1_700_000_000), "search");
    assert!(advised(primary(), true));
    assert!(!advised(primary(), false));
    assert!(!advised(secondary(), true));
    assert!(!advised(secondary_with_quota(), true));
}

/// A provider/transport row states `retryable:true` when a retry can
/// help and omits the key otherwise (absence = do not retry unchanged).
#[test]
fn provider_rows_state_retryable_only_when_true() {
    for kind in [
        ProviderErrorKind::Transport,
        ProviderErrorKind::Timeout,
        ProviderErrorKind::Server,
        ProviderErrorKind::NotFound,
        ProviderErrorKind::Configuration,
    ] {
        let error = ProviderError::new(kind, "failed");
        let expected = error.retryable;
        for result in [
            search_row(error.clone()),
            history_error(error.clone(), false, None),
            file_row(error.clone(), json!({"owner":"a","repo":"b","path":"x"})),
            provider_error(error.clone()),
        ] {
            assert_eq!(
                result.data.get("retryable"),
                expected.then_some(&json!(true)),
                "{kind:?}: {}",
                result.data
            );
            assert!(result.data.get("provider").is_none(), "{}", result.data);
        }
    }
}

/// A ghSearchHistory 422 names the search query, with no extra field.
/// (ghGetHistoryItem's bogus-SHA 422 is mapped inside the tool and
/// covered end to end in tests/runtime_github_history.rs.)
#[test]
fn history_search_422_names_the_query() {
    let error = ProviderError {
        kind: ProviderErrorKind::Validation,
        message: "Validation Failed".into(),
        status: Some(422),
        request_id: None,
        documentation_url: None,
        rate_limit: None,
        retryable: false,
        reason: None,
        sso_url: None,
    };
    let result = history_error(error, true, None);
    assert_eq!(
        result.data["error"],
        "Invalid search query or request parameters"
    );
    assert!(
        result.data.get("scopesSuggestion").is_none(),
        "{}",
        result.data
    );
}
