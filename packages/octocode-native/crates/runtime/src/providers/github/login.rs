//! GitHub device-flow login (no TUI).
mod device;
#[cfg(test)]
mod flow_tests;
mod http;
mod refresh;
use device::TokenResponse;
pub use device::login_device_flow_in_store;
use http::{
    LOGIN_HTTP_TIMEOUT, auth_call, device_code_form, expires_after, login_cancelled, login_client,
    parse_granted_scopes, post_form, refresh_token_form, rfc3339_now, token_poll_form,
};
pub use refresh::refresh_auth_token_result_in_store;
pub(crate) use refresh::refresh_stored_in_store;

use super::StoredCredentials;
use octocode_github::ProviderErrorKind;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// One authenticated read through the shared executor (throttling, rate-limit
/// bookkeeping, bounded retries).
async fn authenticated_read(
    api_url: &str,
    route: &str,
    token: &str,
    timeout: Duration,
) -> Option<Result<octocode_github::ResponsePage, octocode_github::ProviderError>> {
    let endpoint = octocode_github::GitHubEndpoint::new(url::Url::parse(api_url).ok()?).ok()?;
    let transport = octocode_github::GitHubTransport::new(
        endpoint.clone(),
        octocode_github::RetryPolicy {
            max_attempts: 2,
            ..Default::default()
        },
        octocode_github::GitHubBudget::global(),
    )
    .ok()?;
    let url = endpoint.rest(&[route]).ok()?;
    Some(
        transport
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
            .await,
    )
}

/// One GitHub quota bucket: what the token has left of it, and when it resets.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub resource: String,
    pub remaining: u64,
    pub limit: u64,
    pub reset_at: String,
}

/// The token's quota buckets the tools draw on (`GET /rate_limit`, which GitHub
/// does not charge); empty when GitHub could not be asked or has no limits
/// (a GitHub Enterprise server with rate limiting off).
pub async fn quota(api_url: &str, token: &str, timeout: Duration) -> Vec<Quota> {
    let Some(Ok(page)) = authenticated_read(api_url, "rate_limit", token, timeout).await else {
        return Vec::new();
    };
    let Ok(answer) = serde_json::from_slice::<serde_json::Value>(&page.body) else {
        return Vec::new();
    };
    ["core", "search", "code_search", "graphql"]
        .into_iter()
        .filter_map(|resource| {
            let bucket = answer.pointer(&format!("/resources/{resource}"))?;
            let reset = i64::try_from(bucket.get("reset")?.as_u64()?).ok()?;
            Some(Quota {
                resource: resource.to_owned(),
                remaining: bucket.get("remaining")?.as_u64()?,
                limit: bucket.get("limit")?.as_u64()?,
                reset_at: crate::civil_date::iso8601_secs(reset),
            })
        })
        .collect()
}

/// `GET /user` through the shared executor: whether GitHub accepts `token`.
pub async fn verify_token(api_url: &str, token: &str, timeout: Duration) -> TokenCheck {
    let Some(read) = authenticated_read(api_url, "user", token, timeout).await else {
        return TokenCheck::Unverified;
    };
    match read {
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
        Err(_) => u64::try_from(crate::civil_date::parse_rfc3339(trimmed)?.0).unwrap_or(0),
    };
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

fn mask_token_text(message: &str) -> String {
    let mut out = message.to_owned();
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        while let Some(start) = out.find(prefix) {
            let rest = &out[start + prefix.len()..];
            let take = rest.chars().take_while(char::is_ascii_alphanumeric).count();
            if take >= 36 {
                out.replace_range(start..start + prefix.len() + take, "***MASKED***");
            } else {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod quota_tests {
    use super::quota;
    use std::time::Duration;
    use wiremock::matchers::{header, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn quota_names_each_bucket_the_tools_draw_on() {
        let server = MockServer::builder().start().await;
        Mock::given(path("/api/v3/rate_limit"))
            .and(header("authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "resources": {
                    "core": {"limit": 5000, "remaining": 4321, "reset": 86400},
                    "search": {"limit": 30, "remaining": 29, "reset": 86400},
                    "code_search": {"limit": 10, "remaining": 9, "reset": 86400},
                    "graphql": {"limit": 5000, "remaining": 5000, "reset": 86400},
                    "dependency_snapshots": {"limit": 100, "remaining": 100, "reset": 86400}
                }
            })))
            .mount(&server)
            .await;
        let api = format!("{}/api/v3", server.uri());
        let buckets = quota(&api, "t", Duration::from_secs(5)).await;
        let names: Vec<_> = buckets.iter().map(|q| q.resource.as_str()).collect();
        assert_eq!(names, ["core", "search", "code_search", "graphql"]);
        assert_eq!((buckets[0].remaining, buckets[0].limit), (4321, 5000));
        assert_eq!(buckets[0].reset_at, "1970-01-02T00:00:00Z");
        assert!(
            quota(&api, "wrong", Duration::from_secs(5))
                .await
                .is_empty()
        );
    }
}
