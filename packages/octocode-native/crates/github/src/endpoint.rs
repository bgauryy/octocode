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
mod localhost_rule_tests {
    use super::{is_loopback, names_localhost, proxy_for};
    use octocode_io::http::Proxy;
    use url::Url;

    /// Every client built for a loopback host (each test's mock server)
    /// connects directly; a remote host keeps the user's system proxy.
    #[test]
    fn only_a_loopback_host_is_reached_without_the_system_proxy() {
        for (url, proxy) in [
            ("http://127.0.0.1:4873/pkg", Proxy::Direct),
            ("http://localhost:1/api/v3", Proxy::Direct),
            ("http://[::1]:8080/", Proxy::Direct),
            ("https://registry.npmjs.org/pkg", Proxy::System),
            ("https://github.com/login/oauth/access_token", Proxy::System),
            ("http://a.localhost/", Proxy::System),
        ] {
            assert_eq!(proxy_for(&Url::parse(url).expect("url")), proxy, "{url}");
        }
    }

    #[test]
    fn the_deny_rule_is_broader_than_the_credential_rule() {
        // Deny: every RFC 6761 `localhost` name, any case, with or without the root dot.
        for host in [
            "localhost",
            "LOCALHOST.",
            "a.localhost",
            "Verdaccio.LocalHost.",
        ] {
            assert!(names_localhost(host), "{host}");
        }
        for host in ["notlocalhost", "localhost.example.com", "example.com", ""] {
            assert!(!names_localhost(host), "{host}");
        }
        // Credentials over HTTP: only the exact loopback names and addresses.
        for url in [
            "http://localhost:4873/",
            "http://127.0.0.2/",
            "http://[::1]/",
        ] {
            assert!(is_loopback(&Url::parse(url).unwrap()), "{url}");
        }
        for url in [
            "http://a.localhost/",
            "http://localhost./",
            "http://10.0.0.1/",
        ] {
            assert!(!is_loopback(&Url::parse(url).unwrap()), "{url}");
        }
    }
}

#[cfg(test)]
mod deployment_tests {
    use super::{Deployment, GitHubEndpoint, api_base_for_host, credential_host};
    use url::Url;

    /// One deployment kind decides the GraphQL URL, the credential host
    /// and the API base of every host a user can name.
    #[test]
    fn each_host_derives_its_urls_and_credential_host_from_one_kind() {
        for (base, kind, graphql, credentials) in [
            (
                "https://api.github.com",
                Deployment::Cloud,
                "https://api.github.com/graphql",
                "github.com",
            ),
            (
                "https://api.acme.ghe.com",
                Deployment::Cloud,
                "https://api.acme.ghe.com/graphql",
                "acme.ghe.com",
            ),
            (
                "https://ghe.example.com/api/v3",
                Deployment::Server,
                "https://ghe.example.com/api/graphql",
                "ghe.example.com",
            ),
            // A loopback test server keeps the Server layout.
            (
                "http://127.0.0.1:8080/api/v3",
                Deployment::Server,
                "http://127.0.0.1:8080/api/graphql",
                "127.0.0.1",
            ),
        ] {
            let endpoint = GitHubEndpoint::new(Url::parse(base).expect("url")).expect("endpoint");
            assert_eq!(endpoint.deployment(), kind, "{base}");
            assert_eq!(endpoint.graphql().as_str(), graphql, "{base}");
            assert_eq!(endpoint.credential_host(), credentials, "{base}");
        }
    }

    /// Host strings (a configured host, a login host) follow the same rule.
    #[test]
    fn host_strings_use_the_same_rule() {
        assert_eq!(credential_host("api.acme.ghe.com"), "acme.ghe.com");
        assert_eq!(credential_host("acme.ghe.com"), "acme.ghe.com");
        assert_eq!(credential_host("api.github.com"), "github.com");
        assert_eq!(
            credential_host("api.ghe.example.com"),
            "api.ghe.example.com"
        );
        // Only one subdomain label is a GHE.com enterprise.
        assert_eq!(Deployment::of_host("api.a.b.ghe.com"), Deployment::Server);
        assert_eq!(Deployment::of_host("api.ghe.com"), Deployment::Server);
        assert_eq!(api_base_for_host("github.com"), "https://api.github.com");
        assert_eq!(
            api_base_for_host("acme.ghe.com"),
            "https://api.acme.ghe.com"
        );
        assert_eq!(
            api_base_for_host("api.acme.ghe.com"),
            "https://api.acme.ghe.com"
        );
        assert_eq!(
            api_base_for_host("ghe.example.com"),
            "https://ghe.example.com/api/v3"
        );
    }
}

#[cfg(test)]
mod resource_tests {
    use super::GitHubEndpoint;
    use crate::budget::GitHubResource;
    use url::Url;

    fn endpoint(base: &str) -> GitHubEndpoint {
        GitHubEndpoint::new(Url::parse(base).expect("base")).expect("endpoint")
    }

    fn resource(endpoint: &GitHubEndpoint, path: &str) -> GitHubResource {
        let mut url = endpoint.rest(&[]).expect("root");
        url.set_path(path);
        endpoint.resource(&url)
    }

    #[test]
    fn requests_are_classified_by_their_route_under_the_api_root() {
        use GitHubResource::{CodeSearch, Core, Graphql, Search};
        let dotcom = endpoint("https://api.github.com");
        let ghes = endpoint("https://ghe.example.com/api/v3");
        for (endpoint, root) in [(&dotcom, ""), (&ghes, "/api/v3")] {
            let cases = [
                ("/search/code", CodeSearch),
                ("/search/issues", Search),
                ("/search/repositories", Search),
                ("/search", Core),
                ("/search/", Core),
                ("/repos/a/b/issues", Core),
                // Owner, repository and ref names never pick the bucket.
                ("/repos/graphql/graphql-spec/contents/README.md", Core),
                ("/repos/apollographql/graphql-tools/commits/main", Core),
                ("/repos/search/code/contents/x", Core),
                ("/repos/owner/search/git/trees/HEAD", Core),
                ("/users/graphql", Core),
                ("/repos/a/b/commits/search/issues", Core),
            ];
            for (route, expected) in cases {
                assert_eq!(
                    resource(endpoint, &format!("{root}{route}")),
                    expected,
                    "{root}{route}"
                );
            }
            assert_eq!(endpoint.resource(&endpoint.graphql()), Graphql);
        }
        // GHES serves GraphQL beside, not under, the REST root.
        assert_eq!(resource(&ghes, "/api/graphql"), Graphql);
        assert_eq!(resource(&ghes, "/api/v3/graphql"), Core);
        assert_eq!(resource(&dotcom, "/graphql"), Graphql);
    }
}
