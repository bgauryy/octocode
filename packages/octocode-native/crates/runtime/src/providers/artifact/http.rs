use super::ArtifactError;
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;
use url::Url;

mod client;
mod system;

pub(crate) use client::RegistryClient;
pub use system::SystemArtifactHttp;

/// One runtime's registry answers and release facts. A disk directory
/// keeps them across CLI processes; without one they stay in memory.
pub struct ArtifactCache(Store<serde_json::Value>);

impl ArtifactCache {
    pub fn new(dir: Option<std::path::PathBuf>) -> Self {
        Self(Store::new(CacheConfig::default(), dir))
    }
}

/// A registry answer lives five minutes: a new release shows up soon.
const REGISTRY_ANSWER: CacheClass = CacheClass::Volatile {
    ttl: Duration::from_secs(300),
};

/// A read's key: the registry host scoped by the exact credential
/// ([`octocode_github::Partition`]), so one token's answers never serve
/// another caller.
fn cache_key(url: &Url, accept: &str, authorization: Option<&SecretString>) -> CacheKey {
    // One URL can answer in several representations (npm's abbreviated
    // packument); the default JSON keeps its historical key.
    let resource = if accept == JSON {
        url.as_str().to_owned()
    } else {
        format!("{accept} {url}")
    };
    CacheKey {
        namespace: "artifact".into(),
        scope: octocode_github::Partition::new(url.host_str().unwrap_or("registry"), authorization)
            .identity()
            .to_owned(),
        resource,
    }
}

/// Release facts are public: one unscoped namespace.
fn fact_key(resource: &str) -> CacheKey {
    CacheKey {
        namespace: "artifact-fact".into(),
        scope: String::new(),
        resource: resource.to_owned(),
    }
}

/// Page cursors of public registry searches: one unscoped namespace.
fn cursor_key(resource: &str) -> CacheKey {
    CacheKey {
        namespace: "artifact-cursor".into(),
        scope: String::new(),
        resource: resource.to_owned(),
    }
}

pub(crate) const JSON: &str = "application/json";
/// npm's abbreviated packument: versions and dist-tags without readmes.
pub(crate) const NPM_INSTALL_JSON: &str = "application/vnd.npm.install-v1+json";

pub type ArtifactHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ArtifactHttpResponse, ArtifactError>> + Send + 'a>>;

#[derive(Clone)]
pub struct ArtifactHttpRequest {
    pub url: Url,
    pub accept: &'static str,
    pub authorization: Option<SecretString>,
    pub(crate) dns_pin: Option<DnsPin>,
}

impl fmt::Debug for ArtifactHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactHttpRequest")
            .field("url", &self.url)
            .field("accept", &self.accept)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// A custom registry DNS result that was checked by the registry policy and
/// must be reused for the actual connection. Reusing these exact addresses
/// closes the validation-to-connect DNS rebinding window.
#[derive(Clone, Debug)]
pub(crate) struct DnsPin {
    pub host: String,
    pub addresses: Vec<SocketAddr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Takes one body chunk; `Break` ends the read before the body does.
pub type BodySink<'a> = &'a mut (dyn FnMut(&[u8]) -> ControlFlow<()> + Send);
pub type StatusFuture<'a> = Pin<Box<dyn Future<Output = Result<u16, ArtifactError>> + Send + 'a>>;

pub trait ArtifactHttp: Send + Sync {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a>;

    /// Feeds a 2xx body to `sink` as it arrives, until the sink breaks or
    /// the body ends, and returns the answer's status (another status feeds
    /// nothing). By default the whole body is read with [`Self::get`] and
    /// fed once.
    fn stream<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
        sink: BodySink<'a>,
    ) -> StatusFuture<'a> {
        Box::pin(async move {
            let response = self.get(request, budget).await?;
            if (200..300).contains(&response.status) {
                let _ = sink(&response.body);
            }
            Ok(response.status)
        })
    }
}

#[cfg(test)]
mod tests;
