//! ghSearchCode: indexed default-branch code or path search.
mod code_output;
mod fragments;
mod lines;
#[cfg(test)]
mod provider_tests;
mod query;
mod ranking;

use crate::providers::github::{
    CodeSearchRequest, ConditionalCache, GitHubProvider, ProviderError, ProviderErrorKind,
    RequestContext,
};
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{
    GhFailure, SEARCH_RESULT_CAP, add_next, apply_partial, reject_window, search_failure,
    with_ref_recovery,
};
use crate::tools::id::ToolId;
use crate::tools::num::usize_of;
use crate::tools::result::{Continuation, ToolData, remove_null_fields};
use serde_json::{Value, json};

pub use crate::contracts::tool_types::{GhSearchCodeQuery, GhSearchCodeQueryMatch};

/// Run one ghSearchCode row.
pub async fn run<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchCodeQuery,
    request: Result<&RequestContext, ProviderError>,
    security: &impl ContentScan,
) -> Result<ToolData, GhFailure> {
    let result = match request {
        Ok(context) => execute(provider, query, context, security).await,
        Err(error) => Err(error),
    };
    result.map_err(|error| {
        let failure = search_failure(
            error,
            "Lower page, or narrow with path, extensions, or filename to reach deeper results.",
        );
        match query.repo.as_deref() {
            Some(repo) => with_ref_recovery(failure, &query.owner, repo),
            None => failure,
        }
    })
}

pub(crate) async fn execute<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<ToolData, ProviderError> {
    query::validate_code_scope(query)?;
    if !query::code_has_narrowing_selector(query) {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "Code search requires non-empty keywords, path, extension, filename, or language; owner/repo alone is not a bounded code search.",
        ));
    }
    let current = usize_of(query.page);
    let per = usize_of(query.page_size).min(crate::contracts::query_schema_max(
        ToolId::GhSearchCode,
        None,
        "pageSize",
    ));
    reject_window(current, per)?;
    // The commit hit lines are read at does not depend on the hits: resolve
    // it during the index search instead of after it.
    let (data, commit) = futures_util::future::join(
        search_page(provider, query, current, per, context),
        code_output::line_commit(provider, query, context),
    )
    .await;
    let data = data?;
    // GitHub code search does not follow renames: an empty scoped search
    // reruns once against the canonical name, as ghSearchHistory does.
    if data.items.is_empty()
        && let Some((renamed, warnings)) = renamed_scope(provider, query, context).await?
    {
        let mut output = Box::pin(execute(provider, &renamed, context, security)).await?;
        let mut all = warnings.into_iter().map(Value::String).collect::<Vec<_>>();
        if let Some(Value::Array(rest)) = output.data.get_mut("warnings").map(Value::take) {
            all.extend(rest);
        }
        output.data["warnings"] = Value::Array(all);
        return Ok(output);
    }
    let more = current < data.pages;
    let mut items = code_output::files(&data.items, query, security)?;
    let mut value = json!({});
    let resolution = code_output::resolve_lines(
        provider,
        query,
        &items,
        &data.items,
        commit,
        context,
        security,
    )
    .await?;
    let resolved_sha = resolution
        .as_ref()
        .and_then(|resolution| resolution.sha.clone());
    let reads = code_output::shape_files(&mut value, &mut items, query, resolution);
    code_output::merge_identical(&mut items);
    if !items.is_empty() {
        value["files"] = json!(items);
    }
    if data.pages > 1 {
        value["pagination"] = crate::response::pages::PageFacts::open(current, Some(per), more)
            .with_total(data.total)
            .to_value();
    }
    add_next(&mut value, ToolId::GhSearchCode, query, current, more);
    if let Some(read) = reads.top {
        value["next"]["read"] = read;
    }
    // `readHits`, `readHits2`, …: one per capped or cut file.
    for (position, read) in reads.hits.into_iter().enumerate() {
        let key = match position {
            0 => "readHits".to_owned(),
            n => format!("readHits{}", n + 1),
        };
        value["next"][key] = read;
    }
    // The index scope is stated once, on the first page of a ref search.
    if !items.is_empty() && current == 1 {
        code_output::disclose_index_ref(provider, &mut value, query, resolved_sha, context).await?;
    }
    // Provider-index completeness is reported on every page, apart from
    // whether another page exists.
    apply_partial(
        &mut value,
        ToolId::GhSearchCode,
        query,
        data.incomplete,
        // Past the cap the page states GitHub's match count.
        if data.capped { data.matched } else { 0 },
        current,
        more,
        "code",
    );
    let mut output = ToolData::from(value);
    if data.incomplete {
        output.diagnostics.add(
            "ghIncompleteResults",
            "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
            true,
        );
    }
    if data.items.is_empty() {
        output.status = Some("empty");
        empty_page(provider, query, context, &mut output).await?;
    }
    Ok(output)
}

