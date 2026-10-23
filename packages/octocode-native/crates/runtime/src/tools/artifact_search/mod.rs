//! Typed artifact registry discovery and exact metadata lookup.
mod leads;
mod output;
mod pages;

use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
pub(crate) use output::Output;
use pages::discovery_page;
pub(crate) use pages::{last_reachable_page, settle_repaired_row};

use crate::providers::artifact::{
    ArtifactCache, ArtifactError, ArtifactProviderContext, ArtifactProviderPage, ReleaseTags,
    SystemArtifactHttp, execute_artifact, github_repository, github_slug, invalid_registry_url,
};
use crate::response::paging;
use octocode_io::budget::RequestBudget;
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{
    ArtifactItem, ArtifactSearchQuery, ArtifactType, ResolvedNpmRegistry,
};

/// What one call takes from the runtime besides its query.
pub struct ArtifactCall<'a> {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    /// Opt-in escape hatch for the npm registry SSRF guard (`network`).
    pub allow_private_registry: bool,
    /// The runtime's registry cache; `None` (non-persistent storage)
    /// bypasses it.
    pub cache: Option<&'a ArtifactCache>,
    /// The runtime's network timeout: each call's request budget, within its
    /// deadline.
    pub timeout: std::time::Duration,
    /// The runtime's retries of a 5xx or 429 registry answer.
    pub max_retries: u8,
    /// Upstream release-tag checks through the GitHub API.
    pub tags: Option<&'a dyn ReleaseTags>,
    /// The runtime's resolved environment: the npm userconfig and its
    /// `${NAME}` credential values come from it, never from the process.
    pub env: &'a std::collections::BTreeMap<String, String>,
}

pub async fn execute(
    query: ArtifactSearchQuery,
    call: ArtifactCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared()?.with_retries(call.max_retries);
    let budget = RequestBudget::new(
        call.deadline.min(Instant::now() + call.timeout),
        call.cancellation,
        16 * 1024 * 1024,
    );
    let npm_registry = npm_registry(&query, call.env)?;
    let page = execute_artifact(
        &query,
        &ArtifactProviderContext {
            http: &http,
            budget: &budget,
            npm_registry: npm_registry.as_ref(),
            allow_private_registry: call.allow_private_registry,
            cache: call.cache,
            tags: call.tags,
        },
    )
    .await
    .map_err(|error| leads::with_repair(error, &query))?;
    respond(&query, page).map_err(|error| leads::with_repair(error, &query))
}

