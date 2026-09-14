//! GitHub device-flow HTTP and empty-secret GitHub App token refresh.
use std::{
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Client,
    header::{ACCEPT, AUTHORIZATION, USER_AGENT},
};
use serde_json::{Value, json};
use url::Url;

use super::{
    CredentialSource, GitHubEndpoint, OAuthToken, ProviderError, ProviderErrorKind,
    ResolvedCredential, StoredCredentials, load_stored_credential, store_platform_credential,
};

pub const GITHUB_APP_CLIENT_ID: &str = "178c6fc778ccc68e1d6a";
pub const DEFAULT_SCOPES: &[&str] = &["repo", "read:org", "gist"];
const DEFAULT_HOSTNAME: &str = "github.com";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const USER_AGENT_VALUE: &str = "octocode-native";
const ACCESS_SKEW: Duration = Duration::from_secs(5 * 60);
const SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginOrigins {
    pub web: Url,
    pub api: Url,
    pub hostname: String,
}

impl LoginOrigins {
    pub fn from_hostname(hostname: &str) -> Result<Self, ProviderError> {
        let hostname = normalize_hostname(hostname);
        if hostname.is_empty() {
            return Err(config_error("GitHub hostname is empty"));
        }
        if is_github_dot_com(&hostname) {
            return github_dot_com();
        }
        let web = Url::parse(&format!("https://{hostname}/")).map_err(|_| invalid_url())?;
        let api = Url::parse(&format!("https://{hostname}/api/v3/")).map_err(|_| invalid_url())?;
        Ok(Self { web, api, hostname })
    }

    pub fn from_api_url(api_url: &str) -> Result<Self, ProviderError> {
        let mut api = Url::parse(api_url.trim()).map_err(|_| invalid_url())?;
        if !matches!(api.scheme(), "http" | "https") || api.host_str().is_none() {
            return Err(invalid_url());
        }
        if !api.path().ends_with('/') {
            api.set_path(&format!("{}/", api.path()));
        }
        if api.host_str() == Some("api.github.com") {
            return github_dot_com();
        }
        let hostname = api.host_str().unwrap_or_default().to_ascii_lowercase();
        let mut web = api.clone();
        web.set_path("/");
        web.set_query(None);
        web.set_fragment(None);
        Ok(Self { web, api, hostname })
    }

    pub fn from_endpoint(endpoint: &GitHubEndpoint) -> Result<Self, ProviderError> {
        Self::from_api_url(endpoint.rest_base().as_str())
    }
}

fn github_dot_com() -> Result<LoginOrigins, ProviderError> {
    Ok(LoginOrigins {
        web: Url::parse("https://github.com/").map_err(|_| invalid_url())?,
        api: Url::parse("https://api.github.com/").map_err(|_| invalid_url())?,
        hostname: DEFAULT_HOSTNAME.into(),
    })
}

pub trait CredentialIo: Send + Sync {
    fn store(&self, credentials: &StoredCredentials) -> Result<(), ProviderError>;
    fn load(&self, host: &str) -> Result<Option<StoredCredentials>, ProviderError>;
}

pub struct PlatformIo;

impl CredentialIo for PlatformIo {
    fn store(&self, credentials: &StoredCredentials) -> Result<(), ProviderError> {
        store_platform_credential(credentials)
    }
    fn load(&self, host: &str) -> Result<Option<StoredCredentials>, ProviderError> {
        load_stored_credential(host)
    }
}

pub struct Verification {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    device_code: String,
}

pub struct LoginSuccess {
    pub username: String,
    pub hostname: String,
}

pub struct DeviceClient {
    http: Client,
    origins: LoginOrigins,
    client_id: String,
    skip_sleep: bool,
}

