use super::super::super::login;
use super::super::StoredCredentials;
use crate::config::ConfigOutput;
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind};
use octocode_io::budget::RequestBudget;

pub(super) trait AuthBackend {
    async fn load(
        &self,
        host: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError>;
    async fn refresh(
        &self,
        host: &str,
        client_id: &str,
        source: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError>;
    async fn gh(
        &self,
        host: &str,
        budget: &RequestBudget,
    ) -> Result<Option<secrecy::SecretString>, ProviderError>;
}
pub(super) struct SystemBackend<'a> {
    pub(super) config: &'a ConfigOutput,
}
impl AuthBackend for SystemBackend<'_> {
    async fn load(
        &self,
        host: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError> {
        let host = host.to_owned();
        let store = super::super::CredentialStore::new(&self.config.home);
        tokio::task::spawn_blocking(move || store.load(&host))
            .await
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::CredentialStoreUnavailable,
                    "credential resolution worker failed",
                )
            })?
    }
    async fn refresh(
        &self,
        host: &str,
        client_id: &str,
        source: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError> {
        login::refresh_stored_in_store(
            host,
            client_id,
            &super::super::CredentialStore::new(&self.config.home),
            source,
        )
        .await
    }
    async fn gh(
        &self,
        host: &str,
        budget: &RequestBudget,
    ) -> Result<Option<secrecy::SecretString>, ProviderError> {
        super::super::discovery::gh_token(host, self.config.effective_env(), budget).await
    }
}