/// The npm registry a query reads: only the query names it (default
/// npmjs); credentials come only from the user npmrc `env` names and only
/// when scoped to that origin. Other ecosystems have none.
fn npm_registry(
    query: &ArtifactSearchQuery,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<Option<ResolvedNpmRegistry>, ArtifactError> {
    if artifact_search_query::artifact_type(query) != ArtifactType::Npm {
        return Ok(None);
    }
    let mut registry = match artifact_search_query::registry_url(query) {
        Some(raw) => {
            let base = url::Url::parse(raw).map_err(|_| invalid_registry_url())?;
            let cache_identity = base.host_str().unwrap_or("npm").to_owned();
            ResolvedNpmRegistry {
                base,
                authorization: None,
                cache_identity,
            }
        }
        None => ResolvedNpmRegistry::npmjs(),
    };
    registry.authorization = crate::providers::artifact::npm_authorization(&registry.base, env);
    Ok(Some(registry))
}

/// The response for one provider page. Rows are typed until here: a
/// homepage that restates the repository page is cleared before rows are
/// encoded, so every row keeps its field order (`name` first). An exact
/// lookup has no pagination; a discovery page states whether more exist and
/// the registry's total, and a later page names itself.
fn respond(
    query: &ArtifactSearchQuery,
    page: ArtifactProviderPage,
) -> Result<Value, ArtifactError> {
    let ArtifactProviderPage {
        mut artifacts,
        next_state,
        total,
        terminal_limit,
        ..
    } = page;
    if artifacts.is_empty()
        && let Some(name) = artifact_search_query::package_name(query)
        && terminal_limit.is_none()
    {
        // An exact coordinate the registry does not know is not-found, like
        // a GitHub 404; only a keyword discovery can be empty.
        return Err(ArtifactError::new(
            ErrorCode::NotFound,
            format!(
                "Package {name} not found in the {} registry",
                artifact_search_query::artifact_type(query)
            ),
        )
        .with_status(404));
    }
    let details = artifact_search_query::wants(query, octocode_contract_types::ExtraKind::Details);
    shape_rows(query, &mut artifacts, details);
    let mut rows = serde_json::to_value(&artifacts)
        .map_err(|_| ArtifactError::new(ErrorCode::ProviderError, "Failed to encode artifacts."))?;
    for row in rows.as_array_mut().into_iter().flatten() {
        name_github_repository(row, details);
    }
    let mut data = json!({ "artifacts": rows });
    // An empty page the registry ended the list at reaches only the rows
    // before it.
    let total = match (&terminal_limit, artifacts.is_empty()) {
        (Some(_), true) => total.map(|total| {
            total.min(
                artifact_search_query::page(query)
                    .saturating_sub(1)
                    .saturating_mul(artifact_search_query::page_size(query) as u64),
            )
        }),
        _ => total,
    };
    match artifact_search_query::package_name(query) {
        None => discovery_page(query, &mut data, next_state.as_ref(), total),
        // An exact lookup whose source lives on GitHub continues to its
        // tree. Registry metadata can point at a fork or stale repo, so
        // these are leads, not proof.
        Some(_) => {
            for (name, lead) in artifacts
                .first()
                .map(leads::source_leads)
                .unwrap_or_default()
            {
                data["next"][name] = lead;
            }
        }
    }
    paging::push_warnings(
        &mut data,
        artifacts
            .iter()
            .filter_map(leads::source_warning)
            .chain(artifacts.iter().filter_map(leads::property_warning)),
    );
    if let Some(limit) = terminal_limit {
        paging::terminal_limit(&mut data, limit.reason, &[limit.warning]);
    }
    if artifacts.is_empty() {
        data["status"] = json!("empty");
        leads::broaden_empty(&mut data, query);
    }
    Ok(data)
}

/// Typed row shaping before encoding: a row whose registry names no
/// repository but whose homepage is a GitHub repository points at it; a
/// homepage restating the repository is cleared (kept with `details`); the
/// release source is labeled.
fn shape_rows(query: &ArtifactSearchQuery, artifacts: &mut [ArtifactItem], details: bool) {
    let exact = artifact_search_query::package_name(query).is_some();
    for artifact in artifacts {
        if artifact.repository.is_none() {
            artifact.repository = artifact.homepage.as_deref().and_then(github_repository);
        }
        if !details {
            drop_repository_homepage(artifact);
        }
        leads::label_release_source(artifact, exact);
    }
}

/// A row whose `repository` URL is a GitHub repository names its `owner` and
/// `repo` right after it: the coordinates every GitHub tool takes. A URL
/// that is exactly `https://github.com/{owner}/{repo}` then says nothing
/// more and is dropped (kept with `details`).
fn name_github_repository(row: &mut Value, details: bool) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let Some((owner, repo)) = fields
        .get("repository")
        .and_then(Value::as_str)
        .and_then(github_slug)
    else {
        return;
    };
    let after = fields
        .keys()
        .position(|key| key == "repository")
        .map_or(fields.len(), |index| index + 1);
    let restated = !details
        && fields.get("repository").and_then(Value::as_str)
            == Some(format!("https://github.com/{owner}/{repo}").as_str());
    fields.shift_insert(after, "owner".to_owned(), json!(owner));
    fields.shift_insert(after + 1, "repo".to_owned(), json!(repo));
    if restated {
        fields.shift_remove("repository");
    }
}

/// Clears a homepage that is the repository page, or a `/tree/<ref>/…`
/// page inside it (it adds nothing). The registry URL is verbose (core
/// field class); every other field is evidence.
fn drop_repository_homepage(artifact: &mut ArtifactItem) {
    let bare = |url: &str| {
        url.split(['#', '?'])
            .next()
            .unwrap_or(url)
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .to_ascii_lowercase()
    };
    let duplicate = match (artifact.homepage.as_deref(), artifact.repository.as_deref()) {
        (Some(homepage), Some(repository)) => {
            let (page, repository) = (bare(homepage), bare(repository));
            page == repository || page.starts_with(&format!("{repository}/tree/"))
        }
        _ => false,
    };
    if duplicate {
        artifact.homepage = None;
    }
}

/// The generated query for a test's JSON row.
#[cfg(test)]
fn typed(query: &Value) -> ArtifactSearchQuery {
    serde_json::from_value(query.clone()).expect("valid artifactSearch query")
}

/// A test call: no cache, no tag checks, the default network settings.
#[cfg(test)]
fn call(deadline: Instant, allow_private_registry: bool) -> ArtifactCall<'static> {
    ArtifactCall {
        deadline,
        cancellation: CancellationToken::new(),
        allow_private_registry,
        cache: None,
        timeout: std::time::Duration::from_secs(300),
        max_retries: 1,
        tags: None,
        env: &NO_ENV,
    }
}

/// A test runtime environment with no npm userconfig or home.
#[cfg(test)]
static NO_ENV: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

#[cfg(test)]
mod row_tests;

#[cfg(test)]
mod anchor_tests;

#[cfg(test)]
mod discovery_tests;

#[cfg(test)]
mod page_tests;

#[cfg(test)]
mod npm_auth_tests;

#[cfg(test)]
mod npm_window_tests;
