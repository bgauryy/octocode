use super::refresh::{RefreshMode, RefreshStore, exchange_refresh_token, refresh_locked};
use super::{
    LoginEndpoints, StoredCredentials, device_code_form, is_refresh_token_expired,
    is_token_expired, login_device_flow_in_store, parse_granted_scopes, refresh_token_form,
    token_poll_form,
};
use crate::civil_date::unix_secs;
use crate::providers::github::OAuthToken;
use octocode_github::{ProviderError, ProviderErrorKind};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Fail the test if anything POSTs to `server`.
pub(super) async fn forbid_posts(server: &MockServer) {
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(server)
        .await;
}

pub(super) fn stored(expires_at: Option<&str>, refresh: Option<&str>) -> StoredCredentials {
    StoredCredentials {
        hostname: "github.com".into(),
        username: "octo".into(),
        token: OAuthToken {
            token: "gho_old".into(),
            token_type: "oauth".into(),
            scopes: None,
            refresh_token: refresh.map(str::to_owned),
            expires_at: expires_at.map(str::to_owned),
            refresh_token_expires_at: None,
        },
        git_protocol: "https".into(),
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}

#[test]
fn github_com_uses_web_origin_not_api_host() {
    let endpoints = LoginEndpoints::from_api_url("https://api.github.com/");
    assert_eq!(endpoints.web_origin, "https://github.com");
    assert_eq!(endpoints.host, "github.com");
    let ghes = LoginEndpoints::from_api_url("https://ghe.example.com/api/v3");
    assert_eq!(ghes.web_origin, "https://ghe.example.com");
    assert_eq!(ghes.host, "ghe.example.com");
    assert_eq!(LoginEndpoints::from_host("github.com").host, "github.com");
}

#[test]
fn oauth_forms_use_standard_form_encoding_and_space_delimited_scopes() {
    let device = url::form_urlencoded::parse(device_code_form("client id").as_bytes())
        .into_owned()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        device.get("client_id").map(String::as_str),
        Some("client id")
    );
    assert_eq!(
        device.get("scope").map(String::as_str),
        Some("repo read:org gist")
    );

    let poll = url::form_urlencoded::parse(token_poll_form("cid", "device/code").as_bytes())
        .into_owned()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        poll.get("device_code").map(String::as_str),
        Some("device/code")
    );
    assert_eq!(
        poll.get("grant_type").map(String::as_str),
        Some("urn:ietf:params:oauth:grant-type:device_code")
    );

    let refresh =
        url::form_urlencoded::parse(refresh_token_form("cid", "refresh/token").as_bytes())
            .into_owned()
            .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        refresh.get("refresh_token").map(String::as_str),
        Some("refresh/token")
    );
    assert!(!refresh.contains_key("client_secret"));
    assert_eq!(
        parse_granted_scopes("repo, read:org gist"),
        vec!["repo", "read:org", "gist"]
    );
}

#[test]
fn expiry_helpers_match_five_minute_skew() {
    let future = crate::civil_date::iso8601_secs(unix_secs() + 3600);
    assert!(!is_token_expired(&stored(Some(&future), Some("r"))));
    let soon = crate::civil_date::iso8601_secs(unix_secs() + 30);
    assert!(is_token_expired(&stored(Some(&soon), Some("r"))));
    assert!(!is_token_expired(&stored(None, Some("r"))));
    assert!(!is_refresh_token_expired(&stored(None, Some("r"))));
}

/// A negative UTC offset is a valid RFC 3339 expiry, not an
/// unparsable (and so expired) one.
#[test]
fn expiry_honors_negative_and_positive_offsets() {
    let in_two_hours = crate::civil_date::iso8601_secs(unix_secs() + 2 * 3600);
    let local = |sign: char| {
        // The same instant written in UTC-05:00 / UTC+05:00 local time.
        let shift = if sign == '-' { -5 * 3600 } else { 5 * 3600 };
        let wall = crate::civil_date::iso8601_secs(unix_secs() + 2 * 3600 + shift);
        format!("{}.000{sign}05:00", wall.trim_end_matches('Z'))
    };
    for expiry in [in_two_hours, local('-'), local('+')] {
        let mut credentials = stored(Some(&expiry), Some("r"));
        assert!(!is_token_expired(&credentials), "{expiry}");
        credentials.token.refresh_token_expires_at = Some(expiry.clone());
        assert!(!is_refresh_token_expired(&credentials), "{expiry}");
    }
    assert!(is_token_expired(&stored(Some("not a date"), Some("r"))));
}

#[tokio::test]
async fn refresh_posts_to_web_origin_and_keeps_username() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gho_new",
            "token_type": "bearer",
            "refresh_token": "r2",
            "expires_in": 28800
        })))
        .mount(&server)
        .await;
    let endpoints = LoginEndpoints {
        web_origin: server.uri(),
        api_origin: server.uri(),
        host: "example.test".into(),
    };
    let token = exchange_refresh_token(&endpoints, "cid", "refresh-1")
        .await
        .expect("refresh HTTP");
    assert_eq!(token.access, "gho_new");
    assert_eq!(token.refresh_token.as_deref(), Some("r2"));
    assert_eq!(token.expires_in, Some(28800));
}

