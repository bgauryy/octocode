//! A tool call resolves its credential once for all its rows and identity checks.
use super::*;

const HOST: &str = "github.com";

fn rows(count: usize) -> Vec<usize> {
    (0..count).collect()
}

#[tokio::test]
async fn gh_call_resolves_the_credential_once_for_every_row() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    let call = CallSelection::default();
    let budget = budget();
    let selected = futures_util::future::join_all(
        rows(5)
            .into_iter()
            .map(|_| auth.resolve_shared_with(&call, HOST, &budget, &backend)),
    )
    .await;
    for selection in selected {
        assert_eq!(selection.unwrap().unwrap().token(), "gh-token");
    }
    assert_eq!(backend.calls(), ["load", "gh"], "one resolution for 5 rows");
}

#[tokio::test]
async fn gh_next_call_resolves_again_and_observes_rotation() {
    let mut backend = Backend::new(None);
    let auth = authentication(None);
    let first = CallSelection::default();
    auth.resolve_shared_with(&first, HOST, &budget(), &backend)
        .await
        .unwrap();
    backend.gh = Some("rotated-token");
    let same_call = auth
        .resolve_shared_with(&first, HOST, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(same_call.token(), "gh-token", "a call keeps its credential");
    let next = auth
        .resolve_shared_with(&CallSelection::default(), HOST, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.token(), "rotated-token");
    assert_eq!(backend.calls(), ["load", "gh", "load", "gh"]);
}

#[tokio::test]
async fn gh_identity_checks_reuse_the_call_selection() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    let call = CallSelection::default();
    let used = auth
        .resolve_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    let identity = auth
        .identity_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap();
    let expected = octocode_github::Partition::new(HOST, Some(used.credential.secret()));
    assert_eq!(identity, expected.identity());
    assert_eq!(backend.calls(), ["load", "gh"], "no second resolution");

    // A call that sent no row inspects the sources once, however often it asks.
    let idle = CallSelection::default();
    for _ in 0..3 {
        auth.identity_shared_with(&idle, HOST, &budget(), &backend)
            .await
            .unwrap();
    }
    assert_eq!(backend.calls(), ["load", "gh", "load", "gh"]);
}

#[tokio::test]
async fn gh_rejected_credential_is_reselected_and_loses_its_identity() {
    let backend = Backend::new(None);
    let auth = authentication(None);
    let call = CallSelection::default();
    let used = auth
        .resolve_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    auth.reject_shared(&call, HOST, Some(&used.credential));
    let error = auth
        .identity_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Authentication);
    let next = auth
        .resolve_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap();
    assert!(next.is_none(), "the rejected gh token is skipped");
}

#[tokio::test]
async fn gh_failed_resolution_is_not_kept_for_the_next_row() {
    let mut backend = Backend::new(None);
    backend.stored = Err(ProviderError::new(
        ProviderErrorKind::Configuration,
        "store broke",
    ));
    backend.gh = None;
    let auth = authentication(None);
    let call = CallSelection::default();
    assert!(
        auth.resolve_shared_with(&call, HOST, &budget(), &backend)
            .await
            .is_err()
    );
    backend.stored = Ok(None);
    backend.gh = Some("gh-token");
    let retry = auth
        .resolve_shared_with(&call, HOST, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.token(), "gh-token");
}
