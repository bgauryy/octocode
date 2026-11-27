use url::Url;

use super::{ProviderError, ProviderErrorKind, budget::GitHubResource};

/// Where a GitHub API runs, which decides its URL layout and limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deployment {
    /// GitHub's cloud: github.com (`api.github.com`) and GHE.com
    /// (`api.<subdomain>.ghe.com`), which mirrors api.github.com
    /// (<https://docs.github.com/en/enterprise-cloud@latest/rest/meta/meta>):
    /// REST at the host root, GraphQL at `<base>/graphql`, credentials
    /// stored for the host without `api.`.
    Cloud,
    /// GitHub Enterprise Server (any other host, loopback test servers
    /// included): REST under `/api/v3`, GraphQL at `/api/graphql`.
    Server,
}

impl Deployment {
    /// The deployment of an API host, or of the web host its credentials
    /// are stored for (`github.com`, `<subdomain>.ghe.com`).
    #[must_use]
    pub fn of_host(host: &str) -> Self {
        let web = strip_api(host).unwrap_or(host);
        if is_cloud_web_host(web) {
            Self::Cloud
        } else {
            Self::Server
        }
    }
}

/// `host` without a leading `api.` label, case-insensitively.
fn strip_api(host: &str) -> Option<&str> {
    host.split_at_checked(4)
        .filter(|(label, _)| label.eq_ignore_ascii_case("api."))
        .map(|(_, web)| web)
}

/// Whether `host` is a cloud web host: github.com, or one GHE.com
/// subdomain.
fn is_cloud_web_host(host: &str) -> bool {
    const GHE_COM: &str = ".ghe.com";
    let enterprise = host
        .len()
        .checked_sub(GHE_COM.len())
        .filter(|&at| at > 0 && host.is_char_boundary(at))
        .map(|at| host.split_at(at))
        .filter(|(_, suffix)| suffix.eq_ignore_ascii_case(GHE_COM));
    host.eq_ignore_ascii_case("github.com")
        || enterprise.is_some_and(|(subdomain, _)| !subdomain.contains('.'))
}

/// The host stored credentials and limits belong to for an API host (or
/// that web host itself): a cloud API host drops its `api.` label
/// (`api.github.com` -> `github.com`, `api.acme.ghe.com` -> `acme.ghe.com`,
/// as gh stores them); a Server host is itself.
pub fn credential_host(api_host: &str) -> &str {
    match (Deployment::of_host(api_host), strip_api(api_host)) {
        (Deployment::Cloud, Some(web)) => web,
        _ => api_host,
    }
}

/// The REST API base URL for a host a user names (a web or API host): a
/// cloud host's API is `https://api.<web host>`, a Server's is
/// `https://<host>/api/v3`.
#[must_use]
pub fn api_base_for_host(host: &str) -> String {
    match Deployment::of_host(host) {
        Deployment::Cloud => format!("https://api.{}", credential_host(host)),
        Deployment::Server => format!("https://{host}/api/v3"),
    }
}

#[derive(Clone, Debug)]
pub struct GitHubEndpoint {
    rest_base: Url,
    graphql: Url,
    deployment: Deployment,
}