impl DeviceClient {
    pub fn new(origins: LoginOrigins, client_id: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            http: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| config_error("failed to build HTTP client"))?,
            origins,
            client_id: client_id.into(),
            skip_sleep: false,
        })
    }

    pub fn origins(&self) -> &LoginOrigins {
        &self.origins
    }

    #[cfg(test)]
    fn skipping_sleep(mut self) -> Self {
        self.skip_sleep = true;
        self
    }

    pub async fn login(
        &self,
        io: &dyn CredentialIo,
        scopes: &[String],
        on_verification: &mut dyn FnMut(&Verification),
    ) -> Result<LoginSuccess, ProviderError> {
        let scopes = if scopes.is_empty() {
            default_scopes()
        } else {
            scopes.to_vec()
        };
        let device = self.request_device_code(&scopes).await?;
        on_verification(&device);
        let token = self.poll_access_token(&device).await?;
        let username = self.fetch_username(&token.access_token).await?;
        let stored = stored_from_access_token(&self.origins.hostname, &username, &token, &scopes)?;
        io.store(&stored)?;
        Ok(LoginSuccess {
            username,
            hostname: self.origins.hostname.clone(),
        })
    }

    pub async fn refresh(&self, io: &dyn CredentialIo) -> Result<StoredCredentials, ProviderError> {
        let Some(mut stored) = io.load(&self.origins.hostname)? else {
            return Err(typed(
                "credential.refreshFailed",
                ProviderErrorKind::Authentication,
                format!("Not logged in to {}", self.origins.hostname),
            ));
        };
        let Some(refresh_token) = stored
            .token
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            return Err(typed(
                "credential.refreshUnsupported",
                ProviderErrorKind::Authentication,
                "Token does not support refresh. Run `octo login`.",
            ));
        };
        if refresh_token_expired(&stored) {
            return Err(typed(
                "credential.refreshExpired",
                ProviderErrorKind::Authentication,
                "Refresh token has expired. Run `octo login`.",
            ));
        }
        let token = match self.refresh_access_token(&refresh_token).await {
            Ok(token) => token,
            Err(error)
                if error.kind == ProviderErrorKind::Authentication
                    || error.kind == ProviderErrorKind::Permission
                    || matches!(error.status, Some(401 | 403)) =>
            {
                return Err(typed(
                    "credential.refreshFailed",
                    ProviderErrorKind::Authentication,
                    "Token refresh failed. Run `octo login`.",
                ));
            }
            Err(error) => return Err(error),
        };
        apply_access_token(&mut stored, &token);
        stored.updated_at = now_rfc3339();
        io.store(&stored)?;
        Ok(stored)
    }

    async fn request_device_code(&self, scopes: &[String]) -> Result<Verification, ProviderError> {
        let url = web_path(&self.origins.web, "/login/device/code")?;
        let value = self
            .post_oauth(
                url,
                json!({
                    "client_id": self.client_id,
                    "scope": scopes.join(","),
                }),
            )
            .await?;
        if let Some(error) = oauth_error(&value) {
            return Err(error);
        }
        Ok(Verification {
            device_code: required_string(&value, "device_code")?,
            user_code: required_string(&value, "user_code")?,
            verification_uri: required_string(&value, "verification_uri")?,
            expires_in: json_u64(&value, "expires_in").unwrap_or(900),
            interval: json_u64(&value, "interval").unwrap_or(5),
        })
    }

    async fn poll_access_token(&self, device: &Verification) -> Result<AccessToken, ProviderError> {
        let url = web_path(&self.origins.web, "/login/oauth/access_token")?;
        let deadline = Instant::now() + Duration::from_secs(device.expires_in.max(1));
        let mut interval = Duration::from_secs(device.interval);
        loop {
            if Instant::now() >= deadline {
                return Err(typed(
                    "credential.refreshFailed",
                    ProviderErrorKind::Timeout,
                    "Device authorization timed out. Run `octo login`.",
                ));
            }
            let value = self
                .post_oauth(
                    url.clone(),
                    json!({
                        "client_id": self.client_id,
                        "device_code": device.device_code,
                        "grant_type": DEVICE_GRANT_TYPE,
                    }),
                )
                .await?;
            if let Some(code) = value.get("error").and_then(Value::as_str) {
                match code {
                    "authorization_pending" => {
                        self.sleep(interval).await;
                        continue;
                    }
                    "slow_down" => {
                        interval = interval.saturating_add(SLOW_DOWN_INCREMENT);
                        self.sleep(interval).await;
                        continue;
                    }
                    "expired_token" => {
                        return Err(typed(
                            "credential.refreshFailed",
                            ProviderErrorKind::Authentication,
                            "Device code expired. Run `octo login`.",
                        ));
                    }
                    "access_denied" => {
                        return Err(typed(
                            "credential.refreshFailed",
                            ProviderErrorKind::Authentication,
                            "Device authorization was denied. Run `octo login`.",
                        ));
                    }
                    other => return Err(oauth_named(other, &value)),
                }
            }
            return parse_access_token(&value);
        }
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<AccessToken, ProviderError> {
        let url = web_path(&self.origins.web, "/login/oauth/access_token")?;
        let value = self
            .post_oauth(
                url,
                json!({
                    "client_id": self.client_id,
                    "client_secret": "",
                    "grant_type": "refresh_token",
                    "refresh_token": refresh_token,
                }),
            )
            .await?;
        if let Some(error) = oauth_error(&value) {
            return Err(error);
        }
        parse_access_token(&value)
    }

    async fn fetch_username(&self, token: &str) -> Result<String, ProviderError> {
        let url = self.origins.api.join("user").map_err(|_| invalid_url())?;
        let response = self
            .http
            .get(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/vnd.github+json")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .map_err(|error| transport_error(&error))?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "failed to decode GitHub user")
        })?;
        if !status.is_success() {
            return Err(http_status_error(status.as_u16(), &value));
        }
        value
            .get("login")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|login| !login.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                ProviderError::new(ProviderErrorKind::Decode, "GitHub user is missing login")
            })
    }

    async fn post_oauth(&self, url: Url, body: Value) -> Result<Value, ProviderError> {
        let response = self
            .http
            .post(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| transport_error(&error))?;
        let status = response.status().as_u16();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if matches!(status, 401 | 403) {
            return Err(http_status_error(status, &value));
        }
        if value.is_null() && !(200..300).contains(&status) {
            return Err(http_status_error(status, &value));
        }
        Ok(value)
    }

    async fn sleep(&self, duration: Duration) {
        if self.skip_sleep || duration.is_zero() {
            return;
        }
        tokio::time::sleep(duration).await;
    }
}

