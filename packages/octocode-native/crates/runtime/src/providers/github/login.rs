//! GitHub device-flow login (no TUI).
#[cfg(test)]
mod flow_tests;

use super::{CredentialStore, OAuthToken, StoredCredentials};
use crate::civil_date::unix_secs;
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind};

use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::{
    future::Future,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

const GITHUB_APP_CLIENT_ID: &str = "178c6fc778ccc68e1d6a";

/// The OAuth client for `host`: the configured one, else the Octocode app on
/// github.com; empty for an enterprise host without a configured client.
pub fn client_id_for_host<'a>(host: &str, configured: Option<&'a str>) -> &'a str {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if super::auth::normalize_host(host) == "github.com" {
                GITHUB_APP_CLIENT_ID
            } else {
                ""
            }
        })
}

#[derive(Clone, Debug)]
pub struct LoginEndpoints {
    pub web_origin: String,
    pub api_origin: String,
    pub host: String,
}

impl LoginEndpoints {
    pub fn from_host(host: &str) -> Self {
        let host = host.trim().trim_end_matches('/');
        if octocode_github::credential_host(host) == "github.com" {
            Self::from_api_url("https://api.github.com")
        } else {
            Self::from_api_url(&format!("https://{host}/api/v3"))
        }
    }

    pub(crate) fn from_api_url(api_url: &str) -> Self {
        let parsed = url::Url::parse(api_url).ok();
        let api_host = parsed
            .as_ref()
            .and_then(|url| url.host_str())
            .unwrap_or("api.github.com");
        if api_host == "api.github.com" {
            Self {
                web_origin: "https://github.com".into(),
                api_origin: "https://api.github.com".into(),
                host: "github.com".into(),
            }
        } else {
            let origin = parsed
                .as_ref()
                .map(|url| format!("{}://{}", url.scheme(), api_host))
                .unwrap_or_else(|| format!("https://{api_host}"));
            Self {
                web_origin: origin.clone(),
                api_origin: api_url.trim_end_matches('/').to_owned(),
                host: api_host.to_owned(),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<u64>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
    scope: Option<String>,
    error: Option<String>,
    interval: Option<u64>,
}

/// One second of the device flow's clock: RFC 8628 states `interval` and
/// `expires_in` in seconds. Tests pass a shorter one so a poll costs
/// milliseconds instead of a real second.
const DEVICE_SECOND: Duration = Duration::from_secs(1);

/// RFC 8628 §3.5 `slow_down`: GitHub returns the new total `interval`; never
/// poll sooner than the current interval plus 5 s (`second` long each).
fn slow_down_interval(current: Duration, returned: Option<u64>, second: Duration) -> Duration {
    let floor = current + second * 5;
    returned.map_or(floor, |secs| (second * secs_u32(secs)).max(floor))
}

/// A device-flow second count as a `Duration` multiplier.
fn secs_u32(secs: u64) -> u32 {
    u32::try_from(secs).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const LOGIN_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// The OAuth web-origin client (device code, polling, refresh) pooled per
/// Tokio runtime; admission goes through the executor's `auth` group.
fn login_client() -> Result<reqwest::Client, ProviderError> {
    static CLIENTS: crate::providers::RuntimeClients<()> = crate::providers::RuntimeClients::new(4);
    CLIENTS.get((), || {
        // Never follow redirects: a 307/308 would replay the device-code,
        // poll, or refresh-token body to wherever `Location` points.
        octocode_io::http::client(&octocode_io::http::ClientPolicy {
            timeout: Some(LOGIN_HTTP_TIMEOUT),
            ..octocode_io::http::ClientPolicy::default()
        })
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                "failed to initialize login HTTP client",
            )
        })
    })
}

/// POST an OAuth form to the web origin; a transport failure or redirect
/// fails as `what`.
async fn post_form(
    client: &reqwest::Client,
    endpoints: &LoginEndpoints,
    path: &str,
    form: String,
    what: &str,
) -> Result<reqwest::Response, ProviderError> {
    client
        .post(format!("{}{path}", endpoints.web_origin))
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, octocode_github::HTTP_USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|_| ProviderError::new(ProviderErrorKind::Transport, format!("{what} failed")))
        .and_then(|response| reject_redirect(response, what))
}

