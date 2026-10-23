use super::*;
use crate::providers::github::CredentialStore;
use crate::providers::github::login::device::login_device_flow_with_store;
use crate::providers::github::login::refresh::RefreshMode;
use crate::providers::github::login::refresh::refresh_selected;
use crate::providers::github::login::refresh::refresh_stored_credentials;

#[tokio::test]
async fn refresh_cannot_restore_logout_or_overwrite_a_new_login() {
    for replacement in [false, true] {
        let server = MockServer::builder().start().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());
        let mut old = tests::stored(Some("2000-01-01T00:00:00Z"), Some("old-refresh"));
        old.hostname = "fixture.example".into();
        store.save(&old).unwrap();
        let changed = store.clone();
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(move |_: &wiremock::Request| {
                // Simulate another process completing logout or login while HTTP is in flight.
                if replacement {
                    let mut newer = old.clone();
                    newer.token.token = "new-login-token".into();
                    newer.token.refresh_token = Some("new-login-refresh".into());
                    changed.save(&newer).unwrap();
                } else {
                    std::fs::remove_file(changed.home().join("credentials.json")).unwrap();
                    std::fs::remove_file(changed.home().join(".key")).unwrap();
                }
                ResponseTemplate::new(200).set_body_json(token())
            })
            .expect(1)
            .mount(&server)
            .await;
        let result = refresh_selected(
            &endpoints(&server),
            "client",
            RefreshMode::IfExpired,
            &store,
            CredentialSource::Home,
        )
        .await;
        assert!(
            result.is_err(),
            "a changed credential must reject the stale refresh"
        );
        if replacement {
            assert_eq!(
                store
                    .load_from("fixture.example", CredentialSource::Home)
                    .unwrap()
                    .unwrap()
                    .token
                    .token,
                "new-login-token"
            );
        } else {
            assert!(!store.home().join("credentials.json").exists());
        }
    }
}

#[tokio::test]
async fn missing_enterprise_client_id_never_posts_a_refresh() {
    let server = MockServer::builder().start().await;
    tests::forbid_posts(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let mut old = tests::stored(Some("2000-01-01T00:00:00Z"), Some("refresh"));
    old.hostname = "fixture.example".into();
    store.save(&old).unwrap();
    let error = refresh_selected(
        &endpoints(&server),
        "",
        RefreshMode::IfExpired,
        &store,
        CredentialSource::Home,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Authentication);
    assert!(error.message.contains("OCTOCODE_GITHUB_CLIENT_ID"));
}

#[tokio::test]
async fn refresh_persists_rotated_tokens_and_propagates_save_failure() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token()))
        .expect(2)
        .mount(&server)
        .await;
    let mut stored = tests::stored(Some("2000-01-01T00:00:00Z"), Some("old-refresh"));
    stored.hostname = "fixture.example".into();
    let saved = Mutex::new(None);
    let save = |value: &StoredCredentials| {
        *saved.lock().unwrap() = Some(value.clone());
        Ok(())
    };
    let updated = refresh_stored_credentials(stored.clone(), &endpoints(&server), "client", &save)
        .await
        .unwrap();
    assert_eq!(
        updated.token.refresh_token.as_deref(),
        Some("synthetic-refresh")
    );
    assert_eq!(
        saved.lock().unwrap().as_ref().unwrap().token.token,
        updated.token.token
    );
    let fail = |_: &StoredCredentials| {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "write failed",
        ))
    };
    assert_eq!(
        refresh_stored_credentials(stored, &endpoints(&server), "client", &fail)
            .await
            .unwrap_err()
            .kind,
        ProviderErrorKind::CredentialStoreUnavailable
    );
}