struct AccessToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
    scope: Option<String>,
}

fn parse_access_token(value: &Value) -> Result<AccessToken, ProviderError> {
    if let Some(error) = oauth_error(value) {
        return Err(error);
    }
    Ok(AccessToken {
        access_token: required_string(value, "access_token")?,
        refresh_token: json_string(value, "refresh_token"),
        expires_in: json_u64(value, "expires_in"),
        refresh_token_expires_in: json_u64(value, "refresh_token_expires_in"),
        scope: json_string(value, "scope"),
    })
}

fn stored_from_access_token(
    hostname: &str,
    username: &str,
    token: &AccessToken,
    requested_scopes: &[String],
) -> Result<StoredCredentials, ProviderError> {
    let now = now_rfc3339();
    let scopes = token
        .scope
        .as_deref()
        .map(split_scopes)
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| requested_scopes.to_vec());
    Ok(StoredCredentials {
        hostname: hostname.to_owned(),
        username: username.to_owned(),
        token: OAuthToken {
            token: token.access_token.clone(),
            token_type: "oauth".to_owned(),
            scopes: Some(scopes),
            refresh_token: token.refresh_token.clone(),
            expires_at: token.expires_in.map(offset_rfc3339),
            refresh_token_expires_at: token.refresh_token_expires_in.map(offset_rfc3339),
        },
        git_protocol: "https".to_owned(),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn apply_access_token(stored: &mut StoredCredentials, token: &AccessToken) {
    stored.token.token = token.access_token.clone();
    stored.token.token_type = "oauth".to_owned();
    if let Some(refresh) = &token.refresh_token {
        stored.token.refresh_token = Some(refresh.clone());
    }
    if let Some(expires_in) = token.expires_in {
        stored.token.expires_at = Some(offset_rfc3339(expires_in));
    }
    if let Some(expires_in) = token.refresh_token_expires_in {
        stored.token.refresh_token_expires_at = Some(offset_rfc3339(expires_in));
    }
    if let Some(scope) = token.scope.as_deref() {
        let scopes = split_scopes(scope);
        if !scopes.is_empty() {
            stored.token.scopes = Some(scopes);
        }
    }
}

pub async fn refresh_auth_token(
    host: &str,
    client_id: &str,
) -> Result<StoredCredentials, ProviderError> {
    let origins = LoginOrigins::from_hostname(host)?;
    let client = DeviceClient::new(origins, client_id)?;
    client.refresh(&PlatformIo).await
}

pub async fn refresh_storage_if_needed(
    origins: &LoginOrigins,
    client_id: &str,
    current: Option<ResolvedCredential>,
) -> Option<ResolvedCredential> {
    let current = current?;
    if current.source != CredentialSource::Storage {
        return Some(current);
    }
    let Ok(Some(stored)) = load_stored_credential(&origins.hostname) else {
        return Some(current);
    };
    if !access_token_expired(&stored) {
        return Some(current);
    }
    let Ok(client) = DeviceClient::new(origins.clone(), client_id) else {
        return Some(current);
    };
    match client.refresh(&PlatformIo).await {
        Ok(updated) => Some(ResolvedCredential::new(
            updated.token.token.clone(),
            CredentialSource::Storage,
        )),
        Err(_) => Some(current),
    }
}

pub fn parse_scopes(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return default_scopes();
    };
    let scopes = split_scopes(raw);
    if scopes.is_empty() {
        default_scopes()
    } else {
        scopes
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        return command
            .status()
            .map(|_| ())
            .map_err(|error| error.to_string());
    } else {
        Command::new("xdg-open")
    };
    command
        .arg(url)
        .status()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn default_scopes() -> Vec<String> {
    DEFAULT_SCOPES
        .iter()
        .map(|scope| (*scope).to_owned())
        .collect()
}

fn split_scopes(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == ',' || c.is_ascii_whitespace())
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize_hostname(host: &str) -> String {
    let lower = host.trim().to_ascii_lowercase();
    lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower)
        .trim_end_matches('/')
        .to_owned()
}

fn is_github_dot_com(host: &str) -> bool {
    host == DEFAULT_HOSTNAME || host == "api.github.com"
}

fn web_path(web: &Url, path: &str) -> Result<Url, ProviderError> {
    let mut url = web.clone();
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn required_string(value: &Value, key: &str) -> Result<String, ProviderError> {
    json_string(value, key).ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::Decode,
            format!("GitHub OAuth response is missing {key}"),
        )
    })
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    match value.get(key)? {
        Value::Number(number) => number
            .as_u64()
            .or_else(|| number.as_f64().map(|n| n as u64)),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn oauth_error(value: &Value) -> Option<ProviderError> {
    let code = value.get("error").and_then(Value::as_str)?;
    Some(oauth_named(code, value))
}

fn oauth_named(code: &str, value: &Value) -> ProviderError {
    let description = value
        .get("error_description")
        .and_then(Value::as_str)
        .unwrap_or(code);
    let mut error = typed(
        "credential.refreshFailed",
        ProviderErrorKind::Authentication,
        mask_secrets(description),
    );
    if matches!(
        code,
        "incorrect_client_credentials" | "bad_refresh_token" | "invalid_grant"
    ) {
        error.status = Some(401);
    }
    error
}

fn http_status_error(status: u16, value: &Value) -> ProviderError {
    let kind = match status {
        401 => ProviderErrorKind::Authentication,
        403 => ProviderErrorKind::Permission,
        404 => ProviderErrorKind::NotFound,
        400 | 422 => ProviderErrorKind::Validation,
        500..=599 => ProviderErrorKind::Server,
        _ => ProviderErrorKind::Transport,
    };
    let message = value
        .get("message")
        .or_else(|| value.get("error_description"))
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("GitHub OAuth request failed");
    let mut error = ProviderError::new(kind, mask_secrets(message));
    error.status = Some(status);
    error
}

fn transport_error(error: &reqwest::Error) -> ProviderError {
    ProviderError::new(
        if error.is_timeout() {
            ProviderErrorKind::Timeout
        } else {
            ProviderErrorKind::Transport
        },
        mask_secrets(&error.to_string()),
    )
}

fn typed(code: &str, kind: ProviderErrorKind, message: impl Into<String>) -> ProviderError {
    ProviderError::new(kind, format!("{code}: {}", message.into()))
}

fn config_error(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Configuration, message)
}

