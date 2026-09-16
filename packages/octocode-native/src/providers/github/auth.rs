use std::{
    future::Future,
    pin::Pin,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::config::ConfigOutput;
use aes_gcm::{AesGcm, KeyInit, aead::AeadInOut, aead::consts::U16, aes::Aes256};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

use super::ProviderError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    Override,
    Environment,
    Storage,
}

#[derive(Clone)]
pub struct ResolvedCredential {
    secret: SecretString,
    pub source: CredentialSource,
}

impl ResolvedCredential {
    pub fn new(secret: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            secret: secret.into(),
            source,
        }
    }
    pub(crate) fn expose(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("secret", &"[REDACTED]")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone)]
pub struct CredentialRequest<'a> {
    pub host: &'a str,
    pub override_token: Option<&'a str>,
}

pub trait CredentialResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>;
}

pub trait CredentialSourceProvider: Send + Sync {
    fn load_blocking(&self, host: &str) -> Result<Option<SecretString>, ProviderError>;
}

pub struct ConfigCredentialResolver<S> {
    config: std::sync::Arc<ConfigOutput>,
    secure_storage: S,
}

#[derive(Clone, Debug, Default)]
pub struct PlatformCredentialStore;

#[derive(Clone, Debug)]
pub struct LegacyCredentialStore {
    home: PathBuf,
}
impl LegacyCredentialStore {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }
}
impl CredentialSourceProvider for LegacyCredentialStore {
    fn load_blocking(&self, host: &str) -> Result<Option<SecretString>, ProviderError> {
        let host = normalize_host(host);
        read_legacy(&self.home, &host)
    }
}

pub struct ChainedCredentialSource<A, B> {
    first: A,
    second: B,
}
impl<A, B> ChainedCredentialSource<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}
impl<A: CredentialSourceProvider, B: CredentialSourceProvider> CredentialSourceProvider
    for ChainedCredentialSource<A, B>
{
    fn load_blocking(&self, host: &str) -> Result<Option<SecretString>, ProviderError> {
        match self.first.load_blocking(host) {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => self.second.load_blocking(host),
            Err(primary) => match self.second.load_blocking(host)? {
                Some(value) => Ok(Some(value)),
                None => Err(primary),
            },
        }
    }
}

impl CredentialSourceProvider for PlatformCredentialStore {
    fn load_blocking(&self, host: &str) -> Result<Option<SecretString>, ProviderError> {
        load_platform_credential(host)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GhCliCredentialSource;

impl CredentialSourceProvider for GhCliCredentialSource {
    fn load_blocking(&self, host: &str) -> Result<Option<SecretString>, ProviderError> {
        Ok(gh_cli_token(host))
    }
}

fn gh_cli_token(host: &str) -> Option<SecretString> {
    let mut command = Command::new("gh");
    command
        .args(["auth", "token", "--hostname", host])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => {
                let _ = child.kill();
                return None;
            }
            Ok(None) if started.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let output = child.wait_with_output().ok()?;
    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| SecretString::from(token))
}

pub fn delete_platform_credential(host: &str) -> Result<(), ProviderError> {
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
        return Ok(());
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let store = match store {
            Ok(store) => store,
            Err(_) => return Ok(()),
        };
        let entry = match store.build("octocode", host, None) {
            Ok(entry) => entry,
            Err(_) => return Ok(()),
        };
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_store_error(error)),
        }
    }
}

fn load_platform_credential(host: &str) -> Result<Option<SecretString>, ProviderError> {
    Ok(load_platform_password(host)?.map(|secret| token_from_stored_blob(&secret)))
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
    use keyring_core::api::CredentialStoreApi;
    let host = normalize_host(host);
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
            super::ProviderErrorKind::CredentialStoreUnavailable,
            "no secure credential store is available on this platform",
        ));
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let store = store.map_err(|_| {
            ProviderError::new(
                super::ProviderErrorKind::CredentialStoreUnavailable,
                "secure credential store is unavailable",
            )
        })?;
        let entry = store
            .build("octocode", &host, None)
            .map_err(map_store_error)?;
        match entry.get_password() {
            Ok(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
            Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_store_error(error)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredentials {
    pub hostname: String,
    pub username: String,
    pub token: OAuthToken,
    pub git_protocol: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
            super::ProviderErrorKind::CredentialStoreUnavailable,
            "failed to encode stored credentials",
        )
    })?;
    set_platform_password(&credentials.hostname, &payload)
}