#[tokio::test]
async fn missing_or_expired_refresh_token_never_calls_provider_or_saves() {
    let server = MockServer::builder().start().await;
    tests::forbid_posts(&server).await;
    for refresh in [None, Some(""), Some("expired-refresh")] {
        let mut stored = tests::stored(Some("2000-01-01T00:00:00Z"), refresh);
        stored.token.refresh_token_expires_at = Some("2000-01-01T00:00:00Z".into());
        let save = |_: &StoredCredentials| -> Result<(), ProviderError> {
            panic!("unusable refresh must not save")
        };
        let error = refresh_stored_credentials(stored, &endpoints(&server), "client", &save)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Authentication);
    }
    server.verify().await;
}

#[tokio::test]
async fn concurrent_home_refreshes_rotate_once_and_persist_for_next_process() {
    let server = MockServer::builder().start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let mut old = tests::stored(Some("2000-01-01T00:00:00Z"), Some("single-use-refresh"));
    old.hostname = "fixture.example".into();
    store.save(&old).unwrap();
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(50))
                .set_body_json(token()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let endpoints = endpoints(&server);
    let (left, right) = tokio::join!(
        refresh_selected(
            &endpoints,
            "client",
            RefreshMode::IfExpired,
            &store,
            CredentialSource::Home
        ),
        refresh_selected(
            &endpoints,
            "client",
            RefreshMode::IfExpired,
            &store,
            CredentialSource::Home
        )
    );
    assert_eq!(left.unwrap().unwrap().token.token, "synthetic-access");
    assert_eq!(right.unwrap().unwrap().token.token, "synthetic-access");
    let loaded = CredentialStore::new(dir.path())
        .load("fixture.example")
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(
        loaded.token.refresh_token.as_deref(),
        Some("synthetic-refresh")
    );
    assert!(!is_token_expired(&loaded));
    assert!(
        !std::fs::read_to_string(dir.path().join("credentials.json"))
            .unwrap()
            .contains("single-use-refresh")
    );
}

#[tokio::test]
async fn failed_reauthentication_preserves_existing_home_login() {
    let server = MockServer::builder().start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let mut existing = tests::stored(None, Some("existing-refresh"));
    existing.hostname = "fixture.example".into();
    store.save(&existing).unwrap();
    let before = std::fs::read(dir.path().join("credentials.json")).unwrap();
    device(&server, 15).await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error":"access_denied"})))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        login_device_flow_with_store(
            &endpoints(&server),
            "client",
            &CancellationToken::new(),
            &|value| store.save(value),
            TEST_SECOND,
        )
        .await
        .is_err()
    );
    assert_eq!(
        std::fs::read(dir.path().join("credentials.json")).unwrap(),
        before
    );
}

/// A refresh on a new runtime reaches the provider while an earlier runtime
/// sits idle or closed: the login client is pooled per runtime, so no
/// connection whose dispatch task lives on another runtime is reused.
#[test]
fn a_new_runtime_refreshes_through_its_own_client() {
    let server_runtime = tokio::runtime::Runtime::new().unwrap();
    let server = server_runtime.block_on(async {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token()))
            .expect(3)
            .mount(&server)
            .await;
        server
    });
    let mut stored = tests::stored(Some("2000-01-01T00:00:00Z"), Some("old-refresh"));
    stored.hostname = "fixture.example".into();
    let refresh = |runtime: &tokio::runtime::Runtime| {
        runtime.block_on(async {
            // A refresh on another runtime's connection never completes:
            // the bound only stands for "never", not for a slow machine.
            tokio::time::timeout(
                std::time::Duration::from_secs(60),
                refresh_stored_credentials(
                    stored.clone(),
                    &endpoints(&server),
                    "client",
                    &|_: &StoredCredentials| Ok(()),
                ),
            )
            .await
            .expect("refresh stalled on another runtime's connection")
            .unwrap()
            .token
            .token
        })
    };
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    };
    // The first runtime stays open but is no longer driven.
    let idle = runtime();
    assert_eq!(refresh(&idle), "synthetic-access");
    let closed = runtime();
    assert_eq!(refresh(&closed), "synthetic-access");
    drop(closed);
    assert_eq!(refresh(&runtime()), "synthetic-access");
    drop(idle);
    server_runtime.block_on(server.verify());
}