fn invalid_url() -> ProviderError {
    config_error("invalid GitHub URL")
}

fn mask_secrets(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = secret_end(&chars, i) {
            out.push_str("***MASKED***");
            i = end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn secret_end(chars: &[char], start: usize) -> Option<usize> {
    let prefixes = [
        ['g', 'h', 'p', '_'],
        ['g', 'h', 'o', '_'],
        ['g', 'h', 'u', '_'],
        ['g', 'h', 's', '_'],
        ['g', 'h', 'r', '_'],
    ];
    for prefix in prefixes {
        if chars[start..].starts_with(&prefix) {
            let mut end = start + prefix.len();
            while end < chars.len() && chars[end].is_ascii_alphanumeric() {
                end += 1;
            }
            if end - start >= 40 {
                return Some(end);
            }
        }
    }
    if chars[start].is_ascii_alphanumeric() {
        let mut end = start;
        while end < chars.len() && chars[end].is_ascii_alphanumeric() {
            end += 1;
        }
        if end - start >= 40 {
            return Some(end);
        }
    }
    None
}

fn access_token_expired(credentials: &StoredCredentials) -> bool {
    let Some(expires_at) = credentials.token.expires_at.as_deref() else {
        return false;
    };
    let Some(expires) = parse_rfc3339(expires_at) else {
        return true;
    };
    expires.saturating_sub(unix_now()) < ACCESS_SKEW.as_secs()
}

fn refresh_token_expired(credentials: &StoredCredentials) -> bool {
    let Some(expires_at) = credentials.token.refresh_token_expires_at.as_deref() else {
        return false;
    };
    let Some(expires) = parse_rfc3339(expires_at) else {
        return true;
    };
    unix_now() >= expires
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_rfc3339() -> String {
    rfc3339_from_unix(unix_now())
}

fn offset_rfc3339(seconds: u64) -> String {
    rfc3339_from_unix(unix_now().saturating_add(seconds))
}

fn rfc3339_from_unix(epoch: u64) -> String {
    let days = epoch / 86400;
    let rem = epoch % 86400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

fn parse_rfc3339(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.len() < 19
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || !matches!(value.as_bytes().get(10), Some(&b'T' | &b't' | &b' '))
        || value.as_bytes().get(13) != Some(&b':')
        || value.as_bytes().get(16) != Some(&b':')
    {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    let month: u32 = value.get(5..7)?.parse().ok()?;
    let day: u32 = value.get(8..10)?.parse().ok()?;
    let hour: u64 = value.get(11..13)?.parse().ok()?;
    let minute: u64 = value.get(14..16)?.parse().ok()?;
    let second: u64 = value.get(17..19)?.parse().ok()?;
    let rest = value[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if !(rest.is_empty() || rest == "Z" || rest == "z" || rest == "+00:00") {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(
        days.saturating_mul(86400)
            .saturating_add(hour.saturating_mul(3600))
            .saturating_add(minute.saturating_mul(60))
            .saturating_add(second),
    )
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * u64::from(mp) + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let z = era * 146_097 + doe as i64 - 719_468;
    u64::try_from(z).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{body_string_contains, header, method, path},
    };

    struct MemoryIo {
        inner: Mutex<Option<StoredCredentials>>,
    }

    impl MemoryIo {
        fn new() -> Self {
            Self {
                inner: Mutex::new(None),
            }
        }
        fn stored(&self) -> StoredCredentials {
            self.inner
                .lock()
                .expect("io lock")
                .clone()
                .expect("stored credentials")
        }
    }

    impl CredentialIo for MemoryIo {
        fn store(&self, credentials: &StoredCredentials) -> Result<(), ProviderError> {
            *self.inner.lock().expect("io lock") = Some(credentials.clone());
            Ok(())
        }
        fn load(&self, host: &str) -> Result<Option<StoredCredentials>, ProviderError> {
            Ok(self
                .inner
                .lock()
                .expect("io lock")
                .as_ref()
                .filter(|value| value.hostname == host)
                .cloned())
        }
    }

    fn sample_stored(
        hostname: &str,
        refresh: Option<&str>,
        refresh_exp: Option<&str>,
    ) -> StoredCredentials {
        StoredCredentials {
            hostname: hostname.into(),
            username: "alice".into(),
            token: OAuthToken {
                token: "gho_oldtokenoldtokenoldtokenoldtokenold12".into(),
                token_type: "oauth".into(),
                scopes: Some(vec!["repo".into()]),
                refresh_token: refresh.map(str::to_owned),
                expires_at: Some("2000-01-01T00:00:00Z".into()),
                refresh_token_expires_at: refresh_exp.map(str::to_owned),
            },
            git_protocol: "https".into(),
            created_at: "2000-01-01T00:00:00Z".into(),
            updated_at: "2000-01-01T00:00:00Z".into(),
        }
    }

    async fn client_for(server: &MockServer) -> DeviceClient {
        let origins = LoginOrigins::from_api_url(&format!("{}/api/v3", server.uri())).expect("url");
        DeviceClient::new(origins, GITHUB_APP_CLIENT_ID)
            .expect("client")
            .skipping_sleep()
    }

    #[test]
    fn github_dot_com_uses_web_origin_not_api_host() {
        let origins = LoginOrigins::from_api_url("https://api.github.com").expect("origins");
        assert_eq!(origins.hostname, "github.com");
        assert_eq!(origins.web.as_str(), "https://github.com/");
        assert_eq!(origins.api.as_str(), "https://api.github.com/");
        assert_ne!(origins.web.host_str(), Some("api.github.com"));
        assert_eq!(
            LoginOrigins::from_hostname("github.com").expect("named"),
            origins
        );
    }

    #[test]
    fn ghes_serves_device_flow_on_web_host() {
        let origins =
            LoginOrigins::from_api_url("https://ghe.example.com/api/v3").expect("origins");
        assert_eq!(origins.hostname, "ghe.example.com");
        assert_eq!(origins.web.as_str(), "https://ghe.example.com/");
        assert_eq!(origins.api.as_str(), "https://ghe.example.com/api/v3/");
        assert_eq!(
            web_path(&origins.web, "/login/device/code")
                .expect("path")
                .as_str(),
            "https://ghe.example.com/login/device/code"
        );
    }

    #[test]
    fn stored_credentials_from_device_token_are_node_shaped() {
        let token = AccessToken {
            access_token: "gho_new".into(),
            refresh_token: Some("ghr_refresh".into()),
            expires_in: Some(8),
            refresh_token_expires_in: Some(16),
            scope: Some("repo,read:org,gist".into()),
        };
        let stored = stored_from_access_token(
            "github.com",
            "alice",
            &token,
            &["repo".into(), "read:org".into(), "gist".into()],
        )
        .expect("stored");
        let json = serde_json::to_value(&stored).expect("json");
        assert_eq!(json["hostname"], "github.com");
        assert_eq!(json["username"], "alice");
        assert_eq!(json["gitProtocol"], "https");
        assert_eq!(json["token"]["token"], "gho_new");
        assert_eq!(json["token"]["tokenType"], "oauth");
        assert_eq!(json["token"]["refreshToken"], "ghr_refresh");
        assert!(json["token"]["expiresAt"].as_str().is_some());
        assert!(json["createdAt"].as_str().is_some());
    }

    #[tokio::test]
    async fn device_login_polls_web_and_reads_user_from_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/device/code"))
            .and(body_string_contains(GITHUB_APP_CLIENT_ID))
            .and(body_string_contains("repo,read:org,gist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "device-secret",
                "user_code": "ABCD-1234",
                "verification_uri": format!("{}/login/device", server.uri()),
                "expires_in": 900,
                "interval": 0
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v3/login/device/code"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("device-secret"))
            .and(body_string_contains(DEVICE_GRANT_TYPE))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "gho_loginloginloginloginloginloginlogin12",
                "token_type": "bearer",
                "scope": "repo,read:org,gist",
                "refresh_token": "ghr_refreshrefreshrefreshrefreshrefresh1",
                "expires_in": 28800,
                "refresh_token_expires_in": 15811200
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .and(header(
                "authorization",
                "Bearer gho_loginloginloginloginloginloginlogin12",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"alice"})))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let io = MemoryIo::new();
        let mut seen = None;
        let result = client
            .login(&io, &[], &mut |verification| {
                seen = Some((
                    verification.user_code.clone(),
                    verification.verification_uri.clone(),
                ));
            })
            .await
            .expect("login");
        assert_eq!(result.username, "alice");
        assert_eq!(
            seen.as_ref().map(|(code, _)| code.as_str()),
            Some("ABCD-1234")
        );
        let stored = io.stored();
        assert_eq!(stored.username, "alice");
        assert_eq!(
            stored.token.token,
            "gho_loginloginloginloginloginloginlogin12"
        );
        assert_eq!(
            stored.token.refresh_token.as_deref(),
            Some("ghr_refreshrefreshrefreshrefreshrefresh1")
        );
        let encoded = serde_json::to_value(&stored).expect("json");
        assert!(encoded.get("git_protocol").is_none());
        assert_eq!(encoded["token"]["tokenType"], "oauth");
    }

    #[derive(Clone)]
    struct PendingThenOk(Arc<AtomicUsize>);
    impl Respond for PendingThenOk {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            match self.0.fetch_add(1, Ordering::SeqCst) {
                0 => ResponseTemplate::new(200)
                    .set_body_json(json!({"error":"authorization_pending"})),
                1 => ResponseTemplate::new(200).set_body_json(json!({"error":"slow_down"})),
                _ => ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "gho_afterpendingafterpendingafterpending12",
                    "token_type": "bearer",
                    "scope": "repo"
                })),
            }
        }
    }

    #[tokio::test]
    async fn device_login_honors_pending_and_slow_down() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "pending-device",
                "user_code": "WXYZ-0000",
                "verification_uri": format!("{}/login/device", server.uri()),
                "expires_in": 900,
                "interval": 0
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(PendingThenOk(Arc::new(AtomicUsize::new(0))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"bob"})))
            .mount(&server)
            .await;
        let client = client_for(&server).await;
        let io = MemoryIo::new();
        let result = client
            .login(&io, &["repo".into()], &mut |_| {})
            .await
            .expect("login");
        assert_eq!(result.username, "bob");
        assert_eq!(
            io.stored().token.token,
            "gho_afterpendingafterpendingafterpending12"
        );
    }

    #[tokio::test]
    async fn refresh_posts_empty_secret_and_keeps_token_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("refresh_token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": "bad_refresh_token",
                "error_description": "gho_shouldmaskshouldmaskshouldmaskshould12 is invalid"
            })))
            .mount(&server)
            .await;
        let client = client_for(&server).await;
        let io = MemoryIo::new();
        io.store(&sample_stored(
            &client.origins().hostname,
            Some("ghr_refreshrefreshrefreshrefreshrefresh1"),
            Some("2099-01-01T00:00:00Z"),
        ))
        .expect("seed");
        let error = match client.refresh(&io).await {
            Err(error) => error,
            Ok(_) => panic!("refresh 401"),
        };
        assert!(error.message.contains("credential.refreshFailed"));
        assert_eq!(
            io.stored().token.token,
            "gho_oldtokenoldtokenoldtokenoldtokenold12"
        );
        assert!(
            !error
                .message
                .contains("gho_shouldmaskshouldmaskshouldmaskshould12")
        );
    }

    #[tokio::test]
    async fn refresh_succeeds_with_empty_client_secret() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("\"client_secret\":\"\""))
            .and(body_string_contains("\"grant_type\":\"refresh_token\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "gho_newtokennewtokennewtokennewtokennew12",
                "refresh_token": "ghr_newrefreshnewrefreshnewrefreshnew1",
                "expires_in": 28800,
                "refresh_token_expires_in": 15811200,
                "token_type": "bearer"
            })))
            .mount(&server)
            .await;
        let client = client_for(&server).await;
        let io = MemoryIo::new();
        io.store(&sample_stored(
            &client.origins().hostname,
            Some("ghr_refreshrefreshrefreshrefreshrefresh1"),
            Some("2099-01-01T00:00:00Z"),
        ))
        .expect("seed");
        let stored = client.refresh(&io).await.expect("refresh");
        assert_eq!(
            stored.token.token,
            "gho_newtokennewtokennewtokennewtokennew12"
        );
        assert_eq!(
            stored.token.refresh_token.as_deref(),
            Some("ghr_newrefreshnewrefreshnewrefreshnew1")
        );
        assert_eq!(stored.username, "alice");
    }

    #[tokio::test]
    async fn refresh_without_refresh_token_is_unsupported() {
        let server = MockServer::start().await;
        let client = client_for(&server).await;
        let io = MemoryIo::new();
        io.store(&sample_stored(&client.origins().hostname, None, None))
            .expect("seed");
        let error = match client.refresh(&io).await {
            Err(error) => error,
            Ok(_) => panic!("unsupported"),
        };
        assert!(error.message.contains("credential.refreshUnsupported"));
        assert_eq!(
            io.stored().token.token,
            "gho_oldtokenoldtokenoldtokenoldtokenold12"
        );
    }

    #[tokio::test]
    async fn refresh_token_expired_is_typed_and_keeps_store() {
        let server = MockServer::start().await;
        let client = client_for(&server).await;
        let io = MemoryIo::new();
        io.store(&sample_stored(
            &client.origins().hostname,
            Some("ghr_refreshrefreshrefreshrefreshrefresh1"),
            Some("2000-01-01T00:00:00Z"),
        ))
        .expect("seed");
        let error = match client.refresh(&io).await {
            Err(error) => error,
            Ok(_) => panic!("expired"),
        };
        assert!(error.message.contains("credential.refreshExpired"));
        assert_eq!(
            io.stored().token.token,
            "gho_oldtokenoldtokenoldtokenoldtokenold12"
        );
    }

    #[test]
    fn mask_redacts_github_tokens() {
        let masked = mask_secrets("failed gho_shouldmaskshouldmaskshouldmaskshould12 extra");
        assert!(!masked.contains("gho_shouldmaskshouldmaskshouldmaskshould12"));
        assert!(masked.contains("***MASKED***"));
    }
}
