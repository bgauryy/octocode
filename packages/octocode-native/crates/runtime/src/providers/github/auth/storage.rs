use super::super::{ProviderError, ProviderErrorKind};
use super::normalize_host;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn delete_platform_credential(host: &str) -> Result<(), ProviderError> {
    let Ok(entry) = platform_entry(host) else {
        return Ok(());
    };
    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(map_store_error(error)),
    }
}

/// The `octocode` keychain entry for `host` in the platform credential store.
fn platform_entry(host: &str) -> Result<keyring_core::Entry, ProviderError> {
    use keyring_core::api::CredentialStoreApi;
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = host;
        return Err(ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "no secure credential store is available on this platform",
        ));
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let store = store.map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::CredentialStoreUnavailable,
                "secure credential store is unavailable",
            )
        })?;
        store.build("octocode", host, None).map_err(map_store_error)
    }
}

pub fn load_stored_credentials(host: &str) -> Result<Option<StoredCredentials>, ProviderError> {
    let host = normalize_host(host);
    let Some(blob) = load_platform_password(&host)? else {
        return Ok(None);
    };
    if let Ok(stored) = serde_json::from_str::<StoredCredentials>(&blob) {
        return Ok(Some(stored));
    }
    let token = blob.trim();
    if token.is_empty() {
        return Ok(None);
    }
    Ok(Some(StoredCredentials {
        hostname: host,
        username: String::new(),
        token: OAuthToken {
            token: token.to_owned(),
            token_type: "oauth".into(),
            scopes: None,
            refresh_token: None,
            expires_at: None,
            refresh_token_expires_at: None,
        },
        git_protocol: "https".into(),
        created_at: String::new(),
        updated_at: String::new(),
    }))
}

fn load_platform_password(host: &str) -> Result<Option<String>, ProviderError> {
    match platform_entry(&normalize_host(host))?.get_password() {
        Ok(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
        Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(map_store_error(error)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredentials {
    pub hostname: String,
    pub username: String,
    pub token: OAuthToken,
    pub git_protocol: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthToken {
    pub token: String,
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_expires_at: Option<String>,
}

impl std::fmt::Debug for OAuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthToken")
            .field("token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("scopes", &self.scopes)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("refresh_token_expires_at", &self.refresh_token_expires_at)
            .finish()
    }
}

pub fn token_from_stored_blob(blob: &str) -> SecretString {
    let trimmed = blob.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        && let Some(token) = value.pointer("/token/token").and_then(Value::as_str)
        && !token.is_empty()
    {
        return SecretString::from(token);
    }
    SecretString::from(trimmed)
}

pub fn store_platform_credential(credentials: &StoredCredentials) -> Result<(), ProviderError> {
    let payload = serde_json::to_string(credentials).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "failed to encode stored credentials",
        )
    })?;
    set_platform_password(&credentials.hostname, &payload)
}

fn set_platform_password(host: &str, payload: &str) -> Result<(), ProviderError> {
    platform_entry(host)?
        .set_password(payload)
        .map_err(map_store_error)
}

fn map_store_error(error: keyring_core::Error) -> ProviderError {
    let kind = if matches!(error, keyring_core::Error::NoEntry) {
        ProviderErrorKind::NotFound
    } else {
        ProviderErrorKind::CredentialStoreUnavailable
    };
    ProviderError::new(kind, "secure credential store operation failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_credentials_debug_never_exposes_access_or_refresh_tokens() {
        let stored: StoredCredentials = serde_json::from_value(serde_json::json!({
            "hostname":"example.test", "username":"fixture-user",
            "token":{"token":"synthetic-access-secret", "tokenType":"oauth",
                "refreshToken":"synthetic-refresh-secret"},
            "gitProtocol":"https", "createdAt":"", "updatedAt":""
        }))
        .unwrap();
        assert!(!format!("{stored:?}").contains("synthetic-"));
        assert!(!format!("{:?}", stored.token).contains("synthetic-"));
    }
}
