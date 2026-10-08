mod http;
mod maven;
mod npm;
mod npmrc;
mod nuget;
mod registries;
mod release_ref;
mod types;
mod util;
mod versions;

pub use http::{ArtifactCache, ArtifactHttp, SystemArtifactHttp};
pub(crate) use npmrc::npm_authorization;
pub use release_ref::{ReleaseTags, TagFuture};
pub use types::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, ResolvedNpmRegistry,
};

use crate::providers::RequestBudget;
use http::RegistryClient;
use url::Url;

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
    validate_cursor_state(&state)?;
    if query.version().is_some() && version_support(query.artifact_type().as_str()) == "none" {
        return Err(ArtifactError::new(
            "capabilityUnavailable",
            format!(
                "{}; {} lookups return the latest release.",
                version_support_summary(),
                query.artifact_type().as_str()
            ),
        )
        .with_hint("Omit version, or read the release in its source repository."));
    }
    match query.artifact_type() {
        ArtifactType::Npm => {
            let default_registry = ResolvedNpmRegistry {
                base: Url::parse("https://registry.npmjs.org/").map_err(|_| {
                    ArtifactError::new("invalidInput", "Invalid default npm registry URL.")
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
        ArtifactType::Pypi => registries::pypi(query, &client).await,
        ArtifactType::Crates => registries::crates(query, &state, &client).await,
        ArtifactType::Go => registries::go(query, &state, &client).await,
        ArtifactType::Packagist => registries::packagist(query, &state, &client).await,
        ArtifactType::Rubygems => registries::rubygems(query, &state, &client).await,
        ArtifactType::Maven => maven::maven(query, &state, &client).await,
        ArtifactType::Nuget => nuget::nuget(query, &state, &client).await,
    }
}

/// The provider position of the query's `page`: an item offset for
/// offset-addressed registries, the page itself for page-addressed ones.
/// Every page starts at `(page - 1) * pageSize`, so consecutive pages tile
/// the result list without gaps.
fn page_state(query: &ArtifactSearchQuery) -> ArtifactProviderState {
    let page = query.page();
    let size = query.page_size().unwrap_or(10) as u64;
    match query.artifact_type() {
        ArtifactType::Crates | ArtifactType::Packagist | ArtifactType::Go => {
            ArtifactProviderState {
                page: Some(page),
                ..Default::default()
            }
        }
        _ => ArtifactProviderState {
            offset: Some(page.saturating_sub(1).saturating_mul(size)),
            ..Default::default()
        },
    }
}

/// Registries cap deep paging well below these bounds; a page beyond them
/// fails loudly rather than serving a wrong page labeled as final data.
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
            "invalidInput",
            "page is outside the supported paging range; narrow the keywords.",
        ));
    }
    Ok(())
}

/// The contract's `version` capability table (`x-versionSupport` on the
/// artifactSearch `version` field, AR4): ecosystem → `range`, `exact`, or
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
        crate::contracts::tool_contract(crate::tools::id::ToolId::ArtifactSearch)
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
            ArtifactProviderState {
                token: Some("x".repeat(MAX_CURSOR_TOKEN_BYTES + 1)),
                ..Default::default()
            },
        ] {
            let error = validate_cursor_state(&state).expect_err("out of range");
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
            assert!(validate_cursor_state(&state).is_ok());
        }
    }

    /// AR4: the gate reads the contract's capability table, so a new
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
