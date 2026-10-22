mod crates;
mod go;
mod http;
mod maven;
mod npm;
mod npmrc;
mod nuget;
mod packagist;
mod pypi;
mod release_ref;
mod rubygems;
#[cfg(test)]
pub(crate) mod test_http;
pub(crate) mod types;
mod util;
mod versions;

use crate::providers::artifact::types::artifact_search_query;
use crate::providers::artifact::types::artifact_type;
pub use http::{ArtifactCache, ArtifactHttp, SystemArtifactHttp};
pub(crate) use npm::invalid_registry_url;
pub(crate) use npmrc::npm_authorization;
pub use release_ref::{ReleaseTags, TagFuture};
pub use types::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, ResolvedNpmRegistry, TerminalLimit,
};
pub(crate) use util::{github_repo_dir, github_repository, github_slug, release_ref_in};

use http::RegistryClient;
use octocode_io::budget::RequestBudget;

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
            "capabilityUnavailable",
            format!(
                "{}; {} lookups return the latest release.",
                version_support_summary(),
                artifact_type::as_str(artifact_search_query::artifact_type(query))
            ),
        ));
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

/// The provider position of the query's `page`: an item offset for
/// offset-addressed registries, the page itself for page-addressed ones.
/// Every page starts at `(page - 1) * pageSize`, so consecutive pages tile
/// the result list without gaps.
/// A token-paged registry's page also carries the query's `cursor`, the
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

/// Registries cap deep paging well below these bounds; a page beyond them
/// fails loudly rather than serving a wrong page labeled as final data.
const MAX_CURSOR_OFFSET: u64 = 10_000;
const MAX_CURSOR_PAGE: u64 = 1_000;
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
    let out_of_range = page_start(state, page_size) > max_start_offset(artifact_type)
        || state.page.unwrap_or(1) > MAX_CURSOR_PAGE
        || state.page == Some(0);
    if out_of_range {
        return Err(ArtifactError::new(
            "invalidInput",
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
        ] {
            let error =
                validate_cursor_state(ArtifactType::Nuget, &state, 50).expect_err("out of range");
            assert_eq!(error.code, "invalidInput");
        }
        for state in [
            ArtifactProviderState::default(),
            ArtifactProviderState {
                offset: Some(30),
                page: Some(2),
                token: Some("issued".into()),
            },
        ] {
            assert!(validate_cursor_state(ArtifactType::Nuget, &state, 50).is_ok());
        }
    }

    /// A page-addressed registry's page is bounded by the result position
    /// it starts at, like an offset: crates.io serves results below 1,000,
    /// so 100-row pages end at page 10 and 30-row pages at page 34.
    #[test]
    fn a_page_addressed_registry_is_bounded_by_its_page_start() {
        let at = |page: u64| ArtifactProviderState {
            page: Some(page),
            ..Default::default()
        };
        for (page, page_size, admitted) in [
            (10, 100, true),
            (11, 100, false),
            (34, 30, true),
            (35, 30, false),
        ] {
            assert_eq!(
                validate_cursor_state(ArtifactType::Crates, &at(page), page_size).is_ok(),
                admitted,
                "page {page} of {page_size}"
            );
        }
    }

    /// The query's cursor reaches the provider as the page's token, for
    /// every paging style; a query without one carries none.
    #[test]
    fn the_query_cursor_is_the_provider_page_token() {
        for (ecosystem, page) in [("go", Some(3)), ("nuget", None)] {
            let query = types::artifact_query(
                serde_json::json!({"ecosystem": ecosystem, "keywords": ["http"],
                    "page": 3, "pageSize": 10, "cursor": "issued"}),
                None,
            );
            let state = page_state(&query);
            assert_eq!(state.token.as_deref(), Some("issued"), "{ecosystem}");
            assert_eq!(state.page, page, "{ecosystem}");
        }
        let plain = types::artifact_query(
            serde_json::json!({"ecosystem": "go", "keywords": ["http"]}),
            None,
        );
        assert_eq!(page_state(&plain).token, None);
    }

    /// The gate reads the contract's capability table, so a new
    /// ecosystem level changes one core table, not a native `matches!`.
    #[test]
    fn version_gate_reads_the_contract_capability_table() {
        assert_eq!(version_support("npm"), "range");
        assert_eq!(version_support("maven"), "exact");
        assert_eq!(version_support("go"), "exact");
        assert_eq!(version_support("packagist"), "none");
        assert_eq!(version_support("unknown"), "none");
        let summary = version_support_summary();
        assert!(
            summary.contains("ranges for crates, npm, pypi"),
            "{summary}"
        );
        assert!(
            summary.contains("exact versions for go, maven, nuget"),
            "{summary}"
        );
    }
}

