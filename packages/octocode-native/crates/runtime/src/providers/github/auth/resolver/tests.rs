use super::*;
use crate::config::{ConfigInput, FileInput, RuntimeSurface, resolve_config};
use std::{collections::BTreeMap, sync::Mutex, time::Duration};
mod resolution;
mod storage_refresh;

fn authentication(token: Option<&str>) -> Authentication {
    let env = token
        .map(|token| BTreeMap::from([("GH_TOKEN".into(), token.into())]))
        .unwrap_or_default();
    let missing = || FileInput::Missing {
        path: "/synthetic/missing".into(),
    };
    let config = resolve_config(&ConfigInput {
        env,
        cwd: "/synthetic".into(),
        os_home: "/synthetic".into(),
        global_env: missing(),
        project_env: missing(),
        config_file: missing(),
        project_config_file: missing(),
        runtime_surface: RuntimeSurface::Mcp,
    });
    Authentication::new(Arc::new(config))
}
fn stored(expired: bool) -> StoredCredentials {
    StoredCredentials {
        hostname: "github.com".into(),
        username: "octo".into(),
        token: super::super::OAuthToken {
            token: "stored-token".into(),
            token_type: "oauth".into(),
            scopes: None,
            refresh_token: Some("refresh".into()),
            expires_at: expired.then(|| "2000-01-01T00:00:00Z".into()),
            refresh_token_expires_at: None,
        },
        git_protocol: "https".into(),
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}
struct Backend {
    calls: Mutex<Vec<&'static str>>,
    stored: Result<Option<StoredCredentials>, ProviderError>,
    refreshed: Result<Option<StoredCredentials>, ProviderError>,
    gh: Option<&'static str>,
}
impl Backend {
    fn new(initial: Option<StoredCredentials>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            stored: Ok(initial),
            refreshed: Ok(Some(stored(false))),
            gh: Some("gh-token"),
        }
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}
impl AuthBackend for Backend {
    async fn load(
        &self,
        _: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError> {
        self.calls.lock().unwrap().push("load");
        // A real store read is a blocking worker: other rows run meanwhile.
        tokio::task::yield_now().await;
        self.stored
            .clone()
            .map(|stored| stored.map(|stored| (stored, CredentialSource::Storage)))
    }
    async fn refresh(
        &self,
        _: &str,
        _: &str,
        _: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError> {
        self.calls.lock().unwrap().push("refresh");
        self.refreshed.clone()
    }
    async fn gh(
        &self,
        _: &str,
        _: &RequestBudget,
    ) -> Result<Option<secrecy::SecretString>, ProviderError> {
        self.calls.lock().unwrap().push("gh");
        Ok(self.gh.map(Into::into))
    }
}
fn budget() -> RequestBudget {
    RequestBudget::with_timeout(Duration::from_secs(30), 1)
}
