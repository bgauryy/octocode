//! Home credentials are primary; the platform store preserves existing native logins.
use super::super::{ProviderError, ProviderErrorKind};
use super::{CredentialSource, StoredCredentials, home_store::HomeStore, normalize_host, storage};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct CredentialStore {
    home: PathBuf,
}
impl CredentialStore {
    pub fn new(home: impl AsRef<Path>) -> Self {
        Self {
            home: home.as_ref().into(),
        }
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn load(
        &self,
        host: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError> {
        self.load_with(host, &storage::load_stored_credentials)
    }
    fn load_with(
        &self,
        host: &str,
        platform: &dyn Fn(&str) -> Result<Option<StoredCredentials>, ProviderError>,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError> {
        let host = normalize_host(host);
        let home = HomeStore::new(&self.home).load(&host);
        match home {
            Ok(Some(credentials)) => Ok(Some((credentials, CredentialSource::Home))),
            Ok(None) => {
                platform(&host).map(|value| value.map(|value| (value, CredentialSource::Storage)))
            }
            Err(error) => match platform(&host) {
                Ok(Some(credentials)) => Ok(Some((credentials, CredentialSource::Storage))),
                _ => Err(error),
            },
        }
    }
    pub fn save(&self, credentials: &StoredCredentials) -> Result<(), ProviderError> {
        HomeStore::new(&self.home).save(credentials)
    }
    pub fn delete(&self, host: &str) -> Result<(), ProviderError> {
        self.delete_with(host, &storage::delete_platform_credential)
    }
    fn delete_with(
        &self,
        host: &str,
        platform: &dyn Fn(&str) -> Result<(), ProviderError>,
    ) -> Result<(), ProviderError> {
        let host = normalize_host(host);
        let home = HomeStore::new(&self.home).delete(&host);
        let platform = platform(&host);
        home.and(platform)
    }
    pub(crate) fn load_from(
        &self,
        host: &str,
        source: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError> {
        match source {
            CredentialSource::Home => HomeStore::new(&self.home).load(host),
            CredentialSource::Storage => storage::load_stored_credentials(host),
            _ => Err(unavailable()),
        }
    }
    pub(crate) fn save_to(
        &self,
        previous: &StoredCredentials,
        credentials: &StoredCredentials,
        source: CredentialSource,
    ) -> Result<(), ProviderError> {
        match source {
            CredentialSource::Home => HomeStore::new(&self.home).replace(previous, credentials),
            CredentialSource::Storage => storage::store_platform_credential(credentials),
            _ => Err(unavailable()),
        }
    }
}
fn unavailable() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::CredentialStoreUnavailable,
        "credential store context unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn credential() -> StoredCredentials {
        serde_json::from_value(serde_json::json!({"hostname":"example.test","username":"fixture","token":{"token":"synthetic-token","tokenType":"oauth"},"gitProtocol":"https","createdAt":"","updatedAt":""})).unwrap()
    }
    #[test]
    fn home_wins_and_platform_fallback_remains_available() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());
        let fallback = |_: &str| Ok(Some(credential()));
        assert_eq!(
            store
                .load_with("example.test", &fallback)
                .unwrap()
                .unwrap()
                .1,
            CredentialSource::Storage
        );
        store.save(&credential()).unwrap();
        let no_platform = |_: &str| -> Result<Option<StoredCredentials>, ProviderError> {
            panic!("home must win")
        };
        assert_eq!(
            store
                .load_with("EXAMPLE.TEST", &no_platform)
                .unwrap()
                .unwrap()
                .1,
            CredentialSource::Home
        );
        std::fs::write(dir.path().join("credentials.json"), "corrupt").unwrap();
        assert_eq!(
            store
                .load_with("example.test", &fallback)
                .unwrap()
                .unwrap()
                .1,
            CredentialSource::Storage
        );
        assert!(store.load_with("example.test", &|_| Ok(None)).is_err());
    }
    #[test]
    fn logout_clears_both_sources_and_reports_partial_failure() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());
        store.save(&credential()).unwrap();
        let called = std::cell::Cell::new(false);
        let platform = |host: &str| {
            assert_eq!(host, "example.test");
            called.set(true);
            Err(unavailable())
        };
        assert!(store.delete_with("EXAMPLE.TEST", &platform).is_err());
        assert!(called.get());
        assert!(!dir.path().join("credentials.json").exists());
        assert!(store.delete_with("example.test", &|_| Ok(())).is_ok());
    }
}
