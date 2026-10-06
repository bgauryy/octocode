use super::*;
use serde_json::json;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn refresh_cannot_restore_logout_or_overwrite_a_new_login() {
    for replacement in [false, true] {
        let server = MockServer::start().await;
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

fn endpoints(server: &MockServer) -> LoginEndpoints {
    LoginEndpoints {
        web_origin: server.uri(),
        api_origin: server.uri(),
        host: "fixture.example".into(),
    }
}

async fn device(server: &MockServer, expires: u64) {
    Mock::given(method("POST")).and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code":"fixture-device", "user_code":"FIXTURE", "verification_uri":"https://example.test/login",
            "interval":1, "expires_in":expires
        }))).mount(server).await;
}

async fn user(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer synthetic-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"fixture-user"})))
        .expect(1)
        .mount(server)
        .await;
}

fn token() -> serde_json::Value {
    json!({"access_token":"synthetic-access", "refresh_token":"synthetic-refresh",
        "token_type":"bearer", "expires_in":3600, "refresh_token_expires_in":7200, "scope":"repo, read:org"})
}

/// The token poll succeeds on its one expected request.
async fn grant_token_once(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token()))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn token_refresh_debug_does_not_expose_the_access_token() {
    let result = TokenWithRefreshResult {
        token: Some("synthetic-debug-secret".into()),
        source: "stored",
        username: None,
        refresh_error: None,
    };
    assert!(!format!("{result:?}").contains("synthetic-debug-secret"));
}

#[tokio::test]
async fn missing_enterprise_client_id_never_posts_a_refresh() {
    let server = MockServer::start().await;
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
async fn device_login_waits_for_authorization_then_saves_complete_credentials_once() {
    let server = MockServer::start().await;
    device(&server, 15).await;
    user(&server).await;
    let polls = AtomicUsize::new(0);
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                json!({"error":"authorization_pending"})
            } else {
                token()
            })
        })
        .expect(2)
        .mount(&server)
        .await;
    let saved = Mutex::new(Vec::new());
    let save = |value: &StoredCredentials| {
        saved.lock().unwrap().push(value.clone());
        Ok(())
    };
    let returned = login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &save,
    )
    .await
    .unwrap();
    let saved = saved.lock().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].username, "fixture-user");
    assert_eq!(saved[0].hostname, "fixture.example");
    assert_eq!(saved[0].token.token, "synthetic-access");
    assert_eq!(
        saved[0].token.refresh_token.as_deref(),
        Some("synthetic-refresh")
    );
    assert_eq!(
        saved[0].token.scopes.as_ref().unwrap(),
        &["repo", "read:org"]
    );
    assert!(!is_token_expired(&saved[0]));
    assert!(!is_refresh_token_expired(&saved[0]));
    assert_eq!(returned.updated_at, saved[0].updated_at);
}

#[tokio::test]
async fn device_login_reports_failed_save_instead_of_success() {
    let server = MockServer::start().await;
    device(&server, 15).await;
    user(&server).await;
    grant_token_once(&server).await;
    let save = |_: &StoredCredentials| {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "fixture storage unavailable",
        ))
    };
    let error = login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &save,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::CredentialStoreUnavailable);
    assert!(!error.message.contains("synthetic-"));
}

#[tokio::test]
async fn denied_or_expired_device_authorization_never_saves_credentials() {
    for error in ["access_denied", "expired_token"] {
        let server = MockServer::start().await;
        device(&server, 15).await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error":error})))
            .expect(1)
            .mount(&server)
            .await;
        let save = |_: &StoredCredentials| -> Result<(), ProviderError> {
            panic!("failed authorization must not save")
        };
        let result = login_device_flow_with_store(
            &endpoints(&server),
            "client",
            &CancellationToken::new(),
            &save,
        )
        .await
        .unwrap_err();
        assert_eq!(result.kind, ProviderErrorKind::Authentication);
    }
}

#[tokio::test]
async fn cancelled_or_timed_out_device_login_never_saves_credentials() {
    let server = MockServer::start().await;
    device(&server, 0).await;
    let save = |_: &StoredCredentials| -> Result<(), ProviderError> {
        panic!("cancelled login must not save")
    };
    let error = login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &save,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = login_device_flow_with_store(&endpoints(&server), "client", &cancellation, &save)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
}

#[tokio::test]
async fn refresh_persists_rotated_tokens_and_propagates_save_failure() {
    let server = MockServer::start().await;
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
    let server = MockServer::start().await;
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
async fn device_login_persists_into_home_and_a_new_store_reads_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let server = MockServer::start().await;
    device(&server, 15).await;
    user(&server).await;
    grant_token_once(&server).await;
    login_device_flow_in_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &store,
    )
    .await
    .unwrap();
    let (saved, source) = CredentialStore::new(dir.path())
        .load("fixture.example")
        .unwrap()
        .unwrap();
    assert_eq!(source, CredentialSource::Home);
    assert_eq!(saved.username, "fixture-user");
    assert_eq!(saved.token.token, "synthetic-access");
    assert_eq!(
        saved.token.refresh_token.as_deref(),
        Some("synthetic-refresh")
    );
    assert!(
        !std::fs::read_to_string(dir.path().join("credentials.json"))
            .unwrap()
            .contains("synthetic-")
    );
}

#[tokio::test]
async fn concurrent_home_refreshes_rotate_once_and_persist_for_next_process() {
    let server = MockServer::start().await;
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
    let server = MockServer::start().await;
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
        login_device_flow_in_store(
            &endpoints(&server),
            "client",
            &CancellationToken::new(),
            &store
        )
        .await
        .is_err()
    );
    assert_eq!(
        std::fs::read(dir.path().join("credentials.json")).unwrap(),
        before
    );
}

/// GitHub's `slow_down` carries the new total interval, not an increment;
/// RFC 8628 §3.5 requires at least +5 s either way.
#[test]
fn slow_down_adopts_the_returned_interval_with_an_rfc_floor() {
    let secs = Duration::from_secs;
    assert_eq!(slow_down_interval(secs(5), Some(10)), secs(10));
    assert_eq!(slow_down_interval(secs(5), None), secs(10));
    assert_eq!(slow_down_interval(secs(10), Some(3)), secs(15));
}
