//! ghSearchCode: indexed default-branch code or path search.
mod code_output;
mod code_selection;
mod empty;
mod fragments;
mod leads;
mod lines;
mod listed;
mod page_facts;
mod part;
#[cfg(test)]
mod provider_tests;
mod query;
mod ranking;
mod search_page;
#[cfg(test)]
mod selection_tests;

use crate::id::ToolId;
use crate::response::result::ToolData;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{
    GhFailure, Rerun, SearchWindow, add_next, field_names, pick_rows, rerun_renamed,
    search_failure, search_may_be_renamed, with_ref_recovery, with_search_repair,
};
use octocode_engine::num::saturating_usize;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::json;

use octocode_contract_types::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem};

use code_selection::finish_code_selection;
#[cfg(test)]
use page_facts::index_max_file_bytes;
use page_facts::{
    add_reads, disclose_completeness, disclose_coverage, disclose_incomplete, disclose_reads,
    pagination, skeleton, walked,
};
use search_page::search_page;

/// Whether the path policy withholds a repository file under the current
/// settings ([`crate::policy::path::PathPolicy::withholds_repository_file`]):
/// such a row lists its path only, with no line read or fragment.
pub(crate) type Withholds<'a> = &'a (dyn Fn(&str) -> bool + Sync);

/// Run one ghSearchCode row. `window` is the row's share of the response
/// page: a page reads the files of the rows it holds ([`part`]); `None`
/// reads every row of the page.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    request: Result<&RequestContext, ProviderError>,
    security: &(impl ContentScan + Sync),
    withholds: Withholds<'_>,
    window: Option<usize>,
) -> Result<ToolData, GhFailure> {
    let context = request.as_ref().ok().copied();
    let result = match request {
        Ok(context) => execute_within(provider, query, context, security, withholds, window).await,
        Err(error) => Err(error),
    };
    let error = match result {
        Ok(data) => return Ok(data),
        Err(error) => error,
    };
    // A page past the 1,000-result window repairs to the search's last real
    // page: the window's last page, or the first page's own last page when
    // it names an earlier one (one search). Its reachable
    // results are the first page's total (every merged search's).
    let window = match context {
        Some(context)
            if error.reason == Some(octocode_github::ProviderErrorReason::SearchWindowExceeded) =>
        {
            let window_last = SearchWindow::PageEnd.last_page(per_search(query, limit(query)));
            let first = search_page(provider, query, 1, limit(query), context)
                .await
                .ok();
            let last = first
                .as_ref()
                .map_or(window_last, |page| page.pages.max(1).min(window_last));
            Some((last, first.map(|page| page.total)))
        }
        _ => None,
    };
    Err({
        let repo = query.repo.as_deref().map(std::string::String::as_str);
        let mut failure = with_search_repair(
            search_failure(error),
            ToolId::GhSearchCode,
            query,
            limit(query),
            Some(&query.owner),
            repo,
        );
        if let Some((last, reachable)) = window {
            // Stated in the query's `limit`: each merged search's own page
            // size is internal.
            failure.message = format!(
                "Page {} is past GitHub's search window: at limit {}, page {last} is the last{}.",
                saturating_usize(query.page.get()),
                limit(query),
                reachable.map_or_else(String::new, |total| format!(
                    ", reaching the first {total} results"
                )),
            );
            if let Some(row) = failure
                .calls
                .as_mut()
                .and_then(|next| next.pointer_mut("/repair/input/queries/0"))
            {
                row["page"] = json!(last);
            }
        }
        match repo {
            Some(repo) => with_ref_recovery(failure, &query.owner, repo),
            // An owner-wide search that found no such login: the message
            // names the login, and its one hint is the spelling.
            None if failure.error.reason
                == Some(octocode_github::ProviderErrorReason::UserNotFound) =>
            {
                GhFailure {
                    message: crate::tools::gh_shared::owner_not_found_message(&query.owner),
                    ..failure
                }
                .hint("Check the owner spelling: a user or organization login.")
            }
            None => failure,
        }
    })
}

/// Files per merged page: the query's `limit`, its contract default when
/// omitted.
fn limit(query: &GhSearchCodeQuery) -> usize {
    saturating_usize(query.limit.get())
}

/// Files each merged search lists per page: GitHub ANDs repeated
/// `extension:` qualifiers, so each of `k` extensions is its own search of
/// `ceil(per / k)` files on the same page number.
fn per_search(query: &GhSearchCodeQuery, per: usize) -> usize {
    per.div_ceil(query.extensions.len().max(1)).max(1)
}

/// [`execute_within`] with no response window: every row of the page.
#[cfg(test)]
pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
    withholds: Withholds<'_>,
) -> Result<ToolData, ProviderError> {
    execute_within(provider, query, context, security, withholds, None).await
}

