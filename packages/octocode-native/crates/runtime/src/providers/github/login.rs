//! GitHub device-flow login (no TUI).
#[cfg(test)]
mod flow_tests;
use super::{
    CredentialSource, CredentialStore, OAuthToken, ProviderError, ProviderErrorKind,
    ResolvedCredential, StoredCredentials,
};

use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::{
    future::Future,
    sync::OnceLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

pub const GITHUB_APP_CLIENT_ID: &str = "178c6fc778ccc68e1d6a";

/// Select the shared OAuth client for native CLI and Node authentication.
pub fn client_id_for_host<'a>(host: &str, configured: Option<&'a str>) -> &'a str {
    configured
        .filter(|value| !value.trim().is_empty())
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
        if host == "github.com" || host == "api.github.com" {
            Self::from_api_url("https://api.github.com")
        } else {
            Self::from_api_url(&format!("https://{host}/api/v3"))
        }
    }

    pub fn from_api_url(api_url: &str) -> Self {
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

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithRefreshResult {
    pub token: Option<String>,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_error: Option<String>,
}

impl std::fmt::Debug for TokenWithRefreshResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenWithRefreshResult")
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("source", &self.source)
            .field("username", &self.username)
            .field("refresh_error", &self.refresh_error)
            .finish()
    }
}

pub async fn login_device_flow_with_client_id(
    endpoints: &LoginEndpoints,
    client_id: &str,
) -> Result<StoredCredentials, ProviderError> {
    login_device_flow_cancellable(endpoints, client_id, &CancellationToken::new()).await
}

const LOGIN_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// One pooled client for OAuth web-origin calls (device code, polling,
/// refresh); admission goes through the executor's `auth` group.
fn login_client() -> Result<&'static reqwest::Client, ProviderError> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // Never follow redirects: a 307/308 would replay the device-code,
            // poll, or refresh-token body to wherever `Location` points.
            reqwest::Client::builder()
                .timeout(LOGIN_HTTP_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok()
        })
        .as_ref()
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                "failed to initialize login HTTP client",
            )
        })
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
    let budget = super::GitHubBudget::global();
    let deadline = Instant::now() + LOGIN_HTTP_TIMEOUT * 2;
    let _admission = budget
        .admit_auth(&origin, Duration::from_secs(10), deadline, cancellation)
        .await?;
    tokio::select! {
        _ = cancellation.cancelled() => Err(login_cancelled()),
        result = call => result,
    }
}

/// Device flow whose polling sleep ends on `cancellation` or Ctrl-C.
pub async fn login_device_flow_cancellable(
    endpoints: &LoginEndpoints,
    client_id: &str,
    cancellation: &CancellationToken,
) -> Result<StoredCredentials, ProviderError> {
    login_device_flow_in_store(
        endpoints,
        client_id,
        cancellation,
        &CredentialStore::from_process()?,
    )
    .await
}

pub async fn login_device_flow_in_store(
    endpoints: &LoginEndpoints,
    client_id: &str,
    cancellation: &CancellationToken,
    store: &CredentialStore,
) -> Result<StoredCredentials, ProviderError> {
    login_device_flow_with_store(endpoints, client_id, cancellation, &|value| {
        store.save(value)
    })
    .await
}