#[cfg(test)]
mod exact_lookup_tests {
    use super::http::RegistryClient;
    use super::test_http::RouteHttp;
    use super::types::{exact_query, test_budget};
    use super::*;
    use serde_json::json;

    /// An exact lookup on each registry parses one row with its name,
    /// version and registry page.
    #[tokio::test]
    async fn exact_lookups_parse_one_row_per_registry() {
        let cases = [
            (
                ArtifactType::Pypi,
                "requests",
                json!({
                    "info": {
                        "name": "requests",
                        "version": "2.31.0",
                        "summary": "Python HTTP for Humans.",
                        "license": "Apache-2.0",
                        "home_page": "https://requests.readthedocs.io",
                        "project_urls": {"Source": "https://github.com/psf/requests"}
                    }
                }),
                "2.31.0",
                "https://pypi.org/project/requests/",
            ),
            (
                ArtifactType::Crates,
                "serde",
                json!({
                    "crate": {
                        "id": "serde",
                        "name": "serde",
                        "max_stable_version": "1.0.200",
                        "description": "A generic serialization/deserialization framework",
                        "license": "MIT OR Apache-2.0",
                        "repository": "https://github.com/serde-rs/serde"
                    }
                }),
                "1.0.200",
                "https://crates.io/crates/serde",
            ),
            (
                ArtifactType::Packagist,
                "laravel/framework",
                // p2/{name}.json response with entries for the tagged URL
                json!({
                    "packages": {
                        "laravel/framework": [
                            {
                                "name": "laravel/framework",
                                "version": "10.0.0",
                                "description": "The Laravel Framework.",
                                "license": ["MIT"]
                            }
                        ]
                    }
                }),
                "10.0.0",
                "https://packagist.org/packages/laravel/framework",
            ),
            (
                ArtifactType::Rubygems,
                "rails",
                json!({
                    "name": "rails",
                    "version": "7.0.6",
                    "info": "Full-stack web application framework.",
                    "homepage_uri": "https://rubyonrails.org",
                    "source_code_uri": "https://github.com/rails/rails"
                }),
                "7.0.6",
                "https://rubygems.org/gems/rails",
            ),
        ];
        for (ecosystem, name, body, version, registry_url) in cases {
            let http = RouteHttp::always(body);
            let b = test_budget();
            let client = RegistryClient::uncached(&http, &b);
            let q = exact_query(ecosystem, name);
            let state = ArtifactProviderState::default();
            let page = match ecosystem {
                ArtifactType::Pypi => pypi::pypi(&q, &client).await,
                ArtifactType::Crates => crates::crates(&q, &state, &client).await,
                ArtifactType::Packagist => packagist::packagist(&q, &state, &client).await,
                ArtifactType::Rubygems => rubygems::rubygems(&q, &state, &client).await,
                other => unreachable!("no exact-lookup case for {other:?}"),
            }
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(page.artifacts.len(), 1, "{name}");
            let item = &page.artifacts[0];
            assert_eq!(item.name, name);
            assert_eq!(item.version.as_deref(), Some(version), "{name}");
            assert_eq!(item.registry_url, registry_url, "{name}");
        }
    }
}
