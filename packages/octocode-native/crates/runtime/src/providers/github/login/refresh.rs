use super::super::{CredentialStore, OAuthToken, StoredCredentials};
use super::{
    LoginEndpoints, RefreshResult, TokenResponse, auth_call, expires_after,
    is_refresh_token_expired, is_token_expired, login_client, mask_token_text, post_form,
    refresh_token_form, rfc3339_now,
};
use crate::civil_date::unix_secs;
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How long a refresher waits for another process to finish its refresh:
/// the OAuth call's own admission wait plus its HTTP timeout.
const REFRESH_LOCK_WAIT: Duration = Duration::from_secs(45);

/// Where stored credentials are read and written during a refresh, and the
/// lock file that serializes refreshes across processes. Platform credentials
/// use the per-user lock; home credentials use the resolved Octocode home.
pub(super) struct RefreshStore<'a> {
    pub(super) load: &'a (dyn Fn(&str) -> Result<Option<StoredCredentials>, ProviderError> + Sync),
    pub(super) store:
        &'a (dyn Fn(&StoredCredentials, &StoredCredentials) -> Result<(), ProviderError> + Sync),
    pub(super) lock_path: std::path::PathBuf,
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
        octocode_io::lock::open_lock_file(&lock_path)
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
pub(super) enum RefreshMode {
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
pub(super) async fn refresh_locked(
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

pub(super) async fn refresh_selected(
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

pub(super) async fn refresh_stored_credentials(
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

pub(super) struct RefreshedToken {
    pub(super) access: String,
    token_type: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
}

pub(super) async fn exchange_refresh_token(
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
