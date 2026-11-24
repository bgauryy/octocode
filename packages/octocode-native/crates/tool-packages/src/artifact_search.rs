//! Typed artifact registry discovery and exact metadata lookup.
mod leads;
mod pages;
mod preview;
pub use preview::preview_destinations;
mod summary;

use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactSearchView, ArtifactType, ErrorCode};
use pages::discovery_page;
pub(crate) use pages::last_reachable_page;

use crate::artifact::{
    ArtifactError, ArtifactProviderContext, ArtifactProviderPage, ReleaseTags, SystemArtifactHttp,
    execute_artifact, github_repository, github_slug, invalid_registry_url,
};
use octocode_io::budget::{PROVIDER_BODY_BYTES, RequestBudget};
use octocode_io::cancellation::Cancellation;
use octocode_runtime_base::providers::saved_answers::{ReusedSince, disclose_reuse};
use octocode_runtime_base::response::paging;
use serde_json::{Value, json};
use std::time::Instant;

use crate::artifact::{ArtifactItem, ResolvedNpmRegistry, lookalike};

/// What one call takes from the runtime besides its query.
pub struct ArtifactCall<'a> {
    pub deadline: Instant,
    pub cancellation: Cancellation,
    /// Opt-in escape hatch for the npm registry SSRF guard (`network`).
    pub allow_private_registry: bool,
    /// The runtime's network timeout: each call's request budget, within its
    /// deadline.
    pub timeout: std::time::Duration,
    /// The runtime's retries of a 5xx or 429 registry answer.
    pub max_retries: u8,
    /// Registry answers kept across calls; `None` keeps none.
    pub answers: Option<&'a dyn octocode_github::SavedAnswers>,
    /// Upstream release-tag checks through the GitHub API.
    pub tags: Option<&'a dyn ReleaseTags>,
    /// The runtime's resolved environment: the npm userconfig and its
    /// `${NAME}` credential values come from it, never from the process.
    pub env: &'a std::collections::BTreeMap<String, String>,
    /// What the rows of this call share ([`crate::artifact::CallShare`]);
    /// `None` shares nothing.
    pub share: Option<&'a crate::artifact::CallShare>,
}

pub async fn execute(
    query: ArtifactSearchQuery,
    call: ArtifactCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared().with_retries(call.max_retries);
    let budget = RequestBudget::new(call.deadline, call.cancellation, PROVIDER_BODY_BYTES)
        .with_stall(call.timeout);
    let npm_registry = npm_registry(&query, call.env)?;
    let reused = ReusedSince::default();
    let context = ArtifactProviderContext {
        http: &http,
        budget: &budget,
        npm_registry: npm_registry.as_ref(),
        allow_private_registry: call.allow_private_registry,
        answers: call.answers,
        reused: &reused,
        tags: call.tags,
        share: call.share,
    };
    let mut data = serve(&query, &context).await?;
    if let Some(observed) = reused.get() {
        disclose_reuse(&mut data, observed);
    }
    Ok(data)
}

