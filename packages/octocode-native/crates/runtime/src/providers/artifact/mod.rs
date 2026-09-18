mod http;
mod maven;
mod npm;
mod nuget;
mod registries;
mod types;
mod util;

pub use http::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, SystemArtifactHttp,
};
pub use types::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType, ResolvedNpmRegistry,
};

use crate::providers::RequestBudget;
use http::RegistryClient;
use url::Url;

pub struct ArtifactProviderContext<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub npm_registry: Option<&'a ResolvedNpmRegistry>,
}

pub async fn execute_artifact(
    query: &ArtifactQuery,
    context: &ArtifactProviderContext<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let client = RegistryClient {
        http: context.http,
        budget: context.budget,
    };
    let state = query
        .cursor
        .as_deref()
        .and_then(|cursor| serde_json::from_str(cursor).ok())
        .unwrap_or_default();
    match query.artifact_type {
        ArtifactType::Npm => {
            let default_registry = ResolvedNpmRegistry {
                base: Url::parse("https://registry.npmjs.org/").map_err(|_| {
                    ArtifactError::new("invalid_query", "Invalid default npm registry URL.")
                })?,
                authorization: None,
                cache_identity: "npmjs".into(),
            };
            let registry = context.npm_registry.unwrap_or(&default_registry);
            npm::npm(query, &state, registry, &client).await
        }
        ArtifactType::PyPi => registries::pypi(query, &client).await,
        ArtifactType::Crates => registries::crates(query, &state, &client).await,
        ArtifactType::Go => registries::go(query, &state, &client).await,
        ArtifactType::Packagist => registries::packagist(query, &state, &client).await,
        ArtifactType::Rubygems => registries::rubygems(query, &state, &client).await,
        ArtifactType::Maven => maven::maven(query, &state, &client).await,
        ArtifactType::Nuget => nuget::nuget(query, &state, &client).await,
    }
}
