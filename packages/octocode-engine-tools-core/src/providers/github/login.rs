//! GitHub device-flow login (no TUI).
use super::{
    OAuthToken, ProviderError, ProviderErrorKind, ResolvedCredential, StoredCredentials,
    load_stored_credentials, store_platform_credential,
};

use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const GITHUB_APP_CLIENT_ID: &str = "178c6fc778ccc68e1d6a";

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

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithRefreshResult {
    pub token: Option<String>,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_error: Option<String>,
}

pub async fn login_device_flow(
    endpoints: &LoginEndpoints,
) -> Result<StoredCredentials, ProviderError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                "failed to initialize login HTTP client",
            )
        })?;
    let device: DeviceCode = client
        .post(format!("{}/login/device/code", endpoints.web_origin))
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "octocode-native")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(format!(
            "client_id={GITHUB_APP_CLIENT_ID}&scope=repo%2Cread%3Aorg%2Cgist"
        ))
        .send()
        .await
        .map_err(|_| {
            ProviderError::new(ProviderErrorKind::Transport, "device code request failed")
        })?
        .json()
        .await
        .map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid device code response")
        })?;
    eprintln!(
        "Open {} and enter code {}",
        device.verification_uri, device.user_code
    );
    let deadline = Instant::now() + Duration::from_secs(device.expires_in.unwrap_or(900).min(900));
    let mut interval = Duration::from_secs(device.interval.unwrap_or(5).max(1));
    while Instant::now() < deadline {
        tokio::time::sleep(interval).await;
        let token: TokenResponse = client
            .post(format!("{}/login/oauth/access_token", endpoints.web_origin))
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, "octocode-native")
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!(
                "client_id={GITHUB_APP_CLIENT_ID}&device_code={}&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
                urlencoding_device(&device.device_code)
            ))
            .send()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Transport, "token poll failed")
            })?
            .json()
            .await
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid token poll response")
            })?;
        if token.error.as_deref() == Some("authorization_pending") {
            continue;
        }
        if token.error.as_deref() == Some("slow_down") {
            interval += Duration::from_secs(token.interval.unwrap_or(5));
            continue;
        }
        if let Some(access) = token.access_token.clone().filter(|value| !value.is_empty()) {
            let username = fetch_username(&client, &endpoints.api_origin, &access)
                .await
                .unwrap_or_default();
            let now = rfc3339_now();
            let stored = StoredCredentials {
                hostname: endpoints.host.clone(),
                username,
                token: OAuthToken {
                    token: access,
                    token_type: token.token_type.unwrap_or_else(|| "oauth".into()),
                    scopes: Some(vec!["repo".into(), "read:org".into(), "gist".into()]),
                    refresh_token: token.refresh_token,
                    expires_at: None,
                    refresh_token_expires_at: None,
                },
                git_protocol: "https".into(),
                created_at: now.clone(),
                updated_at: now,
            };
            store_platform_credential(&stored)?;
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

async fn fetch_username(client: &reqwest::Client, api_origin: &str, token: &str) -> Option<String> {
    let value: serde_json::Value = client
        .get(format!("{api_origin}/user"))
        .header(ACCEPT, "application/vnd.github+json")
        .header(USER_AGENT, "octocode-native")
        .bearer_auth(token)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    value
        .get("login")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn urlencoding_device(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
    const DAYS_PER_400Y: i64 = 146_097;
    const DAYS_PER_100Y: i64 = 36_524;
    const DAYS_PER_4Y: i64 = 1_461;
    let z = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let mut days = z;
    let mut year = 1970i64;
    let cycles = days.div_euclid(DAYS_PER_400Y);
    year += cycles * 400;
    days = days.rem_euclid(DAYS_PER_400Y);
    let mut hundreds = days / DAYS_PER_100Y;
    if hundreds == 4 {
        hundreds = 3;
    }
    year += hundreds * 100;
    days -= hundreds * DAYS_PER_100Y;
    let fours = days / DAYS_PER_4Y;
    year += fours * 4;
    days -= fours * DAYS_PER_4Y;
    let mut ones = days / 365;
    if ones == 4 {
        ones = 3;
    }
    year += ones;
    days -= ones * 365;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let md = [
        31,
        28 + i64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for length in md {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    let day = days + 1;
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
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u32;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(i64::from(era) * 146_097 + i64::from(doe) - 719_468)
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

pub async fn refresh_auth_token(
    host: &str,
    client_id: &str,
) -> Result<StoredCredentials, ProviderError> {
    let endpoints = LoginEndpoints::from_host(host);
    let stored = load_stored_credentials(&endpoints.host)?.ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::Authentication,
            format!("Not logged in to {}", endpoints.host),
        )
    })?;
    refresh_stored_credentials(stored, &endpoints, client_id).await
}

pub async fn refresh_stored_credentials(
    stored: StoredCredentials,
    endpoints: &LoginEndpoints,
    client_id: &str,
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
    store_platform_credential(&updated)?;
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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                "failed to initialize refresh HTTP client",
            )
        })?;
    let token: TokenResponse = client
        .post(format!("{}/login/oauth/access_token", endpoints.web_origin))
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "octocode-native")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(format!(
            "grant_type=refresh_token&client_id={}&client_secret=&refresh_token={}",
            urlencoding_device(client_id),
            urlencoding_device(refresh_token)
        ))
        .send()
        .await
        .map_err(|_| {
            ProviderError::new(ProviderErrorKind::Transport, "token refresh request failed")
        })?
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
        })?;
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
    let client_id = client_id.unwrap_or(GITHUB_APP_CLIENT_ID);
    match refresh_auth_token(host, client_id).await {
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
    let host = host.unwrap_or("github.com");
    let endpoints = LoginEndpoints::from_host(host);
    let Ok(Some(stored)) = load_stored_credentials(&endpoints.host) else {
        return TokenWithRefreshResult {
            token: None,
            source: "none",
            username: None,
            refresh_error: None,
        };
    };
    if !is_token_expired(&stored) {
        return TokenWithRefreshResult {
            token: Some(stored.token.token),
            source: "stored",
            username: Some(stored.username),
            refresh_error: None,
        };
    }
    match refresh_stored_credentials(
        stored.clone(),
        &endpoints,
        client_id.unwrap_or(GITHUB_APP_CLIENT_ID),
    )
    .await
    {
        Ok(updated) => TokenWithRefreshResult {
            token: Some(updated.token.token),
            source: "refreshed",
            username: Some(updated.username),
            refresh_error: None,
        },
        Err(error) => TokenWithRefreshResult {
            token: None,
            source: "none",
            username: None,
            refresh_error: Some(mask_token_text(&error.message)),
        },
    }
}

pub async fn resolve_stored_with_refresh(
    host: &str,
) -> Result<Option<ResolvedCredential>, ProviderError> {
    let endpoints = LoginEndpoints::from_host(host);
    let Some(stored) = load_stored_credentials(&endpoints.host)? else {
        return Ok(None);
    };
    if !is_token_expired(&stored) {
        return Ok(Some(ResolvedCredential::new(
            stored.token.token,
            super::CredentialSource::Storage,
        )));
    }
    match refresh_stored_credentials(stored.clone(), &endpoints, GITHUB_APP_CLIENT_ID).await {
        Ok(updated) => Ok(Some(ResolvedCredential::new(
            updated.token.token,
            super::CredentialSource::Storage,
        ))),
        Err(_) => Ok(Some(ResolvedCredential::new(
            stored.token.token,
            super::CredentialSource::Storage,
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LoginEndpoints, StoredCredentials, exchange_refresh_token, is_refresh_token_expired,
        is_token_expired, unix_to_rfc3339,
    };
    use crate::providers::github::OAuthToken;
    use std::time::{SystemTime, UNIX_EPOCH};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn stored(expires_at: Option<&str>, refresh: Option<&str>) -> StoredCredentials {
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
}