/// OAuth endpoints answer directly; any 3xx is refused instead of followed.
fn reject_redirect(
    response: reqwest::Response,
    what: &str,
) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_redirection() {
        let mut error = ProviderError::new(
            ProviderErrorKind::RedirectDenied,
            format!(
                "{what} was redirected (HTTP {}); OAuth requests do not follow redirects",
                status.as_u16()
            ),
        );
        error.status = Some(status.as_u16());
        return Err(error);
    }
    Ok(response)
}

fn login_cancelled() -> ProviderError {
    ProviderError::new(ProviderErrorKind::Cancelled, "GitHub login cancelled")
}

/// Run one OAuth call under the host's `auth` throttling group (anonymous
/// key: these calls authenticate with the client id, not a token).
async fn auth_call<T>(
    endpoints: &LoginEndpoints,
    cancellation: &CancellationToken,
    call: impl Future<Output = Result<T, ProviderError>>,
) -> Result<T, ProviderError> {
    let origin = url::Url::parse(&endpoints.api_origin)
        .or_else(|_| url::Url::parse(&endpoints.web_origin))
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Configuration,
                "invalid GitHub login origin",
            )
        })?;
    let budget = octocode_github::GitHubBudget::global();
    let deadline = Instant::now() + LOGIN_HTTP_TIMEOUT * 2;
    let _admission = budget
        .admit_auth(&origin, Duration::from_secs(10), deadline, cancellation)
        .await?;
    tokio::select! {
        _ = cancellation.cancelled() => Err(login_cancelled()),
        result = call => result,
    }
}

pub async fn login_device_flow_in_store(
    endpoints: &LoginEndpoints,
    client_id: &str,
    cancellation: &CancellationToken,
    store: &CredentialStore,
) -> Result<StoredCredentials, ProviderError> {
    login_device_flow_with_store(
        endpoints,
        client_id,
        cancellation,
        &|value| store.save(value),
        DEVICE_SECOND,
    )
    .await
}

async fn login_device_flow_with_store(
    endpoints: &LoginEndpoints,
    client_id: &str,
    cancellation: &CancellationToken,
    store: &(dyn Fn(&StoredCredentials) -> Result<(), ProviderError> + Sync),
    second: Duration,
) -> Result<StoredCredentials, ProviderError> {
    if client_id.trim().is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "GitHub OAuth client ID is required",
        ));
    }
    let client = &login_client()?;
    let device: DeviceCode = auth_call(endpoints, cancellation, async {
        post_form(
            client,
            endpoints,
            "/login/device/code",
            device_code_form(client_id),
            "device code request",
        )
        .await?
        .json()
        .await
        .map_err(|_| ProviderError::new(ProviderErrorKind::Decode, "invalid device code response"))
    })
    .await?;
    eprintln!(
        "Open {} and enter code {}",
        device.verification_uri, device.user_code
    );
    let deadline = Instant::now() + second * secs_u32(device.expires_in.unwrap_or(900).min(900));
    let mut interval = second * secs_u32(device.interval.unwrap_or(5).max(1));
    while Instant::now() < deadline {
        tokio::select! {
            _ = cancellation.cancelled() => return Err(login_cancelled()),
            _ = tokio::signal::ctrl_c() => return Err(login_cancelled()),
            _ = tokio::time::sleep(interval) => {}
        }
        let token: TokenResponse = auth_call(endpoints, cancellation, async {
            post_form(
                client,
                endpoints,
                "/login/oauth/access_token",
                token_poll_form(client_id, &device.device_code),
                "token poll",
            )
            .await?
            .json()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid token poll response")
            })
        })
        .await?;
        if token.error.as_deref() == Some("authorization_pending") {
            continue;
        }
        if token.error.as_deref() == Some("slow_down") {
            interval = slow_down_interval(interval, token.interval, second);
            continue;
        }
        if let Some(access) = token.access_token.clone().filter(|value| !value.is_empty()) {
            let username =
                fetch_authenticated_login(&endpoints.api_origin, &access, LOGIN_HTTP_TIMEOUT)
                    .await
                    .unwrap_or_default();
            let now_secs = unix_secs();
            let now = crate::civil_date::iso8601_secs(now_secs);
            let scopes = token.scope.as_deref().map(parse_granted_scopes);
            let stored = StoredCredentials {
                hostname: endpoints.host.clone(),
                username,
                token: OAuthToken {
                    token: access,
                    token_type: token.token_type.unwrap_or_else(|| "oauth".into()),
                    scopes,
                    refresh_token: token.refresh_token,
                    expires_at: token
                        .expires_in
                        .map(|seconds| expires_after(now_secs, seconds)),
                    refresh_token_expires_at: token
                        .refresh_token_expires_in
                        .map(|seconds| expires_after(now_secs, seconds)),
                },
                git_protocol: "https".into(),
                created_at: now.clone(),
                updated_at: now,
            };
            store(&stored)?;
            return Ok(stored);
        }
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            token.error.unwrap_or_else(|| "device login failed".into()),
        ));
    }
    Err(ProviderError::new(
        ProviderErrorKind::Timeout,
        "device login timed out",
    ))
}

