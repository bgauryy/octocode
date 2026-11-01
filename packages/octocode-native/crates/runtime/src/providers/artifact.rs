mod crates;
mod go;
mod http;
mod maven;
mod npm;
mod npmrc;
mod nuget;
mod packagist;
mod pypi;
pub(crate) mod query;
mod release_ref;
mod rubygems;
#[cfg(test)]
pub(crate) mod test_http;
pub(crate) mod types;
mod util;
mod versions;

use crate::providers::artifact::query::artifact_search_query;
use crate::providers::artifact::query::artifact_type;
pub use http::{ArtifactCache, ArtifactHttp, SystemArtifactHttp};
pub(crate) use npm::registry::invalid_registry_url;
pub(crate) use npmrc::npm_authorization;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
pub use release_ref::{ReleaseTags, TagFuture};
pub use types::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ResolvedNpmRegistry,
    TerminalLimit,
};
pub(crate) use util::{github_repo_dir, github_repository, github_slug, release_ref_in};

use http::RegistryClient;
use octocode_io::budget::RequestBudget;
use versions::is_range;

pub struct ArtifactProviderContext<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub npm_registry: Option<&'a ResolvedNpmRegistry>,
    /// Opt-in escape hatch for the npm registry SSRF guard (see NetworkConfig).
    pub allow_private_registry: bool,
    /// The runtime's registry cache; `None` (`storage.mode == "memory"`)
    /// bypasses it for both reads and writes.
    pub cache: Option<&'a ArtifactCache>,
    /// Upstream release-tag checks through the GitHub API; `None` reads no tags.
    pub tags: Option<&'a dyn ReleaseTags>,
}

pub async fn execute_artifact(
    query: &ArtifactSearchQuery,
    context: &ArtifactProviderContext<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let client = RegistryClient {
        http: context.http,
        budget: context.budget,
        cache: context.cache,
        tags: context.tags,
    };
    let state = page_state(query);
    validate_cursor_state(
        artifact_search_query::artifact_type(query),
        &state,
        artifact_search_query::page_size(query),
    )?;
    if artifact_search_query::version(query).is_some()
        && version_support(artifact_type::as_str(artifact_search_query::artifact_type(
            query,
        ))) == "none"
    {
        return Err(ArtifactError::new(
            ErrorCode::CapabilityUnavailable,
            format!(
                "{}; {} lookups return the latest release.",
                version_support_summary(),
                artifact_type::as_str(artifact_search_query::artifact_type(query))
            ),
        ));
    }
    // An ecosystem that takes exact versions has no range to resolve: a
    // range is refused as such, never looked up as a version.
    if let Some(version) = artifact_search_query::version(query)
        && is_range(version)
    {
        let ecosystem = artifact_type::as_str(artifact_search_query::artifact_type(query));
        if version_support(ecosystem) == "exact" {
            return Err(ArtifactError::new(
                ErrorCode::CapabilityUnavailable,
                format!(
                    "{ecosystem} takes exact versions only, not the range \"{version}\"; omit version for the latest release."
                ),
            ));
        }
    }
    match artifact_search_query::artifact_type(query) {
        ArtifactType::Npm => {
            let default_registry;
            let registry = match context.npm_registry {
                Some(registry) => registry,
                None => {
                    default_registry = ResolvedNpmRegistry::npmjs();
                    &default_registry
                }
            };
            npm::npm(
                query,
                &state,
                registry,
                &client,
                context.allow_private_registry,
            )
            .await
        }
        ArtifactType::Pypi => pypi::pypi(query, &client).await,
        ArtifactType::Crates => crates::crates(query, &state, &client).await,
        ArtifactType::Go => go::go(query, &state, &client).await,
        ArtifactType::Packagist => packagist::packagist(query, &state, &client).await,
        ArtifactType::Rubygems => rubygems::rubygems(query, &state, &client).await,
        ArtifactType::Maven => maven::maven(query, &state, &client).await,
        ArtifactType::Nuget => nuget::nuget(query, &state, &client).await,
    }
}

/// The provider position of the saved private `page`: an item offset for
/// offset-addressed registries, the page itself for page-addressed ones.
/// Every page starts at `(page - 1) * policy page size`, so consecutive pages tile
/// the result list without gaps.
/// A token-paged registry's saved state also carries `registryCursor`, the
/// registry token that starts it (the contract bounds its length).
fn page_state(query: &ArtifactSearchQuery) -> ArtifactProviderState {
    let page = artifact_search_query::page(query);
    let size = artifact_search_query::page_size(query) as u64;
    let token = artifact_search_query::cursor(query).map(str::to_owned);
    match artifact_search_query::artifact_type(query) {
        ArtifactType::Crates | ArtifactType::Packagist | ArtifactType::Go => {
            ArtifactProviderState {
                page: Some(page),
                token,
                ..Default::default()
            }
        }
        _ => ArtifactProviderState {
            offset: Some(page.saturating_sub(1).saturating_mul(size)),
            token,
            ..Default::default()
        },
    }
}

