mod http;
mod maven;
mod npm;
mod npmrc;
mod nuget;
mod registries;
mod types;
mod util;

pub use http::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, SystemArtifactHttp,
};
#[cfg(test)]
pub(crate) use types::artifact_query;
pub use types::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactSearchQueryType, ResolvedNpmRegistry,
};

pub(crate) use npmrc::{authorization_from_file as npm_authorization, user_npmrc_path};

use crate::providers::RequestBudget;
use http::RegistryClient;
use url::Url;

pub struct ArtifactProviderContext<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub npm_registry: Option<&'a ResolvedNpmRegistry>,
    /// Opt-in escape hatch for the npm registry SSRF guard (see NetworkConfig).
    pub allow_private_registry: bool,
    /// Config revision forwarded to the in-process registry cache.  A revision
    /// bump (e.g. after `storage.mode` or token changes) causes stale cache
    /// entries to be evicted on the next access.
    pub cache_revision: u64,
    /// When `false` the in-process registry HTTP cache is bypassed entirely
    /// (both reads and writes).  Set to `false` when `storage.mode=="memory"`.
    pub cache_enabled: bool,
}

pub async fn execute_artifact(
    query: &ArtifactSearchQuery,
    context: &ArtifactProviderContext<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let client = RegistryClient {
        http: context.http,
        budget: context.budget,
        cache_revision: context.cache_revision,
        cache_enabled: context.cache_enabled,
    };
    // Cursors are opaque continuation state; a cursor that does not parse is
    // caller-constructed or stale and must fail loudly instead of silently
    // serving page 1 again.
    let state: ArtifactProviderState = match query.cursor() {
        Some(cursor) => serde_json::from_str(cursor).map_err(|_| {
            ArtifactError::new(
                "invalid_query",
                "Unrecognized cursor. Copy the complete next.nextPage query unchanged, or omit the cursor to restart.",
            )
        })?,
        None => ArtifactProviderState::default(),
    };
    validate_cursor_state(&state)?;
    match query.type_ {
        ArtifactSearchQueryType::Npm => {
            let default_registry = ResolvedNpmRegistry {
                base: Url::parse("https://registry.npmjs.org/").map_err(|_| {
                    ArtifactError::new("invalid_query", "Invalid default npm registry URL.")
                })?,
                authorization: None,
                cache_identity: "npmjs".into(),
            };
            let registry = context.npm_registry.unwrap_or(&default_registry);
            npm::npm(
                query,
                &state,
                registry,
                &client,
                context.allow_private_registry,
            )
            .await
        }
        ArtifactSearchQueryType::Pypi => registries::pypi(query, &client).await,
        ArtifactSearchQueryType::Crates => registries::crates(query, &state, &client).await,
        ArtifactSearchQueryType::Go => registries::go(query, &state, &client).await,
        ArtifactSearchQueryType::Packagist => registries::packagist(query, &state, &client).await,
        ArtifactSearchQueryType::Rubygems => registries::rubygems(query, &state, &client).await,
        ArtifactSearchQueryType::Maven => maven::maven(query, &state, &client).await,
        ArtifactSearchQueryType::Nuget => nuget::nuget(query, &state, &client).await,
    }
}

/// Registries cap deep paging well below these bounds; state beyond them is
/// stale or caller-constructed. Failing loudly beats serving a wrong page
/// labeled as final data.
const MAX_CURSOR_OFFSET: u64 = 10_000;
const MAX_CURSOR_PAGE: u64 = 1_000;
const MAX_CURSOR_TOKEN_BYTES: usize = 4_096;

fn validate_cursor_state(state: &ArtifactProviderState) -> Result<(), ArtifactError> {
    let out_of_range = state.offset.unwrap_or(0) > MAX_CURSOR_OFFSET
        || state.page.unwrap_or(1) > MAX_CURSOR_PAGE
        || state.page == Some(0)
        || state
            .token
            .as_ref()
            .is_some_and(|token| token.len() > MAX_CURSOR_TOKEN_BYTES);
    if out_of_range {
        return Err(ArtifactError::new(
            "invalid_query",
            "Cursor is outside the supported paging range. Copy the complete next.nextPage query unchanged, or omit the cursor to restart.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod cursor_state_tests {
    use super::*;

    #[test]
    fn constructed_or_stale_cursor_state_is_rejected() {
        for state in [
            ArtifactProviderState {
                offset: Some(MAX_CURSOR_OFFSET + 1),
                ..Default::default()
            },
            ArtifactProviderState {
                page: Some(MAX_CURSOR_PAGE + 1),
                ..Default::default()
            },
            ArtifactProviderState {
                page: Some(0),
                ..Default::default()
            },
            ArtifactProviderState {
                token: Some("x".repeat(MAX_CURSOR_TOKEN_BYTES + 1)),
                ..Default::default()
            },
        ] {
            let error = validate_cursor_state(&state).expect_err("out of range");
            assert_eq!(error.code, "invalid_query");
        }
        for state in [
            ArtifactProviderState::default(),
            ArtifactProviderState {
                offset: Some(30),
                page: Some(2),
                token: Some("issued".into()),
            },
        ] {
            assert!(validate_cursor_state(&state).is_ok());
        }
    }
}
