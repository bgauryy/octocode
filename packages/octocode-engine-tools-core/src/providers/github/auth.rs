use std::{
    future::Future,
    pin::Pin,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::config::ConfigOutput;
use aes_gcm::{AesGcm, KeyInit, aead::AeadInPlace, aead::consts::U16, aes::Aes256};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
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

const KEYCHAIN_SERVICE: &str = "octocode";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredentials {
    pub hostname: String,
    pub username: String,
    pub token: OAuthToken,
    pub git_protocol: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Zeroize for StoredCredentials {
    fn zeroize(&mut self) {
        self.hostname.zeroize();
        self.username.zeroize();
        self.token.zeroize();
        self.git_protocol.zeroize();
        self.created_at.zeroize();
        self.updated_at.zeroize();
    }
}

impl Drop for StoredCredentials {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[derive(Clone, Serialize, Deserialize)]
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

impl Zeroize for OAuthToken {
    fn zeroize(&mut self) {
        self.token.zeroize();
        self.token_type.zeroize();
        if let Some(scopes) = self.scopes.as_mut() {
            for scope in scopes.iter_mut() {
                scope.zeroize();
            }
            scopes.clear();
        }
        if let Some(value) = self.refresh_token.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.expires_at.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.refresh_token_expires_at.as_mut() {
            value.zeroize();
        }
    }
}

impl Drop for OAuthToken {
    fn drop(&mut self) {
        self.zeroize();
    }
}

pub fn store_platform_credential(credentials: &StoredCredentials) -> Result<(), ProviderError> {
    store_in(&platform_store()?, credentials)
}

pub fn load_stored_credential(host: &str) -> Result<Option<StoredCredentials>, ProviderError> {
    load_stored_from(&platform_store()?, host)
}

pub fn delete_platform_credential(host: &str) -> Result<(), ProviderError> {
    let Ok(store) = platform_store() else {
        return Ok(());
    };
    delete_in(&store, host)
}

fn load_platform_credential(host: &str) -> Result<Option<SecretString>, ProviderError> {
    load_http_from(&platform_store()?, host)
}

fn platform_store() -> Result<Arc<keyring_core::CredentialStore>, ProviderError> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    return Err(store_unavailable(
        "no secure credential store is available on this platform",
    ));
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        store
            .map(|store| store as Arc<keyring_core::CredentialStore>)
            .map_err(|_| store_unavailable("secure credential store is unavailable"))
    }
}

fn store_in(
    store: &Arc<keyring_core::CredentialStore>,
    credentials: &StoredCredentials,
) -> Result<(), ProviderError> {
    let mut encoded = credentials.clone();
    encoded.hostname = normalize_host(&credentials.hostname);
    let blob = Zeroizing::new(serde_json::to_string(&encoded).map_err(|_| {
        ProviderError::new(
            super::ProviderErrorKind::Validation,
            "failed to encode stored credentials",
        )
    })?);
    let entry = store
        .build(KEYCHAIN_SERVICE, &encoded.hostname, None)
        .map_err(map_store_error)?;
    entry.set_password(&blob).map_err(map_store_error)
}

fn load_http_from(
    store: &Arc<keyring_core::CredentialStore>,
    host: &str,
) -> Result<Option<SecretString>, ProviderError> {
    Ok(read_entry_password(store, host)?.and_then(|secret| http_token_from_blob(&secret)))
}

fn load_stored_from(
    store: &Arc<keyring_core::CredentialStore>,
    host: &str,
) -> Result<Option<StoredCredentials>, ProviderError> {
    Ok(read_entry_password(store, host)?.and_then(|secret| stored_from_blob(host, &secret)))
}

fn delete_in(store: &Arc<keyring_core::CredentialStore>, host: &str) -> Result<(), ProviderError> {
    let host = normalize_host(host);
    let Ok(entry) = store.build(KEYCHAIN_SERVICE, &host, None) else {
        return Ok(());
    };
    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(map_store_error(error)),
    }
}

fn read_entry_password(
    store: &Arc<keyring_core::CredentialStore>,
    host: &str,
) -> Result<Option<Zeroizing<String>>, ProviderError> {
    let host = normalize_host(host);
    let entry = store
        .build(KEYCHAIN_SERVICE, &host, None)
        .map_err(map_store_error)?;
    match entry.get_password() {
        Ok(secret) => {
            let secret = Zeroizing::new(secret);
            if secret.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(secret))
            }
        }
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(map_store_error(error)),
    }
}