fn set_platform_password(host: &str, payload: &str) -> Result<(), ProviderError> {
    use keyring_core::api::CredentialStoreApi;
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (host, payload);
        return Err(ProviderError::new(
            super::ProviderErrorKind::CredentialStoreUnavailable,
            "no secure credential store is available on this platform",
        ));
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let store = store.map_err(|_| {
            ProviderError::new(
                super::ProviderErrorKind::CredentialStoreUnavailable,
                "secure credential store is unavailable",
            )
        })?;
        let entry = store
            .build("octocode", host, None)
            .map_err(map_store_error)?;
        entry.set_password(payload).map_err(map_store_error)
    }
}

fn map_store_error(error: keyring_core::Error) -> ProviderError {
    let kind = if matches!(error, keyring_core::Error::NoEntry) {
        super::ProviderErrorKind::NotFound
    } else {
        super::ProviderErrorKind::CredentialStoreUnavailable
    };
    ProviderError::new(kind, "secure credential store operation failed")
}

#[derive(Deserialize)]
struct LegacyStore {
    version: u64,
    credentials: std::collections::BTreeMap<String, LegacyCredential>,
}
impl Drop for LegacyStore {
    fn drop(&mut self) {
        for value in self.credentials.values_mut() {
            value.zeroize();
        }
        self.credentials.clear();
    }
}
#[derive(Deserialize)]
struct LegacyCredential {
    hostname: String,
    token: LegacyToken,
}
impl Zeroize for LegacyCredential {
    fn zeroize(&mut self) {
        self.hostname.zeroize();
        self.token.zeroize();
    }
}
#[derive(Deserialize)]
struct LegacyToken {
    token: String,
    #[serde(rename = "tokenType")]
    token_type: String,
}
impl Zeroize for LegacyToken {
    fn zeroize(&mut self) {
        self.token.zeroize();
        self.token_type.zeroize();
    }
}
impl Drop for LegacyToken {
    fn drop(&mut self) {
        self.zeroize();
    }
}
fn read_legacy(home: &Path, host: &str) -> Result<Option<SecretString>, ProviderError> {
    let key_path = home.join(".key");
    let credential_path = home.join("credentials.json");
    if !key_path.exists() || !credential_path.exists() {
        return Ok(None);
    }
    let key_text = read_capped(&key_path, 1024)?;
    let key = Zeroizing::new(hex::decode(key_text.trim()).map_err(|_| corrupt_legacy())?);
    if key.len() != 32 {
        return Err(corrupt_legacy());
    }
    let encrypted = read_capped(&credential_path, 1024 * 1024)?;
    let mut parts = encrypted.trim().split(':');
    let iv = Zeroizing::new(
        hex::decode(parts.next().ok_or_else(corrupt_legacy)?).map_err(|_| corrupt_legacy())?,
    );
    let tag = Zeroizing::new(
        hex::decode(parts.next().ok_or_else(corrupt_legacy)?).map_err(|_| corrupt_legacy())?,
    );
    let mut ciphertext = Zeroizing::new(
        hex::decode(parts.next().ok_or_else(corrupt_legacy)?).map_err(|_| corrupt_legacy())?,
    );
    if parts.next().is_some() || iv.len() != 16 || tag.len() != 16 {
        return Err(corrupt_legacy());
    }
    let cipher = AesGcm::<Aes256, U16>::new_from_slice(&key).map_err(|_| corrupt_legacy())?;
    let nonce: &[u8; 16] = iv.as_slice().try_into().map_err(|_| corrupt_legacy())?;
    let tag: &[u8; 16] = tag.as_slice().try_into().map_err(|_| corrupt_legacy())?;
    cipher
        .decrypt_inout_detached(
            nonce.into(),
            b"",
            ciphertext.as_mut_slice().into(),
            tag.into(),
        )
        .map_err(|_| corrupt_legacy())?;
    let mut store: LegacyStore =
        serde_json::from_slice(&ciphertext).map_err(|_| corrupt_legacy())?;
    if store.version != 1 {
        return Err(corrupt_legacy());
    }
    let Some(mut value) = store.credentials.remove(host) else {
        return Ok(None);
    };
    if normalize_host(&value.hostname) != host
        || value.token.token_type != "oauth"
        || value.token.token.trim().is_empty()
    {
        return Err(corrupt_legacy());
    }
    let token = Zeroizing::new(std::mem::take(&mut value.token.token));
    Ok(Some(SecretString::from(token.as_str())))
}
fn read_capped(path: &Path, max: u64) -> Result<Zeroizing<String>, ProviderError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| corrupt_legacy())?;
    if !file
        .metadata()
        .map_err(|_| corrupt_legacy())?
        .file_type()
        .is_file()
    {
        return Err(corrupt_legacy());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| corrupt_legacy())?;
    if bytes.len() as u64 > max {
        return Err(corrupt_legacy());
    }
    String::from_utf8(std::mem::take(&mut *bytes))
        .map(Zeroizing::new)
        .map_err(|_| corrupt_legacy())
}
fn corrupt_legacy() -> ProviderError {
    ProviderError::new(
        super::ProviderErrorKind::CredentialStoreUnavailable,
        "legacy credential store is invalid or unreadable",
    )
}
fn normalize_host(host: &str) -> String {
    let lower = host.trim().to_ascii_lowercase();
    lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower)
        .trim_end_matches('/')
        .to_owned()
}
impl<S> ConfigCredentialResolver<S> {
    pub fn new(config: std::sync::Arc<ConfigOutput>, secure_storage: S) -> Self {
        Self {
            config,
            secure_storage,
        }
    }
}
pub struct OwnedCredentialRequest {
    pub host: String,
    pub override_token: Option<SecretString>,
}
pub struct CredentialResolutionHandle {
    join: Option<std::thread::JoinHandle<Result<Option<ResolvedCredential>, ProviderError>>>,
}
impl CredentialResolutionHandle {
    pub fn finish(mut self) -> Result<Option<ResolvedCredential>, ProviderError> {
        self.join
            .take()
            .ok_or_else(|| {
                ProviderError::new(
                    super::ProviderErrorKind::CredentialStoreUnavailable,
                    "credential resolution handle already consumed",
                )
            })?
            .join()
            .map_err(|_| {
                ProviderError::new(
                    super::ProviderErrorKind::CredentialStoreUnavailable,
                    "credential resolution worker failed",
                )
            })?
    }
    #[cfg(test)]
    pub(crate) fn from_join(
        join: std::thread::JoinHandle<Result<Option<ResolvedCredential>, ProviderError>>,
    ) -> Self {
        Self { join: Some(join) }
    }
}
impl Drop for CredentialResolutionHandle {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
impl<S: CredentialSourceProvider + Send + Sync + 'static> ConfigCredentialResolver<S> {
    pub fn start_resolve(
        self: std::sync::Arc<Self>,
        request: OwnedCredentialRequest,
    ) -> CredentialResolutionHandle {
        CredentialResolutionHandle {
            join: Some(std::thread::spawn(move || {
                if let Some(token) = request
                    .override_token
                    .filter(|v| !v.expose_secret().trim().is_empty())
                {
                    return Ok(Some(ResolvedCredential::new(
                        token,
                        CredentialSource::Override,
                    )));
                }
                if let Some(token) = self.config.token.as_ref() {
                    return Ok(Some(ResolvedCredential::new(
                        token.token(),
                        CredentialSource::Environment,
                    )));
                }
                Ok(self
                    .secure_storage
                    .load_blocking(&request.host)?
                    .map(|token| ResolvedCredential::new(token, CredentialSource::Storage)))
            })),
        }
    }
}

#[derive(Clone, Default)]
pub struct StaticCredentialResolver {
    token: Option<ResolvedCredential>,
}

impl StaticCredentialResolver {
    pub fn anonymous() -> Self {
        Self::default()
    }
    pub fn new(token: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            token: Some(ResolvedCredential::new(token, source)),
        }
    }
    pub fn from_resolved(token: Option<ResolvedCredential>) -> Self {
        Self { token }
    }
}

impl CredentialResolver for StaticCredentialResolver {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            if let Some(token) = request.override_token {
                return Ok(Some(ResolvedCredential::new(
                    token,
                    CredentialSource::Override,
                )));
            }
            Ok(self.token.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::token_from_stored_blob;
    use secrecy::ExposeSecret;

    #[test]
    fn json_blob_yields_inner_token_and_raw_blob_is_unchanged() {
        let json = r#"{"hostname":"github.com","username":"octo","token":{"token":"gho_inner","tokenType":"oauth"},"gitProtocol":"https","createdAt":"t","updatedAt":"t"}"#;
        assert_eq!(token_from_stored_blob(json).expose_secret(), "gho_inner");
        assert_eq!(
            token_from_stored_blob("gho_raw_token").expose_secret(),
            "gho_raw_token"
        );
    }
}