/// The response `query` gets through `context`: the page as served, or the
/// error with its repair lead. An exact row is checked for a far more
/// downloaded name one edit away ([`leads::lookalike`]).
///
/// The counts of the names one edit away need only the name asked, so they
/// are read beside the registry instead of after it; the registry's own
/// spelling of the name, where it differs, is counted again.
async fn serve(
    query: &ArtifactSearchQuery,
    context: &ArtifactProviderContext<'_>,
) -> Result<Value, ArtifactError> {
    let asked = artifact_search_query::package_name(query)
        .map(|name| (artifact_search_query::artifact_type(query), name));
    // What the early count reused is disclosed only where its answer is used.
    let early_reused = ReusedSince::default();
    let early_context = ArtifactProviderContext {
        http: context.http,
        budget: context.budget,
        npm_registry: context.npm_registry,
        allow_private_registry: context.allow_private_registry,
        answers: context.answers,
        reused: &early_reused,
        tags: context.tags,
        share: context.share,
    };
    let (page, early) = tokio::join!(execute_artifact(query, context), async {
        match asked {
            Some((kind, name)) => lookalike::near_names(kind, name, &early_context).await,
            None => None,
        }
    });
    let page = match page {
        Ok(page) => page,
        Err(error) => return Err(leads::with_repair(near_missing(error, early), query)),
    };
    // The exact row's name as the registry spells it: what its names one
    // edit away are counted against.
    let known = asked
        .and_then(|_| page.artifacts.first())
        .map(|artifact| (artifact.artifact_type, artifact.package_name.clone()));
    let mut data = match respond(query, page) {
        Ok(data) => data,
        Err(error) => return Err(leads::with_repair(near_missing(error, early), query)),
    };
    if let Some((kind, name)) = known {
        let near = if asked == Some((kind, name.as_str())) {
            if let Some(observed) = early_reused.get() {
                context.reused.record(observed);
            }
            early
        } else {
            lookalike::near_names(kind, &name, context).await
        };
        if let Some(near) = near {
            leads::lookalike(&mut data, query, &name, near);
        }
    }
    Ok(data)
}

/// A package the registry does not know carries what the registry counts of
/// the names one edit away (`near`, read for the name asked), for its repair
/// lead; any other failure, and a count that could not be read, carries none.
fn near_missing(
    mut error: ArtifactError,
    near: Option<Result<lookalike::Near, ArtifactError>>,
) -> ArtifactError {
    if error.code != ErrorCode::NotFound || error.missing_version {
        return error;
    }
    if let Some(Ok(near)) = near {
        error.near = Some(Box::new(near));
    }
    error
}