/// One search page, or the part of it `window` holds ([`run`]).
async fn execute_within(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
    withholds: Withholds<'_>,
    window: Option<usize>,
) -> Result<ToolData, ProviderError> {
    let current = saturating_usize(query.page.get());
    let per = limit(query);
    SearchWindow::PageEnd.reject(current, per_search(query, per))?;
    // A ref search's lines are read at the ref's commit, which does not
    // depend on the hits: resolve it during the index search, not after.
    let (data, commit) = futures_util::future::join(
        search_page(provider, query, current, per, context),
        code_output::line_commit(provider, query, context),
    )
    .await;
    // GitHub code search does not follow renames: a scoped search a rename
    // could explain reruns once against the canonical name. The metadata
    // that check read also answers the empty page's scope check.
    let mut checked = None;
    if let Some(repo) = query.repo.as_deref()
        && search_may_be_renamed(&data, |page| page.fetched == 0 && page.total == 0)
    {
        let rerun = |renamed: GhSearchCodeQuery| async move {
            Box::pin(execute_within(
                provider, &renamed, context, security, withholds, window,
            ))
            .await
        };
        match rerun_renamed(provider, query, &query.owner, repo, context, rerun).await? {
            Rerun::Renamed(output) => return Ok(output),
            Rerun::Stands(metadata) => checked = metadata,
        }
    }
    let data = data?;
    // A page past the last holds no files of a search that has results.
    let past_end = data.fetched == 0 && data.total > 0 && current > data.pages;
    let mut items = code_output::files(&data.items, query, security)?;
    // Rows an earlier part of this page showed are ranked with the rest, so
    // the rest keeps the page's order, and listed once.
    items.retain(|row| !data.was_shown(row));
    let mut value = json!({});
    let plan = code_output::plan_lines(
        provider,
        query,
        &items,
        &data.items,
        commit,
        context,
        withholds,
    )
    .await?;
    // The page reads the files of the rows its window holds; the rest is the
    // next part of the same page.
    let (resolution, unshown) = match plan {
        Some(mut plan) => {
            let endpoint = provider.transport.endpoint();
            let room = part::budget(query, window, |continues| {
                skeleton(query, &data, (current, past_end, continues), endpoint)
            });
            let shown = part::read(
                &mut plan, &items, query, withholds, room, provider, context, security,
            )
            .await?;
            let unshown = items.split_off(shown);
            (Some(plan.resolution(0..shown)), unshown)
        }
        None => (None, Vec::new()),
    };
    let within = (!unshown.is_empty()).then(|| {
        data.listed.state_within(items.iter().filter_map(|row| {
            Some((
                row.get("owner")?.as_str()?,
                row.get("repo")?.as_str()?,
                row.get("path")?.as_str()?,
            ))
        }))
    });
    // `pagination` counts search pages; the rest of a page is this
    // response's own continuation (`continues_page`).
    let more_pages = current < data.pages;
    let continues_page = within.is_some();
    let more = more_pages || continues_page;
    let resolved_sha = resolution
        .as_ref()
        .and_then(|resolution| resolution.sha.clone());
    if resolution.is_none() {
        code_output::index_commits(&mut items, &data.items, query);
    }
    let reads = code_output::shape_files(&mut items, query, withholds, resolution);
    disclose_reads(&mut value, &reads, context);
    code_output::merge_identical(&mut items);
    value["files"] = json!(items);
    // The shared sanitizer and selector need the captured hit evidence.
    // A classified response applies the caller's projection after selection.
    if query.classification.is_none() {
        let fields = field_names(&query.fields);
        pick_rows(&mut value, "files", CODE_ROW_ID, &fields);
    }
    if let Some(pagination) = pagination(&data, current, past_end) {
        value["pagination"] = pagination;
    }
    // Every page of a ref search states the index scope (a page can be read
    // on its own); the head the first page read rides the continuation.
    let head = if items.is_empty() {
        None
    } else {
        code_output::index_head(provider, query, context).await?
    };
    // The rest of a part's page is the same page again (`add_next` names
    // the page after the one it is given), carrying what its parts showed.
    let continued = if continues_page { current - 1 } else { current };
    add_next(
        &mut value,
        ToolId::GhSearchCode,
        &walked(query, &data, head.as_deref(), within),
        continued,
        more,
    );
    // A file lists at most a few of its hit lines; `readHits` reads the rest.
    let hits_unlisted = !reads.hits.is_empty();
    add_reads(&mut value, reads);
    if !items.is_empty() {
        code_output::disclose_index_ref(provider, &mut value, query, resolved_sha, head, context)
            .await?;
    }
    disclose_completeness(&mut value, query, &data, (current, more), hits_unlisted);
    let mut output = ToolData::from(value);
    output.diagnostics.continues_page = continues_page;
    disclose_incomplete(&mut output, &data);
    if data.fetched == 0 {
        output.status = Some("empty");
    }
    if data.fetched == 0 && data.incomplete && query.repo.is_none() {
        code_output::owner_exists(&query.owner, &provider.transport, context).await?;
    }
    if data.fetched == 0 && !past_end {
        empty::page(provider, query, context, checked, &mut output).await?;
    }
    disclose_coverage(&mut output, provider.transport.endpoint());
    crate::response::result::finish_calls(&mut output.data);
    Ok(output)
}

/// What a `fields` pick always keeps of a code row: its identity (path, and
/// repository) and the facts that qualify its lines
/// (where else the same evidence sits, how many hits exist, whether the
/// lines were read at the ref, and the commit they were read at).
const CODE_ROW_ID: &[&str] = &[
    "owner",
    "repo",
    "path",
    "alsoAt",
    "matchCount",
    "atRef",
    "lineResolved",
    "commitSha",
];

/// This tool's selection finish for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn finish_selection(
        &self,
        data: &mut serde_json::Value,
        query: &serde_json::Value,
        hidden: &[serde_json::Value],
        _removed: &[serde_json::Value],
    ) {
        finish_code_selection(data, query, hidden);
    }
}
