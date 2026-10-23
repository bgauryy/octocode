use super::*;

#[tokio::test]
async fn fresh_storage_wins_and_inspection_does_not_refresh_expired_storage() {
    for (expired, mode) in [(false, AuthMode::Request), (true, AuthMode::Inspect)] {
        let backend = Backend::new(Some(stored(expired)));
        let selected = authentication(None)
            .resolve_with("github.com", mode, &budget(), &backend)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected.token(), "stored-token");
        assert_eq!(selected.username.as_deref(), Some("octo"));
        assert_eq!(backend.calls(), ["load"]);
    }
}
#[tokio::test]
async fn expired_storage_refreshes_and_failed_refresh_falls_back_with_gh_provenance() {
    let mut backend = Backend::new(Some(stored(true)));
    authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend.calls(), ["load", "refresh"]);
    backend.calls.lock().unwrap().clear();
    backend.refreshed = Err(ProviderError::new(
        ProviderErrorKind::Authentication,
        "refresh failed",
    ));
    let selected = authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "gh-token");
    assert_eq!(selected.source_label(), "gh-cli");
    assert_eq!(backend.calls(), ["load", "refresh", "gh"]);
}
#[tokio::test]
async fn gh_fallback_never_reenters_storage_or_refresh() {
    let mut backend = Backend::new(None);
    backend.stored = Err(ProviderError::new(
        ProviderErrorKind::CredentialStoreUnavailable,
        "headless",
    ));
    let selected = authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.source_label(), "gh-cli");
    assert_eq!(backend.calls(), ["load", "gh"]);
}
#[tokio::test]
async fn deletion_during_refresh_never_reuses_stale_token() {
    let mut backend = Backend::new(Some(stored(true)));
    backend.refreshed = Ok(None);
    backend.gh = None;
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn cancellation_and_refresh_timeout_do_not_fall_back() {
    let mut backend = Backend::new(Some(stored(true)));
    let cancelled = budget();
    cancelled.cancellation.cancel();
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &cancelled, &backend)
            .await
            .is_err()
    );
    assert!(backend.calls().is_empty());
    backend.refreshed = Err(ProviderError::new(ProviderErrorKind::Timeout, "deadline"));
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .is_err()
    );
    assert_eq!(backend.calls(), ["load", "refresh"]);
}
/// A long-lived runtime whose stored token GitHub starts rejecting (401)
/// re-selects on the next request and falls back to gh, as a fresh process
/// would after the same rejection; it never re-selects the rejected token.
#[tokio::test]
async fn rejected_stored_token_falls_back_to_gh_on_the_next_request() {
    let backend = Backend::new(Some(stored(false)));
    let auth = authentication(None);
    let budget = budget();
    let request = || auth.resolve_with("github.com", AuthMode::Request, &budget, &backend);
    let first = request().await.unwrap().unwrap();
    assert_eq!(first.token(), "stored-token");
    auth.reject("github.com", Some(&first.credential));
    let next = request().await.unwrap().unwrap();
    assert_eq!(next.token(), "gh-token");
    assert_eq!(next.source_label(), "gh-cli");
}
/// A stored token remembered while fresh is selected again once it
/// expires: an expired memo entry is a miss, so the next request reloads and
/// refreshes instead of sending the expired token until GitHub rejects it.
#[tokio::test]
async fn memoized_stored_credential_refreshes_once_it_expires() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut soon = stored(false);
    // Fresh now; inside the refresh skew two seconds from now.
    soon.token.expires_at = Some((now + 5 * 60 + 2).to_string());
    let mut backend = Backend::new(Some(soon));
    let mut refreshed = stored(false);
    refreshed.token.token = "refreshed-token".into();
    backend.refreshed = Ok(Some(refreshed));
    let mut auth = authentication(None);
    let budget = budget();
    let first = auth
        .resolve_with("github.com", AuthMode::Request, &budget, &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.token(), "stored-token");
    // Three seconds on: past the refresh time, still inside the memo TTL.
    auth.now = || std::time::SystemTime::now() + Duration::from_secs(3);
    let next = auth
        .resolve_with("github.com", AuthMode::Request, &budget, &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.token(), "refreshed-token");
    assert_eq!(backend.calls(), ["load", "load", "refresh"]);
}
/// GitHub rejecting an expired stored token (401) refreshes it: expiry is
/// checked before rejection, so the rejection never blocks the refresh.
#[tokio::test]
async fn rejected_expired_stored_token_refreshes() {
    let mut backend = Backend::new(Some(stored(true)));
    let mut refreshed = stored(false);
    refreshed.token.token = "refreshed-token".into();
    backend.refreshed = Ok(Some(refreshed));
    let auth = authentication(None);
    auth.reject(
        "github.com",
        Some(&ResolvedCredential::new(
            "stored-token",
            CredentialSource::Storage,
        )),
    );
    let next = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.token(), "refreshed-token");
    assert_eq!(next.source_label(), "platform");
    assert_eq!(backend.calls(), ["load", "refresh"]);
}

#[tokio::test]
async fn saved_response_inspection_rejects_known_rejected_credentials_and_honors_budget() {
    let auth = authentication(Some("env-token"));
    let credential = ResolvedCredential::new("env-token", CredentialSource::Environment);
    let first = auth
        .inspect_response_identity("github.com", &budget())
        .await
        .unwrap();
    assert_eq!(
        first,
        octocode_github::Partition::new("github.com", Some(credential.secret())).identity()
    );
    auth.reject("github.com", Some(&credential));
    let rejected = auth
        .inspect_response_identity("github.com", &budget())
        .await
        .unwrap_err();
    assert_eq!(rejected.kind, ProviderErrorKind::Authentication);
    let expired = RequestBudget::with_timeout(Duration::ZERO, 1024);
    assert!(
        auth.inspect_response_identity("github.com", &expired)
            .await
            .is_err()
    );
}