/// What GitHub's `GET /user` says about a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenCheck {
    /// GitHub accepted the token (its login, when the body names one).
    Valid(Option<String>),
    /// GitHub rejected the token (HTTP 401): it cannot authenticate requests.
    Rejected,
    /// GitHub could not be asked (network, timeout, rate limit, bad URL).
    Unverified,
}

/// `GET /user` through the shared executor (throttling, rate-limit
/// bookkeeping, bounded retries): whether GitHub accepts `token`.
pub async fn verify_token(api_url: &str, token: &str, timeout: Duration) -> TokenCheck {
    let Some((endpoint, transport)) = url::Url::parse(api_url)
        .ok()
        .and_then(|url| octocode_github::GitHubEndpoint::new(url).ok())
        .and_then(|endpoint| {
            let transport = octocode_github::GitHubTransport::new(
                endpoint.clone(),
                octocode_github::RetryPolicy {
                    max_attempts: 2,
                    ..Default::default()
                },
                octocode_github::GitHubBudget::global(),
            )
            .ok()?;
            Some((endpoint, transport))
        })
    else {
        return TokenCheck::Unverified;
    };
    let Ok(url) = endpoint.rest(&["user"]) else {
        return TokenCheck::Unverified;
    };
    match transport
        .execute(
            octocode_github::RequestSpec::get(url),
            &octocode_github::RequestContext::new(
                octocode_io::budget::RequestBudget::with_timeout(timeout, 1024 * 1024),
                Some(octocode_github::ResolvedCredential::new(
                    token.to_owned(),
                    octocode_github::CredentialSource::Override,
                )),
            ),
        )
        .await
    {
        Ok(page) => TokenCheck::Valid(
            serde_json::from_slice::<serde_json::Value>(&page.body)
                .ok()
                .and_then(|value| value.get("login")?.as_str().map(str::to_owned)),
        ),
        Err(error) if error.kind == ProviderErrorKind::Authentication => TokenCheck::Rejected,
        Err(_) => TokenCheck::Unverified,
    }
}

/// The login GitHub reports for `token`, or `None` on any failure.
pub(crate) async fn fetch_authenticated_login(
    api_url: &str,
    token: &str,
    timeout: Duration,
) -> Option<String> {
    match verify_token(api_url, token, timeout).await {
        TokenCheck::Valid(login) => login,
        TokenCheck::Rejected | TokenCheck::Unverified => None,
    }
}

fn device_code_form(client_id: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", client_id)
        .append_pair("scope", "repo read:org gist")
        .finish()
}

fn token_poll_form(client_id: &str, device_code: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", client_id)
        .append_pair("device_code", device_code)
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
        .finish()
}

fn refresh_token_form(client_id: &str, refresh_token: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("client_id", client_id)
        .append_pair("refresh_token", refresh_token)
        .finish()
}

