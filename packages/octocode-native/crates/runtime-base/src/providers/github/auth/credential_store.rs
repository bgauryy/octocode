//! Home credentials are primary; the platform store preserves existing native logins.
use super::{StoredCredentials, home_store::HomeStore, normalize_host, storage};
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind};
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
    pub(crate) fn home(&self) -> &Path {
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
mod tests;
