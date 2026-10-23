use super::*;

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
async fn request_credentials_resolve_once_per_host_until_rejected() {
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
        ["load", "gh"],
        "one resolution per process"
    );
    auth.resolve_with("ghe.example", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(backend.calls().len(), 4, "hosts resolve separately");
    auth.reject("github.com", None);
    auth.resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(
        backend.calls().len(),
        6,
        "a rejected credential re-resolves"
    );
    auth.resolve_with("github.com", AuthMode::Inspect, &budget(), &backend)
        .await
        .unwrap();
    assert_eq!(backend.calls().len(), 8, "inspection never reads the memo");
}

/// A cold batch's rows resolve one host's credential once: the first row
/// reads the store (and spawns `gh`); the rows beside it wait for its answer.
#[tokio::test]
async fn concurrent_rows_resolve_one_host_once() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    let budget = budget();
    let row = || auth.resolve_with("github.com", AuthMode::Request, &budget, &backend);
    let (a, b, c) = tokio::join!(row(), row(), row());
    for selection in [a, b, c] {
        assert_eq!(selection.unwrap().unwrap().token(), "gh-token");
    }
    assert_eq!(backend.calls(), ["load", "gh"], "one resolution per wave");
}

/// A remembered `gh` (or stored) selection has no refresh time of its own,
/// so it lives one [`MEMO_TTL`]: a long-lived host sees `gh auth switch`,
/// a logout, or a rotated token, instead of sending the old token forever.
#[tokio::test]
async fn a_remembered_credential_is_selected_again_after_the_memo_ttl() {
    let mut backend = Backend::new(None);
    let auth = authentication(None);
    let request = async |auth: &Authentication, backend: &Backend| {
        auth.resolve_with("github.com", AuthMode::Request, &budget(), backend)
            .await
            .unwrap()
    };
    assert_eq!(request(&auth, &backend).await.unwrap().token(), "gh-token");
    // `gh auth switch`: within the TTL the memo answers.
    backend.gh = Some("switched-token");
    assert_eq!(request(&auth, &backend).await.unwrap().token(), "gh-token");
    let age = |auth: &Authentication| {
        let mut memo = auth.memo.lock().unwrap();
        let entry = memo.get_mut("github.com").unwrap();
        entry.at = entry.at.checked_sub(MEMO_TTL).unwrap();
    };
    age(&auth);
    assert_eq!(
        request(&auth, &backend).await.unwrap().token(),
        "switched-token"
    );
    // `gh auth logout`: past the TTL the request runs as no credential.
    backend.gh = None;
    age(&auth);
    assert!(request(&auth, &backend).await.is_none());
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