fn parse_granted_scopes(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

fn rfc3339_now() -> String {
    crate::civil_date::iso8601_secs(unix_secs())
}

/// The RFC 3339 instant `seconds` after `now`.
fn expires_after(now: i64, seconds: u64) -> String {
    crate::civil_date::iso8601_secs(now.saturating_add_unsigned(seconds))
}

/// How long before its expiry an access token is refreshed.
const REFRESH_SKEW: Duration = Duration::from_secs(5 * 60);

/// When `credentials` must be refreshed: `REFRESH_SKEW` before the access
/// token expires (`UNIX_EPOCH` when the expiry is unreadable), or `None`
/// when the token does not expire.
pub(crate) fn refresh_due(credentials: &StoredCredentials) -> Option<SystemTime> {
    let expires_at = credentials.token.expires_at.as_deref()?;
    Some(
        parse_expiry(expires_at)
            .and_then(|when| when.checked_sub(REFRESH_SKEW))
            .unwrap_or(UNIX_EPOCH),
    )
}

pub fn is_token_expired(credentials: &StoredCredentials) -> bool {
    refresh_due(credentials).is_some_and(|due| SystemTime::now() >= due)
}

pub(crate) fn is_refresh_token_expired(credentials: &StoredCredentials) -> bool {
    let Some(expires_at) = credentials.token.refresh_token_expires_at.as_deref() else {
        return false;
    };
    match parse_expiry(expires_at) {
        None => true,
        Some(when) => SystemTime::now() >= when,
    }
}

fn parse_expiry(value: &str) -> Option<SystemTime> {
    let trimmed = value.trim();
    let secs = match trimmed.parse::<u64>() {
        Ok(secs) => secs,
        Err(_) => crate::civil_date::parse_rfc3339(trimmed)?.0.max(0) as u64,
    };
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

fn mask_token_text(message: &str) -> String {
    let mut out = message.to_owned();
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        while let Some(start) = out.find(prefix) {
            let rest = &out[start + prefix.len()..];
            let take = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric())
                .count();
            if take >= 36 {
                out.replace_range(start..start + prefix.len() + take, "***MASKED***");
            } else {
                break;
            }
        }
    }
    out
}

/// How long a refresher waits for another process to finish its refresh:
/// the OAuth call's own admission wait plus its HTTP timeout.
const REFRESH_LOCK_WAIT: Duration = Duration::from_secs(45);

/// Where stored credentials are read and written during a refresh, and the
/// lock file that serializes refreshes across processes. Platform credentials
/// use the per-user lock; home credentials use the resolved Octocode home.
struct RefreshStore<'a> {
    load: &'a (dyn Fn(&str) -> Result<Option<StoredCredentials>, ProviderError> + Sync),
    store: &'a (dyn Fn(&StoredCredentials, &StoredCredentials) -> Result<(), ProviderError> + Sync),
    lock_path: std::path::PathBuf,
}

/// The refresh lock for a platform (OS keychain) credential. The keychain
/// entry belongs to the OS user, not to one `OCTOCODE_HOME`, so processes with
/// different homes must share this lock: GitHub refresh tokens are single-use.
fn refresh_lock_path(host: &str) -> std::path::PathBuf {
    let name = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    std::env::home_dir()
        .filter(|home| home.is_absolute())
        .map(|home| home.join(".octocode").join("tmp").join("locks"))
        .unwrap_or_else(|| std::env::temp_dir().join("octocode-locks"))
        .join(format!("oauth-refresh-{name}.lock"))
}

