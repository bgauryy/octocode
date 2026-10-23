use super::*;
use crate::providers::github::CredentialStore;
use crate::providers::github::login::device::login_device_flow_with_store;
use crate::providers::github::login::device::slow_down_interval;

#[tokio::test]
async fn device_login_waits_for_authorization_then_saves_complete_credentials_once() {
    let server = MockServer::builder().start().await;
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
        TEST_SECOND,
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
    let server = MockServer::builder().start().await;
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
        TEST_SECOND,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::CredentialStoreUnavailable);
    assert!(!error.message.contains("synthetic-"));
}

#[tokio::test]
async fn denied_or_expired_device_authorization_never_saves_credentials() {
    for error in ["access_denied", "expired_token"] {
        let server = MockServer::builder().start().await;
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
            TEST_SECOND,
        )
        .await
        .unwrap_err();
        assert_eq!(result.kind, ProviderErrorKind::Authentication);
    }
}

#[tokio::test]
async fn cancelled_or_timed_out_device_login_never_saves_credentials() {
    let server = MockServer::builder().start().await;
    device(&server, 0).await;
    let save = |_: &StoredCredentials| -> Result<(), ProviderError> {
        panic!("cancelled login must not save")
    };
    let error = login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &save,
        TEST_SECOND,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &cancellation,
        &save,
        TEST_SECOND,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
}

#[tokio::test]
async fn device_login_persists_into_home_and_a_new_store_reads_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let server = MockServer::builder().start().await;
    device(&server, 15).await;
    user(&server).await;
    grant_token_once(&server).await;
    login_device_flow_with_store(
        &endpoints(&server),
        "client",
        &CancellationToken::new(),
        &|value| store.save(value),
        TEST_SECOND,
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

/// GitHub's `slow_down` carries the new total interval, not an increment;
/// RFC 8628 §3.5 requires at least +5 s either way.
#[test]
fn slow_down_adopts_the_returned_interval_with_an_rfc_floor() {
    let secs = Duration::from_secs;
    let second = secs(1);
    assert_eq!(slow_down_interval(secs(5), Some(10), second), secs(10));
    assert_eq!(slow_down_interval(secs(5), None, second), secs(10));
    assert_eq!(slow_down_interval(secs(10), Some(3), second), secs(15));
    // A test clock scales every term, the RFC floor included.
    let tick = Duration::from_millis(10);
    assert_eq!(slow_down_interval(tick, Some(3), tick), tick * 6);
}
