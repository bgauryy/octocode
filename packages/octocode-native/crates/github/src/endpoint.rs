use url::Url;

use super::{ProviderError, ProviderErrorKind, budget::GitHubResource};

/// The host stored credentials and limits belong to for an API host:
/// github.com's API host `api.github.com` folds to `github.com`; any other
/// host is itself.
pub fn credential_host(api_host: &str) -> &str {
    if api_host.eq_ignore_ascii_case("api.github.com") {
        "github.com"
    } else {
        api_host
    }
}

#[derive(Clone, Debug)]
pub struct GitHubEndpoint {
    rest_base: Url,
    graphql: Url,
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
        let graphql = if rest_base.host_str() == Some("api.github.com") {
            rest_base.join("graphql")
        } else {
            let mut value = rest_base.clone();
            value.set_path("/api/graphql");
            Ok(value)
        }
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Configuration,
                "invalid GitHub GraphQL URL",
            )
        })?;
        Ok(Self { rest_base, graphql })
    }

    /// Whether the API is on a loopback host.
    pub fn is_loopback(&self) -> bool {
        is_loopback(&self.rest_base)
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

/// A hostname that must be denied when local registry access is forbidden.
/// This broad deny rule does not authorize sending credentials over HTTP.
pub fn names_localhost(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost")
}

#[cfg(test)]
mod localhost_rule_tests {
    use super::{is_loopback, names_localhost};
    use url::Url;

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