/// The npm registry a query reads: only the query names it (default
/// npmjs); credentials come only from the user npmrc `env` names and only
/// when scoped to that origin. Other ecosystems have none.
fn npm_registry(
    query: &ArtifactSearchQuery,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<Option<ResolvedNpmRegistry>, ArtifactError> {
    let Some(mut registry) = selected_npm_registry(query)? else {
        return Ok(None);
    };
    registry.authorization = crate::artifact::npm_authorization(&registry.base, env);
    Ok(Some(registry))
}

/// Resolve the destination without opening a credential file or resolving DNS.
fn selected_npm_registry(
    query: &ArtifactSearchQuery,
) -> Result<Option<ResolvedNpmRegistry>, ArtifactError> {
    if artifact_search_query::artifact_type(query) != ArtifactType::Npm {
        return Ok(None);
    }
    let registry = match artifact_search_query::registry_url(query) {
        Some(raw) => {
            let base = url::Url::parse(raw).map_err(|_| invalid_registry_url())?;
            ResolvedNpmRegistry {
                base,
                authorization: None,
            }
        }
        None => ResolvedNpmRegistry::npmjs(),
    };
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
        any_keyword_total,
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
    let has_next = next_state.is_some();
    shape_rows(query, &mut artifacts);
    let mut rows = serde_json::to_value(&artifacts)
        .map_err(|_| ArtifactError::new(ErrorCode::ProviderError, "Failed to encode artifacts."))?;
    let view = artifact_search_query::view(query);
    for row in rows.as_array_mut().into_iter().flatten() {
        name_github_repository(row);
        if view == ArtifactSearchView::Summary {
            summary::row(row, query);
        }
    }
    let mut data = json!({ "artifacts": rows });
    // An exact lookup's row states its repository, directory, ref and
    // version: the tree read and the advisory check take them as fields.
    if artifact_search_query::package_name(query).is_none() {
        discovery_page(
            query,
            &mut data,
            next_state.as_ref(),
            total,
            any_keyword_total,
        );
    }
    paging::push_warnings(
        &mut data,
        artifacts
            .iter()
            .filter_map(leads::source_warning)
            .chain(
                artifacts
                    .iter()
                    .filter(|_| artifact_search_query::package_name(query).is_some())
                    .filter_map(leads::unknown_source_warning),
            )
            .chain(artifacts.iter().filter_map(leads::property_warning))
            .chain(artifacts.iter().filter_map(leads::retraction_warning))
            .chain(artifacts.iter().filter_map(leads::prerelease_warning))
            .chain(
                artifacts
                    .iter()
                    .flat_map(|artifact| artifact.disclosures.iter().cloned()),
            ),
    );
    // An empty page can establish absence only when the registry reached the
    // end of its matches. A terminal limit instead states what is unread.
    if let Some(limit) = terminal_limit {
        paging::terminal_limit(&mut data, limit.reason, &[limit.warning]);
    } else if artifacts.is_empty() && !has_next {
        data["status"] = json!("empty");
        leads::broaden_empty(&mut data, query);
    }
    octocode_runtime_base::response::result::finish_calls(&mut data);
    Ok(data)
}

/// Typed row shaping before encoding: a row whose registry names no
/// repository but whose homepage is a GitHub repository points at it; a
/// homepage restating the repository is cleared (kept with `details`); the
/// release source is labeled.
fn shape_rows(query: &ArtifactSearchQuery, artifacts: &mut [ArtifactItem]) {
    let exact = artifact_search_query::package_name(query).is_some();
    for artifact in artifacts {
        if artifact.repository.is_none() {
            artifact.repository = artifact.homepage.as_deref().and_then(github_repository);
        }
        drop_repository_homepage(artifact);
        leads::label_release_source(artifact, exact);
    }
}

/// A row whose `repository` URL is a GitHub repository names its `owner` and
/// `repo` right after it: the coordinates every GitHub tool takes. A URL
/// that is exactly `https://github.com/{owner}/{repo}` then says nothing
/// more and is dropped (kept with `details`).
fn name_github_repository(row: &mut Value) {
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
    let restated = fields.get("repository").and_then(Value::as_str)
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

/// This tool's recovery for the shared response stages: a package the
/// registry does not know is a spelling to check where that registry writes
/// it, never a path to list; a registry host the network policy refuses is
/// the user's setting to change, which the error names, so no text is added.
pub struct Output;
impl octocode_runtime_base::response::output::ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        match code {
            ErrorCode::NotFound => Some(NOT_FOUND_HINT),
            ErrorCode::SecurityValidationFailed => {
                Some(octocode_runtime_base::response::output::NO_HINT)
            }
            _ => None,
        }
    }
}

/// The recovery of a package the registry does not know. The schema already
/// refuses a Maven name that is not `group:artifact`.
const NOT_FOUND_HINT: &str =
    "Check packageName as the registry spells it (an npm scope, a Go module path).";

/// The generated query for a test's JSON row. These tests read the whole
/// row, so a row that states no view asks for `details`; a test of the
/// summary states its view.
#[cfg(test)]
fn typed(query: &Value) -> ArtifactSearchQuery {
    let mut query = query.clone();
    if query.get("view").is_none() {
        query["view"] = json!("details");
    }
    serde_json::from_value(query).expect("valid artifactSearch query")
}

/// A test call: no cache, no tag checks, the default network settings.
#[cfg(test)]
fn call(deadline: Instant, allow_private_registry: bool) -> ArtifactCall<'static> {
    ArtifactCall {
        deadline,
        cancellation: Cancellation::new(),
        allow_private_registry,
        timeout: std::time::Duration::from_secs(300),
        max_retries: 1,
        answers: None,
        tags: None,
        env: &NO_ENV,
        share: None,
    }
}

/// A test runtime environment with no npm userconfig or home.
#[cfg(test)]
static NO_ENV: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

#[cfg(test)]
mod row_tests;

#[cfg(test)]
mod view_tests;

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

#[cfg(test)]
mod limit_tests;

#[cfg(test)]
mod tag_list_tests;

#[cfg(test)]
mod resolve_tests;