/// The limit a discovery `query` ends at when its page maps to a registry
/// position past the paging range; `None` when the registry can serve it.
/// A next-page lead names only a page this admits.
pub(crate) fn page_out_of_range(query: &ArtifactSearchQuery) -> Option<TerminalLimit> {
    let artifact_type = artifact_search_query::artifact_type(query);
    validate_cursor_state(
        artifact_type,
        &page_state(query),
        artifact_search_query::page_size(query),
    )
    .err()?;
    Some(TerminalLimit::result_cap(format!(
        "page {} starts past result {}, the deepest {artifact_type} search pages.",
        artifact_search_query::page(query),
        max_start_offset(artifact_type)
    )))
}

/// The deepest result a registry without its own entry in [`reach`] serves:
/// rubygems.org's search window (its search API errors from the page that
/// crosses row 10,000: page 334 of 30 rows, measured 2026-10-31). A page
/// past it fails loudly rather than serving a wrong page labeled as final
/// data. A page number is bounded only by the row it starts at.
const MAX_CURSOR_OFFSET: u64 = 10_000;
/// npm search serves `from` up to 5,000; a deeper `from` answers with the
/// first rows again (registry.npmjs.org, measured 2026-10-15).
const NPM_MAX_FROM: u64 = 5_000;

/// How deep a registry's discovery results reach: the one table both
/// page bounds derive from.
struct Reach {
    /// The deepest result position a discovery page may start at.
    max_start: u64,
    /// The result position no row is served past, when the registry ends
    /// its results there rather than at a page's start; `None` when only
    /// the start of a page is bounded.
    end: Option<u64>,
}

impl Reach {
    /// A registry that serves rows up to `end`: the last page starts at
    /// its final row.
    const fn ending_at(end: u64) -> Self {
        Self {
            max_start: end - 1,
            end: Some(end),
        }
    }

    const fn starting_at_most(max_start: u64) -> Self {
        Self {
            max_start,
            end: None,
        }
    }
}

fn reach(artifact_type: ArtifactType) -> Reach {
    match artifact_type {
        ArtifactType::Npm => Reach::starting_at_most(NPM_MAX_FROM),
        ArtifactType::Crates => Reach::ending_at(crates::CRATES_RELEVANCE_LIMIT),
        ArtifactType::Maven => Reach::ending_at(maven::REACHABLE_END),
        ArtifactType::Nuget => Reach::ending_at(nuget::REACHABLE_END),
        ArtifactType::Go => Reach::ending_at(go::GO_SEARCH_CAP),
        _ => Reach::starting_at_most(MAX_CURSOR_OFFSET),
    }
}

/// The deepest result position a discovery page of `artifact_type` may
/// start at.
fn max_start_offset(artifact_type: ArtifactType) -> u64 {
    reach(artifact_type).max_start
}

/// The result position a discovery of `artifact_type` reaches no row past
/// ([`Reach::end`]).
pub(crate) fn reachable_end(artifact_type: ArtifactType) -> Option<u64> {
    reach(artifact_type).end
}

/// The result position a page of `page_size` rows starts at: the offset of
/// an offset-addressed registry, `(page - 1) * page_size` of a
/// page-addressed one.
fn page_start(state: &ArtifactProviderState, page_size: usize) -> u64 {
    state.offset.unwrap_or_else(|| {
        state
            .page
            .unwrap_or(1)
            .saturating_sub(1)
            .saturating_mul(page_size as u64)
    })
}

fn validate_cursor_state(
    artifact_type: ArtifactType,
    state: &ArtifactProviderState,
    page_size: usize,
) -> Result<(), ArtifactError> {
    let out_of_range =
        page_start(state, page_size) > max_start_offset(artifact_type) || state.page == Some(0);
    if out_of_range {
        return Err(ArtifactError::new(
            ErrorCode::InvalidInput,
            "page is outside the supported paging range; narrow the keywords.",
        ));
    }
    Ok(())
}

/// The contract's `version` capability table (`x-versionSupport` on the
/// artifactSearch `version` field): ecosystem → `range`, `exact`, or
/// `none`, in the contract's order.
fn version_table() -> &'static [(String, String)] {
    static TABLE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        fn find(value: &serde_json::Value) -> Option<&serde_json::Map<String, serde_json::Value>> {
            match value {
                serde_json::Value::Object(map) => map
                    .get("x-versionSupport")
                    .and_then(serde_json::Value::as_object)
                    .or_else(|| map.values().find_map(find)),
                serde_json::Value::Array(items) => items.iter().find_map(find),
                _ => None,
            }
        }
        crate::contracts::tool_contract(crate::id::ToolId::ArtifactSearch)
            .ok()
            .and_then(|tool| find(&tool["inputSchema"]))
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(ecosystem, level)| {
                        Some((ecosystem.clone(), level.as_str()?.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// `range`, `exact`, or `none` (also for an ecosystem the table omits).
fn version_support(ecosystem: &str) -> &'static str {
    version_table()
        .iter()
        .find(|(name, _)| name == ecosystem)
        .map_or("none", |(_, level)| level.as_str())
}

/// "version takes ranges for npm, pypi, crates and exact versions for …".
fn version_support_summary() -> String {
    let list = |level: &str| {
        version_table()
            .iter()
            .filter(|(_, support)| support == level)
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "version takes ranges for {} and exact versions for {}",
        list("range"),
        list("exact")
    )
}

#[cfg(test)]
mod cursor_state_tests;

#[cfg(test)]
mod version_gate_tests;

#[cfg(test)]
mod exact_lookup_tests;
