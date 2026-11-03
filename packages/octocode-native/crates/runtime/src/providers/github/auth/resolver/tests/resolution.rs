use super::*;

#[tokio::test]
async fn credential_rotation_is_observed_on_the_next_request() {
    let mut backend = Backend::new(None);
    let auth = authentication(None);
    let first = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.token(), "gh-token");
    backend.gh = Some("rotated-token");
    let next = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.token(), "rotated-token");
}

#[tokio::test]
async fn absent_credentials_allow_anonymous_access_but_storage_errors_remain_visible() {
    let mut backend = Backend::new(None);
    backend.gh = None;
    let auth = authentication(None);
    assert!(
        auth.resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(backend.calls(), ["load", "gh"]);
    backend.calls.lock().unwrap().clear();
    backend.stored = Err(ProviderError::new(
        ProviderErrorKind::CredentialStoreUnavailable,
        "store unavailable",
    ));
    // Requests run anonymously when there is no token and no usable store.
    let auth = authentication(None);
    assert!(
        auth.resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(backend.calls(), ["load", "gh"]);
    // Inspection still reports the store failure.
    let error = auth
        .resolve_with("github.com", AuthMode::Inspect, &budget(), &backend)
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ProviderErrorKind::CredentialStoreUnavailable);
}

#[tokio::test]
async fn request_credentials_resolve_afresh_for_each_host_and_call() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    for _ in 0..3 {
        let selection = auth
            .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selection.token(), "gh-token");
    }
    assert_eq!(
        backend.calls(),
        ["load", "gh", "load", "gh", "load", "gh"],
        "one fresh resolution per request"
    );
    auth.resolve_with("ghe.example", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(backend.calls().len(), 8, "hosts resolve separately");
    auth.reject("github.com", None);
    auth.resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(
        backend.calls().len(),
        10,
        "a rejected credential re-resolves"
    );
    auth.resolve_with("github.com", AuthMode::Inspect, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(
        backend.calls().len(),
        12,
        "inspection reads current sources"
    );
}

/// Independent requests resolve current credentials even when concurrent.
#[tokio::test]
async fn concurrent_requests_resolve_independently() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    let budget = budget();
    let row = || auth.resolve_with("github.com", AuthMode::Request, &budget, &backend);
    let (a, b, c) = tokio::join!(row(), row(), row());
    for selection in [a, b, c] {
        assert_eq!(selection.unwrap().unwrap().token(), "gh-token");
    }
    let calls = backend.calls();
    assert_eq!(calls.iter().filter(|call| **call == "load").count(), 3);
    assert_eq!(calls.iter().filter(|call| **call == "gh").count(), 3);
}

/// Switching accounts and logging out take effect on the very next request.
#[tokio::test]
async fn account_switch_and_logout_are_observed_without_expiry() {
    let mut backend = Backend::new(None);
    let auth = authentication(None);
    let request = async |backend: &Backend| {
        auth.resolve_with("github.com", AuthMode::Request, &budget(), backend)
            .await
            .unwrap()
    };
    assert_eq!(request(&backend).await.unwrap().token(), "gh-token");
    backend.gh = Some("switched-token");
    assert_eq!(request(&backend).await.unwrap().token(), "switched-token");
    backend.gh = None;
    assert!(request(&backend).await.is_none());
    assert_eq!(backend.calls(), ["load", "gh", "load", "gh", "load", "gh"]);
}

#[tokio::test]
async fn environment_is_lazy_and_host_scoped() {
    let auth = authentication(Some("env-token"));
    let backend = Backend::new(None);
    let selected = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "env-token");
    assert_eq!(selected.source_label(), "env");
    assert!(backend.calls().is_empty());
    let selected = auth
        .resolve_with("enterprise.example", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "gh-token");
}