fn json_inner_token(value: &serde_json::Value) -> Option<&str> {
    value
        .pointer("/token/token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn http_token_from_blob(secret: &str) -> Option<SecretString> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => json_inner_token(&value).map(SecretString::from),
        // Pre-JSON keychain secrets are a raw token.
        Err(_) => Some(SecretString::from(trimmed)),
    }
}

fn stored_from_blob(host: &str, secret: &str) -> Option<StoredCredentials> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => {
            let inner = json_inner_token(&value)?;
            if let Ok(stored) = serde_json::from_str::<StoredCredentials>(trimmed)
                && !stored.token.token.trim().is_empty()
            {
                return Some(stored);
            }
            Some(synthesize_raw(host, inner))
        }
        Err(_) => Some(synthesize_raw(host, trimmed)),
    }
}

fn synthesize_raw(host: &str, token: &str) -> StoredCredentials {
    StoredCredentials {
        hostname: normalize_host(host),
        username: String::new(),
        token: OAuthToken {
            token: token.to_owned(),
            token_type: "oauth".to_owned(),
            scopes: None,
            refresh_token: None,
            expires_at: None,
            refresh_token_expires_at: None,
        },
        git_protocol: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn store_unavailable(message: &str) -> ProviderError {
    ProviderError::new(
        super::ProviderErrorKind::CredentialStoreUnavailable,
        message,
    )
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
    cipher
        .decrypt_in_place_detached(
            iv.as_slice().into(),
            b"",
            &mut ciphertext,
            tag.as_slice().into(),
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
    use super::*;
    use secrecy::ExposeSecret;

    fn mock_store() -> Arc<keyring_core::CredentialStore> {
        keyring_core::mock::Store::new()
            .map(|store| store as Arc<keyring_core::CredentialStore>)
            .expect("in-memory credential store")
    }

    fn sample_credentials(token: &str) -> StoredCredentials {
        StoredCredentials {
            hostname: "github.com".to_owned(),
            username: "alice".to_owned(),
            token: OAuthToken {
                token: token.to_owned(),
                token_type: "oauth".to_owned(),
                scopes: Some(vec![
                    "repo".to_owned(),
                    "read:org".to_owned(),
                    "gist".to_owned(),
                ]),
                refresh_token: Some("refresh_secret".to_owned()),
                expires_at: Some("2026-01-01T00:00:00Z".to_owned()),
                refresh_token_expires_at: Some("2026-02-01T00:00:00Z".to_owned()),
            },
            git_protocol: "https".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-02T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn stored_credentials_json_is_node_shaped() {
        let json = serde_json::to_value(sample_credentials("gho_inner")).expect("json");
        assert_eq!(json["hostname"], "github.com");
        assert_eq!(json["username"], "alice");
        assert_eq!(json["gitProtocol"], "https");
        assert_eq!(json["createdAt"], "2026-01-01T00:00:00Z");
        assert_eq!(json["updatedAt"], "2026-01-02T00:00:00Z");
        assert_eq!(json["token"]["token"], "gho_inner");
        assert_eq!(json["token"]["tokenType"], "oauth");
        assert_eq!(json["token"]["refreshToken"], "refresh_secret");
        assert_eq!(json["token"]["expiresAt"], "2026-01-01T00:00:00Z");
        assert_eq!(
            json["token"]["refreshTokenExpiresAt"],
            "2026-02-01T00:00:00Z"
        );
        assert!(json.get("git_protocol").is_none());
        assert!(json["token"].get("token_type").is_none());
    }

    #[test]
    fn store_json_http_credential_is_inner_token_not_blob() {
        let store = mock_store();
        store_in(&store, &sample_credentials("gho_inner")).expect("store");
        let blob = store
            .build(KEYCHAIN_SERVICE, "github.com", None)
            .expect("entry")
            .get_password()
            .expect("blob");
        assert!(blob.contains("gho_inner"), "blob should be the JSON object");
        assert!(blob.contains("hostname"), "blob should be the JSON object");

        let http = load_http_from(&store, "github.com")
            .expect("load")
            .expect("token");
        assert_eq!(http.expose_secret(), "gho_inner");
        assert!(
            !http.expose_secret().contains("hostname"),
            "HTTP credential must not be the JSON blob"
        );

        let resolved = ResolvedCredential::new(http, CredentialSource::Storage);
        assert_eq!(format!("Bearer {}", resolved.expose()), "Bearer gho_inner");

        let stored = load_stored_from(&store, "github.com")
            .expect("load stored")
            .expect("credentials");
        assert_eq!(stored.username, "alice");
        assert_eq!(stored.token.token, "gho_inner");
        assert_eq!(stored.token.token_type, "oauth");
        assert_eq!(stored.git_protocol, "https");
    }

    #[test]
    fn raw_blob_still_resolves_as_http_token() {
        let store = mock_store();
        store
            .build(KEYCHAIN_SERVICE, "github.com", None)
            .expect("entry")
            .set_password("gho_raw")
            .expect("set raw");

        let http = load_http_from(&store, "github.com")
            .expect("load")
            .expect("token");
        assert_eq!(http.expose_secret(), "gho_raw");
        assert_eq!(
            format!(
                "Bearer {}",
                ResolvedCredential::new(http, CredentialSource::Storage).expose()
            ),
            "Bearer gho_raw"
        );

        let stored = load_stored_from(&store, "github.com")
            .expect("load stored")
            .expect("credentials");
        assert_eq!(stored.token.token, "gho_raw");
        assert_eq!(stored.token.token_type, "oauth");
        assert_eq!(stored.username, "");
        assert_eq!(stored.created_at, "");
        assert_eq!(stored.updated_at, "");
    }

    #[test]
    fn empty_inner_token_is_not_a_raw_secret() {
        let store = mock_store();
        let blob = r#"{"hostname":"github.com","username":"alice","token":{"token":"","tokenType":"oauth"},"gitProtocol":"https","createdAt":"","updatedAt":""}"#;
        store
            .build(KEYCHAIN_SERVICE, "github.com", None)
            .expect("entry")
            .set_password(blob)
            .expect("set");
        assert!(
            load_http_from(&store, "github.com")
                .expect("load")
                .is_none()
        );
        assert!(
            load_stored_from(&store, "github.com")
                .expect("load stored")
                .is_none()
        );
    }

    #[test]
    fn partial_json_uses_inner_token_for_http_and_structured() {
        let store = mock_store();
        store
            .build(KEYCHAIN_SERVICE, "github.com", None)
            .expect("entry")
            .set_password(r#"{"token":{"token":"gho_partial"}}"#)
            .expect("set");
        let http = load_http_from(&store, "github.com")
            .expect("load")
            .expect("token");
        assert_eq!(http.expose_secret(), "gho_partial");
        let stored = load_stored_from(&store, "github.com")
            .expect("load stored")
            .expect("credentials");
        assert_eq!(stored.token.token, "gho_partial");
        assert_eq!(stored.token.token_type, "oauth");
        assert_eq!(stored.username, "");
    }

    #[test]
    fn store_and_load_normalize_hostname_like_node() {
        let store = mock_store();
        let mut credentials = sample_credentials("gho_host");
        credentials.hostname = "https://GitHub.com/".to_owned();
        store_in(&store, &credentials).expect("store");

        let blob = store
            .build(KEYCHAIN_SERVICE, "github.com", None)
            .expect("entry")
            .get_password()
            .expect("blob");
        let json: serde_json::Value = serde_json::from_str(&blob).expect("json");
        assert_eq!(json["hostname"], "github.com");

        for host in ["https://GitHub.com/", "github.com", "HTTPS://github.com"] {
            let http = load_http_from(&store, host).expect("load").expect("token");
            assert_eq!(http.expose_secret(), "gho_host");
            let stored = load_stored_from(&store, host)
                .expect("load stored")
                .expect("credentials");
            assert_eq!(stored.hostname, "github.com");
            assert_eq!(stored.token.token, "gho_host");
        }

        delete_in(&store, "https://GitHub.com/").expect("delete");
        assert!(
            load_http_from(&store, "github.com")
                .expect("load after delete")
                .is_none()
        );
    }

    #[test]
    fn trimmed_json_blob_yields_inner_token() {
        let json = serde_json::to_string(&sample_credentials("gho_pad")).expect("json");
        let padded = format!("\n{json}\n");
        assert_eq!(
            http_token_from_blob(&padded)
                .expect("token")
                .expose_secret(),
            "gho_pad"
        );
    }
}
