use url::Url;

use super::{ProviderError, ProviderErrorKind};

#[derive(Clone, Debug)]
pub struct GitHubEndpoint {
    rest_base: Url,
    graphql: Url,
}

impl GitHubEndpoint {
    pub fn github_com() -> Self {
        Self::new(Url::parse("https://api.github.com/").expect("static URL"))
            .expect("static endpoint")
    }

    pub fn new(mut rest_base: Url) -> Result<Self, ProviderError> {
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

    pub fn host(&self) -> &str {
        self.rest_base.host_str().unwrap_or_default()
    }
    pub fn credential_host(&self) -> &str {
        if self.host() == "api.github.com" {
            "github.com"
        } else {
            self.host()
        }
    }
    pub fn graphql(&self) -> Url {
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
                path.push(segment);
            }
        }
        Ok(url)
    }
    pub fn permits(&self, url: &Url) -> bool {
        self.rest_base.scheme() == url.scheme()
            && self.rest_base.host_str() == url.host_str()
            && self.rest_base.port_or_known_default() == url.port_or_known_default()
    }
}