/// The advisory file lock (`flock` / `LockFileEx`) held for one
/// load-refresh-store sequence, polled so the wait yields to the runtime.
async fn acquire_refresh_lock(
    path: &std::path::Path,
    wait: Duration,
) -> Result<octocode_io::lock::LockedFile, ProviderError> {
    use octocode_io::lock::TryLock;
    let unavailable = || {
        ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "cannot open the GitHub token refresh lock",
        )
    };
    // Creating the directory and opening the file touch the disk, so they run
    // off the async executor.
    let lock_path = path.to_path_buf();
    let mut file = tokio::task::spawn_blocking(move || {
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    let deadline = Instant::now() + wait;
    loop {
        match octocode_io::lock::try_lock(file) {
            Ok(TryLock::Locked(lock)) => return Ok(lock),
            Ok(TryLock::Busy(busy)) => file = busy,
            Err(_) => return Err(unavailable()),
        }
        if Instant::now() >= deadline {
            return Err(ProviderError::new(
                ProviderErrorKind::Timeout,
                "timed out waiting for another process to refresh the GitHub token",
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// When a refresh under the lock is still needed.
enum RefreshMode {
    /// Refresh only if the stored access token is (still) expired.
    IfExpired,
    /// Explicit refresh; skipped when the stored refresh token no longer
    /// matches the one observed before waiting (another process rotated it).
    Force { observed: Option<String> },
}

/// Load, refresh, and store under one cross-process lock. GitHub refresh
/// tokens are single-use: two processes posting the same one revoke the
/// token family, so the stored credential is re-read after the lock is held
/// and a refresh another process already made is reused.
async fn refresh_locked(
    endpoints: &LoginEndpoints,
    client_id: &str,
    mode: RefreshMode,
    store: &RefreshStore<'_>,
) -> Result<Option<StoredCredentials>, ProviderError> {
    let _lock = acquire_refresh_lock(&store.lock_path, REFRESH_LOCK_WAIT).await?;
    let Some(current) = (store.load)(&endpoints.host)? else {
        return Ok(None);
    };
    let refreshed_elsewhere = match &mode {
        RefreshMode::IfExpired => !is_token_expired(&current),
        RefreshMode::Force { observed } => current.token.refresh_token != *observed,
    };
    if refreshed_elsewhere {
        return Ok(Some(current));
    }
    let save = |updated: &StoredCredentials| (store.store)(&current, updated);
    refresh_stored_credentials(current.clone(), endpoints, client_id, &save)
        .await
        .map(Some)
}

async fn refresh_selected(
    endpoints: &LoginEndpoints,
    client_id: &str,
    mode: RefreshMode,
    credentials: &CredentialStore,
    source: CredentialSource,
) -> Result<Option<StoredCredentials>, ProviderError> {
    if client_id.trim().is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            format!(
                "Set {} to refresh credentials for this GitHub host",
                crate::config::environment::OCTOCODE_GITHUB_CLIENT_ID
            ),
        ));
    }
    let load = |host: &str| credentials.load_from(host, source);
    let save = |previous: &StoredCredentials, value: &StoredCredentials| {
        credentials.save_to(previous, value, source)
    };
    let lock_path = if source == CredentialSource::Storage {
        refresh_lock_path(&endpoints.host)
    } else {
        credentials.home().join("tmp/locks").join(format!(
            "oauth-refresh-{}.lock",
            octocode_engine::digest::sha256(endpoints.host.as_bytes())
        ))
    };
    refresh_locked(
        endpoints,
        client_id,
        mode,
        &RefreshStore {
            load: &load,
            store: &save,
            lock_path,
        },
    )
    .await
}

pub(crate) async fn refresh_auth_token_in_store(
    host: &str,
    client_id: &str,
    store: &CredentialStore,
) -> Result<StoredCredentials, ProviderError> {
    let endpoints = LoginEndpoints::from_host(host);
    let not_logged_in = || {
        ProviderError::new(
            ProviderErrorKind::Authentication,
            format!("Not logged in to {}", endpoints.host),
        )
    };
    let (stored, source) = store.load(&endpoints.host)?.ok_or_else(not_logged_in)?;
    let mode = RefreshMode::Force {
        observed: stored.token.refresh_token,
    };
    refresh_selected(&endpoints, client_id, mode, store, source)
        .await?
        .ok_or_else(not_logged_in)
}

async fn refresh_stored_credentials(
    stored: StoredCredentials,
    endpoints: &LoginEndpoints,
    client_id: &str,
    store: &(dyn Fn(&StoredCredentials) -> Result<(), ProviderError> + Sync),
) -> Result<StoredCredentials, ProviderError> {
    let Some(refresh_token) = stored
        .token
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            "credential.refreshUnsupported",
        ));
    };
    if is_refresh_token_expired(&stored) {
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            "credential.refreshExpired",
        ));
    }
    let token = exchange_refresh_token(endpoints, client_id, &refresh_token).await?;
    let now = unix_secs();
    let updated = StoredCredentials {
        hostname: stored.hostname.clone(),
        username: stored.username.clone(),
        token: OAuthToken {
            token: token.access,
            token_type: token.token_type,
            scopes: stored.token.scopes.clone(),
            refresh_token: token.refresh_token.or(stored.token.refresh_token.clone()),
            expires_at: token.expires_in.map(|seconds| expires_after(now, seconds)),
            refresh_token_expires_at: token
                .refresh_token_expires_in
                .map(|seconds| expires_after(now, seconds)),
        },
        git_protocol: stored.git_protocol.clone(),
        created_at: stored.created_at.clone(),
        updated_at: rfc3339_now(),
    };
    store(&updated)?;
    Ok(updated)
}