impl GitHubEndpoint {
    pub fn new(mut rest_base: Url) -> Result<Self, ProviderError> {
        // The Bearer token rides every request: plain http only to loopback.
        if rest_base.scheme() == "http" && !is_loopback(&rest_base) {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "GitHub API base URL must use https (plain http is allowed only for loopback)",
            ));
        }
        if !matches!(rest_base.scheme(), "http" | "https")
            || rest_base.host_str().is_none()
            || !rest_base.username().is_empty()
            || rest_base.password().is_some()
            || rest_base.query().is_some()
            || rest_base.fragment().is_some()
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "invalid GitHub API base URL",
            ));
        }
        if !rest_base.path().ends_with('/') {
            rest_base.set_path(&format!("{}/", rest_base.path()));
        }
        let deployment = Deployment::of_host(rest_base.host_str().unwrap_or_default());
        let graphql = match deployment {
            Deployment::Cloud => rest_base.join("graphql"),
            Deployment::Server => {
                let mut value = rest_base.clone();
                value.set_path("/api/graphql");
                Ok(value)
            }
        }
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Configuration,
                "invalid GitHub GraphQL URL",
            )
        })?;
        Ok(Self {
            rest_base,
            graphql,
            deployment,
        })
    }

    /// Where this API runs.
    #[must_use]
    pub fn deployment(&self) -> Deployment {
        self.deployment
    }

    /// Whether the API is on a loopback host.
    pub fn is_loopback(&self) -> bool {
        is_loopback(&self.rest_base)
    }

    /// The proxy settings a client for this API honors ([`proxy_for`]).
    #[must_use]
    pub fn proxy(&self) -> octocode_io::http::Proxy {
        proxy_for(&self.rest_base)
    }
    pub(crate) fn host(&self) -> &str {
        self.rest_base.host_str().unwrap_or_default()
    }
    pub fn credential_host(&self) -> &str {
        credential_host(self.host())
    }
    pub(crate) fn graphql(&self) -> Url {
        self.graphql.clone()
    }
    pub fn rest(&self, segments: &[&str]) -> Result<Url, ProviderError> {
        let mut url = self.rest_base.clone();
        {
            let mut path = url.path_segments_mut().map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Configuration,
                    "GitHub API base cannot contain path segments",
                )
            })?;
            path.pop_if_empty();
            for segment in segments {
                // `url` resolves a pushed `.`/`..` segment, which would
                // silently re-route an owner/repo to another API path.
                if matches!(*segment, "." | "..") {
                    return Err(ProviderError::new(
                        ProviderErrorKind::Validation,
                        "GitHub route segments cannot be '.' or '..'",
                    ));
                }
                path.push(segment);
            }
        }
        Ok(url)
    }
    /// The rate-limit resource a request draws from, by its route: the
    /// GraphQL URL's path exactly, else the first segments under the REST
    /// root (`search/code`, `search/<kind>`). An owner, repository or ref
    /// named `graphql` or `search` stays core.
    pub(crate) fn resource(&self, url: &Url) -> GitHubResource {
        if url.path() == self.graphql.path() {
            return GitHubResource::Graphql;
        }
        let Some(route) = url.path().strip_prefix(self.rest_base.path()) else {
            return GitHubResource::Core;
        };
        let mut segments = route.split('/');
        match (segments.next(), segments.next()) {
            (Some("search"), Some("code")) => GitHubResource::CodeSearch,
            (Some("search"), Some(kind)) if !kind.is_empty() => GitHubResource::Search,
            _ => GitHubResource::Core,
        }
    }
    pub(crate) fn permits(&self, url: &Url) -> bool {
        self.rest_base.scheme() == url.scheme()
            && self.rest_base.host_str() == url.host_str()
            && self.rest_base.port_or_known_default() == url.port_or_known_default()
    }
}

/// Whether `url` names a loopback host (localhost, 127.0.0.0/8, ::1).
pub fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// The proxy settings a client for `url` honors: a loopback host is reached
/// directly (no system proxy lookup, which on macOS is a configuration-daemon
/// session costing seconds per process, and no proxy that could see a
/// credential meant for this machine); any other host goes through the
/// user's system proxy.
#[must_use]
pub fn proxy_for(url: &Url) -> octocode_io::http::Proxy {
    if is_loopback(url) {
        octocode_io::http::Proxy::Direct
    } else {
        octocode_io::http::Proxy::System
    }
}

/// A hostname that must be denied when local registry access is forbidden.
/// This broad deny rule does not authorize sending credentials over HTTP.
pub fn names_localhost(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost")
}

#[cfg(test)]
mod localhost_rule_tests;

#[cfg(test)]
mod deployment_tests;

#[cfg(test)]
mod resource_tests;