/// One provider page, merged across `extensions`: GitHub ANDs repeated
/// `extension:` qualifiers, so each extension is its own indexed search on
/// the same page number, and the merged page lists every one.
struct SearchPage {
    items: Vec<crate::providers::github::CodeSearchItem>,
    /// Reachable results (each search caps at 1,000).
    total: usize,
    pages: usize,
    capped: bool,
    /// GitHub's match counts, summed over the merged searches.
    matched: usize,
    incomplete: bool,
}

async fn search_page<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchCodeQuery,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<SearchPage, ProviderError> {
    let extensions = query
        .extensions
        .iter()
        .map(|extension| extension.as_str())
        .collect::<Vec<_>>();
    let scopes = match extensions.as_slice() {
        [_, _, ..] => extensions.iter().copied().map(Some).collect(),
        _ => vec![None],
    };
    let mut page = SearchPage {
        items: Vec::new(),
        total: 0,
        pages: 0,
        capped: false,
        matched: 0,
        incomplete: false,
    };
    for extension in scopes {
        let data = provider
            .transport
            .search_code(
                &CodeSearchRequest {
                    query: query::code(query, extension),
                    page: current,
                    per_page: per,
                    include_fragments: query.match_ != GhSearchCodeQueryMatch::Path,
                },
                context,
            )
            .await?;
        let reachable = data.total_count.min(SEARCH_RESULT_CAP);
        page.total += reachable;
        page.pages = page.pages.max(reachable.div_ceil(per));
        page.capped |= data.total_count > SEARCH_RESULT_CAP;
        page.matched += data.total_count;
        page.incomplete |= data.incomplete_results;
        page.items.extend(data.items);
    }
    Ok(page)
}

/// The query re-scoped to a renamed repository's canonical name, with the
/// rename warning; `None` when the name stands or cannot be checked.
async fn renamed_scope<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
) -> Result<Option<(GhSearchCodeQuery, Vec<String>)>, ProviderError> {
    let Some(repo) = query.repo.as_deref() else {
        return Ok(None);
    };
    match provider
        .transport
        .canonical_owner_repo(&query.owner, repo, context)
        .await
    {
        Ok((owner, repo, true, warnings)) => {
            let decode = |error: &dyn std::fmt::Display| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            };
            let mut renamed = query.clone();
            renamed.owner = owner.parse().map_err(|error| decode(&error))?;
            renamed.repo = Some(repo.parse().map_err(|error| decode(&error))?);
            Ok(Some((renamed, warnings)))
        }
        Err(error)
            if matches!(
                error.kind,
                ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
            ) =>
        {
            Err(error)
        }
        Ok(_) | Err(_) => Ok(None),
    }
}

/// Why a search is empty, with the route that verifies it.
async fn empty_page<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    output: &mut ToolData,
) -> Result<(), ProviderError> {
    let value = &mut output.data;
    code_output::empty_scope(
        value,
        &mut output.diagnostics,
        query,
        &provider.transport,
        context,
    )
    .await?;
    if value.get("hints").is_none() && code_output::path_mode_given_code(query) {
        // Path matching never sees file contents: search them instead.
        let mut content = serde_json::to_value(query)
            .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
        remove_null_fields(&mut content);
        content["match"] = json!("file");
        content["page"] = json!(1);
        value["next"]["searchCode"] = Continuation::new(ToolId::GhSearchCode, content)
            .confidence("high")
            .build();
        value["hints"] = json!([
            "match:\"path\" matches file paths, not code; run searchCode to search file contents."
        ]);
    }
    if value.get("hints").is_none() {
        value["hints"] = json!([code_output::empty_hint(query)]);
    }
    Ok(())
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Broaden keywords or remove filters."
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
}