fn expired_stored(refresh: &str) -> StoredCredentials {
    let past = crate::civil_date::iso8601_secs(unix_secs() - 60);
    let mut value = stored(Some(&past), Some(refresh));
    value.hostname = "example.test".into();
    value
}

fn endpoints(server: &MockServer) -> LoginEndpoints {
    LoginEndpoints {
        web_origin: server.uri(),
        api_origin: server.uri(),
        host: "example.test".into(),
    }
}

#[test]
fn concurrent_refreshers_post_the_single_use_refresh_token_once() {
    // Each refresher has its own runtime and lock handle, like two
    // processes; the file lock plus the re-read after acquiring it must
    // leave exactly one refresh POST.
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(async {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("refresh_token=r1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(300))
                    .set_body_json(serde_json::json!({
                        "access_token": "gho_new",
                        "token_type": "bearer",
                        "refresh_token": "r2",
                        "expires_in": 28800
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        server
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir
        .path()
        .join("locks")
        .join("oauth-refresh-example.test.lock");
    let shared = Arc::new(Mutex::new(expired_stored("r1")));
    let endpoints = endpoints(&server);
    let workers = (0..2)
        .map(|_| {
            let shared = shared.clone();
            let lock_path = lock_path.clone();
            let endpoints = endpoints.clone();
            std::thread::spawn(move || {
                let load = |_: &str| -> Result<Option<StoredCredentials>, ProviderError> {
                    Ok(Some(shared.lock().expect("store").clone()))
                };
                let save = |_: &StoredCredentials,
                            value: &StoredCredentials|
                 -> Result<(), ProviderError> {
                    *shared.lock().expect("store") = value.clone();
                    Ok(())
                };
                let store = RefreshStore {
                    load: &load,
                    store: &save,
                    lock_path,
                };
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime")
                    .block_on(refresh_locked(
                        &endpoints,
                        "cid",
                        RefreshMode::IfExpired,
                        &store,
                    ))
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        let refreshed = worker
            .join()
            .expect("worker")
            .expect("refresh")
            .expect("stored");
        assert_eq!(refreshed.token.token, "gho_new");
        assert_eq!(refreshed.token.refresh_token.as_deref(), Some("r2"));
    }
    runtime.block_on(server.verify());
}

#[tokio::test]
async fn forced_refresh_reuses_a_rotation_made_while_waiting() {
    let server = MockServer::builder().start().await;
    forbid_posts(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rotated = expired_stored("r2");
    rotated.token.token = "gho_rotated".into();
    let load = move |_: &str| -> Result<Option<StoredCredentials>, ProviderError> {
        Ok(Some(rotated.clone()))
    };
    let save =
        |_: &StoredCredentials, _: &StoredCredentials| -> Result<(), ProviderError> { Ok(()) };
    let store = RefreshStore {
        load: &load,
        store: &save,
        lock_path: dir.path().join("refresh.lock"),
    };
    let result = refresh_locked(
        &endpoints(&server),
        "cid",
        RefreshMode::Force {
            observed: Some("r1".into()),
        },
        &store,
    )
    .await
    .expect("refresh")
    .expect("stored");
    assert_eq!(result.token.token, "gho_rotated");
    server.verify().await;
}

#[tokio::test]
async fn oauth_calls_do_not_replay_bodies_across_redirects() {
    // A 307/308 from the token endpoint must not re-post the
    // refresh token or device code to another origin.
    let other = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gho_stolen", "device_code": "d", "user_code": "u",
            "verification_uri": "https://x"
        })))
        .expect(0)
        .mount(&other)
        .await;
    let server = MockServer::builder().start().await;
    for (route, status) in [
        ("/login/oauth/access_token", 307_u16),
        ("/login/device/code", 308),
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("location", format!("{}/capture", other.uri()).as_str()),
            )
            .mount(&server)
            .await;
    }
    let refresh = exchange_refresh_token(&endpoints(&server), "cid", "refresh-1")
        .await
        .err()
        .expect("redirect refused");
    assert_eq!(refresh.kind, ProviderErrorKind::RedirectDenied);
    assert_eq!(refresh.status, Some(307));
    let home = tempfile::tempdir().expect("tempdir");
    let device = login_device_flow_in_store(
        &endpoints(&server),
        "cid",
        &CancellationToken::new(),
        &crate::providers::github::CredentialStore::new(home.path()),
    )
    .await
    .expect_err("redirect refused");
    assert_eq!(device.kind, ProviderErrorKind::RedirectDenied);
    assert_eq!(device.status, Some(308));
    other.verify().await;
}
