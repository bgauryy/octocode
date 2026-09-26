use super::*;
use crate::config::{ConfigInput, FileInput, RuntimeSurface, resolve_config};
use std::{collections::BTreeMap, sync::Mutex, time::Duration};

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
        trusted_project: false,
        global_env: missing(),
        project_env: missing(),
        config_file: missing(),
        project_config_file: missing(),
        runtime_surface: RuntimeSurface::Mcp,
        revision: 1,
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
        _: &RequestContext,
    ) -> Result<Option<secrecy::SecretString>, ProviderError> {
        self.calls.lock().unwrap().push("gh");
        Ok(self.gh.map(Into::into))
    }
}
fn budget() -> RequestContext {
    RequestContext::with_timeout(Duration::from_secs(2), 1)
}

#[tokio::test]
async fn absent_credentials_allow_anonymous_access_but_storage_errors_remain_visible() {
    let mut backend = Backend::new(None);
    backend.gh = None;
    let auth = authentication(None);
    assert!(
        auth.resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(backend.calls(), ["load", "gh"]);
    backend.calls.lock().unwrap().clear();
    backend.stored = Err(ProviderError::new(
        ProviderErrorKind::CredentialStoreUnavailable,
        "store unavailable",
    ));
    let error = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ProviderErrorKind::CredentialStoreUnavailable);
    assert_eq!(backend.calls(), ["load", "gh"]);
}

#[tokio::test]
async fn environment_is_lazy_host_scoped_and_override_wins() {
    let auth = authentication(Some("env-token"));
    let backend = Backend::new(None);
    let selected = auth
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "env-token");
    assert_eq!(selected.source_label(), "env");
    assert!(backend.calls().is_empty());
    let selected = auth
        .resolve_with("enterprise.example", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "gh-token");
    let mut budget = budget();
    budget.override_token = Some("override".into());
    let selected = auth
        .resolve_with("github.com", AuthMode::Request, &budget, &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "override");
}
#[tokio::test]
async fn fresh_storage_wins_and_inspection_does_not_refresh_expired_storage() {
    for (expired, mode) in [(false, AuthMode::Request), (true, AuthMode::Inspect)] {
        let backend = Backend::new(Some(stored(expired)));
        let selected = authentication(None)
            .resolve_with("github.com", mode, &budget(), &backend)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected.token(), "stored-token");
        assert_eq!(selected.username.as_deref(), Some("octo"));
        assert_eq!(backend.calls(), ["load"]);
    }
}
#[tokio::test]
async fn expired_storage_refreshes_and_failed_refresh_falls_back_with_gh_provenance() {
    let mut backend = Backend::new(Some(stored(true)));
    authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend.calls(), ["load", "refresh"]);
    backend.calls.lock().unwrap().clear();
    backend.refreshed = Err(ProviderError::new(
        ProviderErrorKind::Authentication,
        "refresh failed",
    ));
    let selected = authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.token(), "gh-token");
    assert_eq!(selected.source_label(), "gh-cli");
    assert_eq!(backend.calls(), ["load", "refresh", "gh"]);
}
#[tokio::test]
async fn gh_fallback_never_reenters_storage_or_refresh() {
    let mut backend = Backend::new(None);
    backend.stored = Err(ProviderError::new(
        ProviderErrorKind::CredentialStoreUnavailable,
        "headless",
    ));
    let selected = authentication(None)
        .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.source_label(), "gh-cli");
    assert_eq!(backend.calls(), ["load", "gh"]);
}
#[tokio::test]
async fn deletion_during_refresh_never_reuses_stale_token() {
    let mut backend = Backend::new(Some(stored(true)));
    backend.refreshed = Ok(None);
    backend.gh = None;
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn cancellation_and_refresh_timeout_do_not_fall_back() {
    let mut backend = Backend::new(Some(stored(true)));
    let cancelled = budget();
    cancelled.cancellation.cancel();
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &cancelled, &backend)
            .await
            .is_err()
    );
    assert!(backend.calls().is_empty());
    backend.refreshed = Err(ProviderError::new(ProviderErrorKind::Timeout, "deadline"));
    assert!(
        authentication(None)
            .resolve_with("github.com", AuthMode::Request, &budget(), &backend)
            .await
            .is_err()
    );
    assert_eq!(backend.calls(), ["load", "refresh"]);
}