struct RefreshedToken {
    access: String,
    token_type: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
}

async fn exchange_refresh_token(
    endpoints: &LoginEndpoints,
    client_id: &str,
    refresh_token: &str,
) -> Result<RefreshedToken, ProviderError> {
    let client = &login_client()?;
    let token: TokenResponse = auth_call(endpoints, &CancellationToken::new(), async {
        post_form(
            client,
            endpoints,
            "/login/oauth/access_token",
            refresh_token_form(client_id, refresh_token),
            "token refresh request",
        )
        .await?
        .error_for_status()
        .map_err(|error| {
            let status = error.status().map(|value| value.as_u16());
            let mut failed = ProviderError::new(
                ProviderErrorKind::Authentication,
                "credential.refreshFailed",
            );
            failed.status = status;
            failed
        })?
        .json()
        .await
        .map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid token refresh response")
        })
    })
    .await?;
    if let Some(error) = token.error.filter(|value| !value.is_empty()) {
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            mask_token_text(&error),
        ));
    }
    let access = token
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Authentication,
                "credential.refreshFailed",
            )
        })?;
    Ok(RefreshedToken {
        access,
        token_type: token.token_type.unwrap_or_else(|| "oauth".into()),
        refresh_token: token.refresh_token,
        expires_in: token.expires_in,
        refresh_token_expires_in: token.refresh_token_expires_in,
    })
}

pub async fn refresh_auth_token_result_in_store(
    host: &str,
    client_id: &str,
    store: &CredentialStore,
) -> RefreshResult {
    refresh_result(
        host,
        refresh_auth_token_in_store(host, client_id, store).await,
    )
}

fn refresh_result(host: &str, result: Result<StoredCredentials, ProviderError>) -> RefreshResult {
    match result {
        Ok(stored) => RefreshResult {
            success: true,
            username: Some(stored.username),
            hostname: Some(stored.hostname),
            error: None,
        },
        Err(error) => RefreshResult {
            success: false,
            username: None,
            hostname: Some(LoginEndpoints::from_host(host).host),
            error: Some(mask_token_text(&error.message)),
        },
    }
}

pub(crate) async fn refresh_stored_in_store(
    host: &str,
    client_id: &str,
    store: &CredentialStore,
    source: CredentialSource,
) -> Result<Option<StoredCredentials>, ProviderError> {
    refresh_selected(
        &LoginEndpoints::from_host(host),
        client_id,
        RefreshMode::IfExpired,
        store,
        source,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{
        LoginEndpoints, RefreshMode, RefreshStore, StoredCredentials, device_code_form,
        exchange_refresh_token, is_refresh_token_expired, is_token_expired,
        login_device_flow_in_store, parse_granted_scopes, refresh_locked, refresh_token_form,
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
            &super::CredentialStore::new(home.path()),
        )
        .await
        .expect_err("redirect refused");
        assert_eq!(device.kind, ProviderErrorKind::RedirectDenied);
        assert_eq!(device.status, Some(308));
        other.verify().await;
    }
}