async fn login_device_flow_with_store(
    endpoints: &LoginEndpoints,
    client_id: &str,
    cancellation: &CancellationToken,
    store: &(dyn Fn(&StoredCredentials) -> Result<(), ProviderError> + Sync),
) -> Result<StoredCredentials, ProviderError> {
    if client_id.trim().is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "GitHub OAuth client ID is required",
        ));
    }
    let client = login_client()?;
    let device: DeviceCode = auth_call(endpoints, cancellation, async {
        client
            .post(format!("{}/login/device/code", endpoints.web_origin))
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, "octocode-native")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(device_code_form(client_id))
            .send()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Transport, "device code request failed")
            })
            .and_then(|response| reject_redirect(response, "device code request"))?
            .json()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid device code response")
            })
    })
    .await?;
    eprintln!(
        "Open {} and enter code {}",
        device.verification_uri, device.user_code
    );
    let deadline = Instant::now() + Duration::from_secs(device.expires_in.unwrap_or(900).min(900));
    let mut interval = Duration::from_secs(device.interval.unwrap_or(5).max(1));
    while Instant::now() < deadline {
        tokio::select! {
            _ = cancellation.cancelled() => return Err(login_cancelled()),
            _ = tokio::signal::ctrl_c() => return Err(login_cancelled()),
            _ = tokio::time::sleep(interval) => {}
        }
        let token: TokenResponse = auth_call(endpoints, cancellation, async {
            client
                .post(format!("{}/login/oauth/access_token", endpoints.web_origin))
                .header(ACCEPT, "application/json")
                .header(USER_AGENT, "octocode-native")
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(token_poll_form(client_id, &device.device_code))
                .send()
                .await
                .map_err(|_| ProviderError::new(ProviderErrorKind::Transport, "token poll failed"))
                .and_then(|response| reject_redirect(response, "token poll"))?
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
            interval += Duration::from_secs(token.interval.unwrap_or(5));
            continue;
        }
        if let Some(access) = token.access_token.clone().filter(|value| !value.is_empty()) {
            let username =
                fetch_authenticated_login(&endpoints.api_origin, &access, LOGIN_HTTP_TIMEOUT)
                    .await
                    .unwrap_or_default();
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_secs())
                .unwrap_or(0);
            let now = unix_to_rfc3339(now_secs);
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
                        .map(|seconds| unix_to_rfc3339(now_secs.saturating_add(seconds))),
                    refresh_token_expires_at: token
                        .refresh_token_expires_in
                        .map(|seconds| unix_to_rfc3339(now_secs.saturating_add(seconds))),
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
        .and_then(|url| super::GitHubEndpoint::new(url).ok())
        .and_then(|endpoint| {
            let transport = super::GitHubTransport::new(
                endpoint.clone(),
                std::sync::Arc::new(super::StaticCredentialResolver::new(
                    token.to_owned(),
                    super::CredentialSource::Override,
                )),
                super::RetryPolicy {
                    max_attempts: 2,
                    ..Default::default()
                },
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
            super::RequestSpec::get(url),
            &super::RequestContext::with_timeout(timeout, 1024 * 1024),
        )
        .await
    {
        Ok(page) => TokenCheck::Valid(
            serde_json::from_slice::<serde_json::Value>(&page.body)
                .ok()
                .and_then(|value| value.get("login")?.as_str().map(str::to_owned)),
        ),
        Err(error) if error.kind == super::ProviderErrorKind::Authentication => {
            TokenCheck::Rejected
        }
        Err(_) => TokenCheck::Unverified,
    }
}

/// The login GitHub reports for `token`, or `None` on any failure.
pub async fn fetch_authenticated_login(
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
    unix_to_rfc3339(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0),
    )
}

fn unix_to_rfc3339(secs: u64) -> String {
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = crate::civil_date::civil_from_days((secs / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

pub fn is_token_expired(credentials: &StoredCredentials) -> bool {
    let Some(expires_at) = credentials.token.expires_at.as_deref() else {
        return false;
    };
    match parse_expiry(expires_at) {
        None => true,
        Some(when) => when
            .duration_since(SystemTime::now())
            .map(|left| left < Duration::from_secs(5 * 60))
            .unwrap_or(true),
    }
}

pub fn is_refresh_token_expired(credentials: &StoredCredentials) -> bool {
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
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(UNIX_EPOCH + Duration::from_secs(secs));
    }
    let trimmed = trimmed
        .trim_end_matches('Z')
        .split_once('+')
        .map(|(head, _)| head)
        .unwrap_or(trimmed.trim_end_matches('Z'));
    let (date, time) = trimmed.split_once('T')?;
    let mut date = date.split('-');
    let year: i32 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;
    let time = time.split('.').next().unwrap_or(time);
    let mut time = time.split(':');
    let hour: u32 = time.next()?.parse().ok()?;
    let minute: u32 = time.next()?.parse().ok()?;
    let second: u32 = time.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    let secs = days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    Some(UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    Some(crate::civil_date::days_from_civil(
        i64::from(year),
        i64::from(month),
        i64::from(day),
    ))
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

/// An advisory file lock (`flock` / `LockFileEx`) held for one
/// load-refresh-store sequence. The OS drops it if the process dies, so a
/// crashed refresher never leaves a stale lock behind.
struct RefreshFileLock(std::fs::File);

impl Drop for RefreshFileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

async fn acquire_refresh_lock(
    path: &std::path::Path,
    wait: Duration,
) -> Result<RefreshFileLock, ProviderError> {
    let unavailable = || {
        ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "cannot open the GitHub token refresh lock",
        )
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| unavailable())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| unavailable())?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(RefreshFileLock(file)),
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(_)) => return Err(unavailable()),
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
    use sha2::{Digest, Sha256};
    if client_id.trim().is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::Authentication,
            "Set OCTOCODE_GITHUB_CLIENT_ID to refresh credentials for this GitHub host",
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
            hex::encode(Sha256::digest(endpoints.host.as_bytes()))
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

pub async fn refresh_auth_token(
    host: &str,
    client_id: &str,
) -> Result<StoredCredentials, ProviderError> {
    refresh_auth_token_in_store(host, client_id, &CredentialStore::from_process()?).await
}

pub async fn refresh_auth_token_in_store(
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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let updated = StoredCredentials {
        hostname: stored.hostname.clone(),
        username: stored.username.clone(),
        token: OAuthToken {
            token: token.access,
            token_type: token.token_type,
            scopes: stored.token.scopes.clone(),
            refresh_token: token.refresh_token.or(stored.token.refresh_token.clone()),
            expires_at: token
                .expires_in
                .map(|seconds| unix_to_rfc3339(now.saturating_add(seconds))),
            refresh_token_expires_at: token
                .refresh_token_expires_in
                .map(|seconds| unix_to_rfc3339(now.saturating_add(seconds))),
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
    let client = login_client()?;
    let token: TokenResponse = auth_call(endpoints, &CancellationToken::new(), async {
        client
            .post(format!("{}/login/oauth/access_token", endpoints.web_origin))
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, "octocode-native")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(refresh_token_form(client_id, refresh_token))
            .send()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Transport, "token refresh request failed")
            })
            .and_then(|response| reject_redirect(response, "token refresh request"))?
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

pub async fn refresh_auth_token_result(
    host: Option<&str>,
    client_id: Option<&str>,
) -> RefreshResult {
    let host = host.unwrap_or("github.com");
    refresh_result(
        host,
        refresh_auth_token(host, client_id_for_host(host, client_id)).await,
    )
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

pub async fn get_token_with_refresh(
    host: Option<&str>,
    client_id: Option<&str>,
) -> TokenWithRefreshResult {
    match CredentialStore::from_process() {
        Ok(store) => get_token_with_refresh_in_store(host, client_id, &store).await,
        Err(error) => token_refresh_error(error),
    }
}

fn token_refresh_error(error: ProviderError) -> TokenWithRefreshResult {
    TokenWithRefreshResult {
        token: None,
        source: "none",
        username: None,
        refresh_error: Some(mask_token_text(&error.message)),
    }
}

pub async fn get_token_with_refresh_in_store(
    host: Option<&str>,
    client_id: Option<&str>,
    store: &CredentialStore,
) -> TokenWithRefreshResult {
    let host = host.unwrap_or("github.com");
    let endpoints = LoginEndpoints::from_host(host);
    let (stored, source) = match store.load(&endpoints.host) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return TokenWithRefreshResult {
                token: None,
                source: "none",
                username: None,
                refresh_error: None,
            };
        }
        Err(error) => return token_refresh_error(error),
    };
    if !is_token_expired(&stored) {
        return TokenWithRefreshResult {
            token: Some(stored.token.token),
            source: "stored",
            username: Some(stored.username),
            refresh_error: None,
        };
    }
    match refresh_selected(
        &endpoints,
        client_id_for_host(host, client_id),
        RefreshMode::IfExpired,
        store,
        source,
    )
    .await
    {
        Ok(None) => TokenWithRefreshResult {
            token: None,
            source: "none",
            username: None,
            refresh_error: None,
        },
        Ok(Some(updated)) => TokenWithRefreshResult {
            token: Some(updated.token.token),
            source: "refreshed",
            username: Some(updated.username),
            refresh_error: None,
        },
        Err(error) => token_refresh_error(error),
    }
}

pub async fn resolve_stored_with_refresh(
    host: &str,
    client_id: &str,
) -> Result<Option<ResolvedCredential>, ProviderError> {
    let store = CredentialStore::from_process()?;
    let Some((stored, source)) = store.load(host)? else {
        return Ok(None);
    };
    if !is_token_expired(&stored) {
        return Ok(Some(ResolvedCredential::new(stored.token.token, source)));
    }
    if client_id.trim().is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "OCTOCODE_GITHUB_CLIENT_ID is required to refresh GitHub Enterprise credentials",
        ));
    }
    Ok(refresh_stored_in_store(host, client_id, &store, source)
        .await?
        .map(|stored| ResolvedCredential::new(stored.token.token, source)))
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
        login_device_flow_cancellable, parse_granted_scopes, refresh_locked, refresh_token_form,
        token_poll_form, unix_to_rfc3339,
    };
    use crate::providers::github::OAuthToken;
    use crate::providers::github::{ProviderError, ProviderErrorKind};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        let future = unix_to_rfc3339(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + 3600,
        );
        assert!(!is_token_expired(&stored(Some(&future), Some("r"))));
        let soon = unix_to_rfc3339(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + 30,
        );
        assert!(is_token_expired(&stored(Some(&soon), Some("r"))));
        assert!(!is_token_expired(&stored(None, Some("r"))));
        assert!(!is_refresh_token_expired(&stored(None, Some("r"))));
    }

    #[tokio::test]
    async fn refresh_posts_to_web_origin_and_keeps_username() {
        let server = MockServer::start().await;
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
        let past = unix_to_rfc3339(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - 60,
        );
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
            let server = MockServer::start().await;
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
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
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
        let other = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "gho_stolen", "device_code": "d", "user_code": "u",
                "verification_uri": "https://x"
            })))
            .expect(0)
            .mount(&other)
            .await;
        let server = MockServer::start().await;
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
        let device =
            login_device_flow_cancellable(&endpoints(&server), "cid", &CancellationToken::new())
                .await
                .expect_err("redirect refused");
        assert_eq!(device.kind, ProviderErrorKind::RedirectDenied);
        assert_eq!(device.status, Some(308));
        other.verify().await;
    }
}
