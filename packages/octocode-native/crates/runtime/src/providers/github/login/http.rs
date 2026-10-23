use super::LoginEndpoints;
use crate::civil_date::unix_secs;
use octocode_github::{ProviderError, ProviderErrorKind};
use reqwest::header::{ACCEPT, USER_AGENT};
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub(super) const LOGIN_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// The OAuth web-origin client (device code, polling, refresh) pooled per
/// Tokio runtime; admission goes through the executor's `auth` group.
pub(super) fn login_client() -> Result<reqwest::Client, ProviderError> {
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
pub(super) async fn post_form(
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

pub(super) fn login_cancelled() -> ProviderError {
    ProviderError::new(ProviderErrorKind::Cancelled, "GitHub login cancelled")
}

/// Run one OAuth call under the host's `auth` throttling group (anonymous
/// key: these calls authenticate with the client id, not a token).
pub(super) async fn auth_call<T>(
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

pub(super) fn device_code_form(client_id: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", client_id)
        .append_pair("scope", "repo read:org gist")
        .finish()
}

pub(super) fn token_poll_form(client_id: &str, device_code: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", client_id)
        .append_pair("device_code", device_code)
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
        .finish()
}

pub(super) fn refresh_token_form(client_id: &str, refresh_token: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("client_id", client_id)
        .append_pair("refresh_token", refresh_token)
        .finish()
}

pub(super) fn parse_granted_scopes(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn rfc3339_now() -> String {
    crate::civil_date::iso8601_secs(unix_secs())
}

/// The RFC 3339 instant `seconds` after `now`.
pub(super) fn expires_after(now: i64, seconds: u64) -> String {
    crate::civil_date::iso8601_secs(now.saturating_add_unsigned(seconds))
}
