use super::super::{CredentialStore, OAuthToken, StoredCredentials};
use super::{
    LOGIN_HTTP_TIMEOUT, LoginEndpoints, auth_call, device_code_form, expires_after,
    fetch_authenticated_login, login_cancelled, login_client, parse_granted_scopes, post_form,
    token_poll_form,
};
use crate::civil_date::unix_secs;
use octocode_github::{ProviderError, ProviderErrorKind};
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<u64>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TokenResponse {
    pub(super) access_token: Option<String>,
    pub(super) token_type: Option<String>,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: Option<u64>,
    pub(super) refresh_token_expires_in: Option<u64>,
    pub(super) scope: Option<String>,
    pub(super) error: Option<String>,
    pub(super) interval: Option<u64>,
}

/// One second of the device flow's clock: RFC 8628 states `interval` and
/// `expires_in` in seconds. Tests pass a shorter one so a poll costs
/// milliseconds instead of a real second.
const DEVICE_SECOND: Duration = Duration::from_secs(1);

/// RFC 8628 §3.5 `slow_down`: GitHub returns the new total `interval`; never
/// poll sooner than the current interval plus 5 s (`second` long each).
pub(super) fn slow_down_interval(
    current: Duration,
    returned: Option<u64>,
    second: Duration,
) -> Duration {
    let floor = current + second * 5;
    returned.map_or(floor, |secs| (second * secs_u32(secs)).max(floor))
}

/// A device-flow second count as a `Duration` multiplier.
fn secs_u32(secs: u64) -> u32 {
    u32::try_from(secs).unwrap_or(u32::MAX)
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

pub(super) async fn login_device_flow_with_store(
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
