//! ghSearchCode: indexed default-branch code or path search.
mod code_output;
mod code_selection;
mod fragments;
mod leads;
mod lines;
#[cfg(test)]
mod provider_tests;
mod query;
mod ranking;
mod search_page;
#[cfg(test)]
mod selection_tests;

use crate::id::ToolId;
use crate::num::usize_of;
use crate::response::paging::{mark_partial, push_warnings};
use crate::response::result::{Continuation, ToolData, query_row};
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{
    GhFailure, SearchCompleteness, SearchWindow, add_next, apply_partial, field_names, pick_rows,
    push_warning_front, rerun_renamed, search_failure, search_may_be_renamed, with_ref_recovery,
    with_search_repair,
};
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::json;

use octocode_contract_types::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem};

use code_selection::finish_code_selection;
use search_page::{SearchPage, search_page};

/// Run one ghSearchCode row.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    request: Result<&RequestContext, ProviderError>,
    security: &(impl ContentScan + Sync),
) -> Result<ToolData, GhFailure> {
    let context = request.as_ref().ok().copied();
    let result = match request {
        Ok(context) => execute(provider, query, context, security).await,
        Err(error) => Err(error),
    };
    let error = match result {
        Ok(data) => return Ok(data),
        Err(error) => error,
    };
    // A page past the 1,000-result window repairs to the search's last real
    // page: the window's last page, or the first page's own last page when
    // it names an earlier one (one search, usually cached). Its reachable
    // results are the first page's total (every merged search's).
    let window = match context {
        Some(context)
            if error.reason == Some(octocode_github::ProviderErrorReason::SearchWindowExceeded) =>
        {
            let window_last = SearchWindow::PageEnd.last_page(per_search(query, page_size(query)));
            let first = search_page(provider, query, 1, page_size(query), context)
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
            page_size(query),
            repo,
        );
        if let Some((last, reachable)) = window {
            // Stated in the core policy's page size: each merged search's own
            // page size is internal.
            failure.message = format!(
                "Page {} is past GitHub's search window: at pageSize {}, page {last} is the last{}.",
                usize_of(query.page),
                page_size(query),
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
                    hints: vec![
                        "Check the owner spelling: a user or organization login.".to_owned(),
                    ],
                    ..failure
                }
            }
            None => failure,
        }
    })
}

/// Files each merged search lists per page: GitHub ANDs repeated
/// `extension:` qualifiers, so each of `k` extensions is its own search of
/// `ceil(per / k)` files on the same page number.
fn per_search(query: &GhSearchCodeQuery, per: usize) -> usize {
    per.div_ceil(query.extensions.len().max(1)).max(1)
}

/// Files per page.
fn page_size(_query: &GhSearchCodeQuery) -> usize {
    crate::id::gh_search_code_policy::PAGE_SIZE
}

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<ToolData, ProviderError> {
    let current = usize_of(query.page);
    let per = page_size(query);
    SearchWindow::PageEnd.reject(current, per_search(query, per))?;
    // A ref search's lines are read at the ref's commit, which does not
    // depend on the hits: resolve it during the index search, not after.
    let (data, commit) = futures_util::future::join(
        search_page(provider, query, current, per, context),
        code_output::line_commit(provider, query, context),
    )
    .await;
    // GitHub code search does not follow renames: a scoped search a rename
    // could explain reruns once against the canonical name.
    if let Some(repo) = query.repo.as_deref()
        && search_may_be_renamed(&data, |page| page.fetched == 0 && page.total == 0)
    {
        let rerun = |renamed: GhSearchCodeQuery| async move {
            Box::pin(execute(provider, &renamed, context, security)).await
        };
        if let Some(output) =
            rerun_renamed(provider, query, &query.owner, repo, context, rerun).await?
        {
            return Ok(output);
        }
    }
    let data = data?;
    // A page past the last holds no files of a search that has results.
    let past_end = data.fetched == 0 && data.total > 0 && current > data.pages;
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
    // The core-quota cost of acquiring this page's source line numbers:
    // the files read through the contents API.
    let content_reads = resolution.as_ref().map_or(0, |resolution| resolution.reads);
    if resolution.is_none() {
        code_output::index_commits(&mut value, &mut items, &data.items, query);
    }
    let reads = code_output::shape_files(&mut value, &mut items, query, resolution);
    if content_reads > 0 {
        value["contentReads"] = json!(content_reads);
    }
    code_output::merge_identical(&mut items);
    code_output::hoist_shared(&mut value, &mut items);
    value["files"] = json!(items);
    // The shared sanitizer and selector need the captured hit evidence.
    // A classified response applies the caller's projection after selection.
    if query.classification.is_none() {
        let fields = field_names(&query.fields);
        pick_rows(&mut value, "files", CODE_ROW_ID, &fields);
    }
    if data.pages > 1 || past_end {
        // The merged searches page together, so the page count is the
        // longest search's, not `totalItems / pageSize`: once a short
        // search runs out, the later pages hold the long one's share only.
        value["pagination"] =
            crate::response::pages::PageFacts::sized(current, data.pages, data.total, more)
                .with_page_size(per)
                .out_of_range(past_end)
                .to_value();
    }
    add_next(&mut value, ToolId::GhSearchCode, query, current, more);
    // A file lists at most a few of its hit lines; `readHits` reads the rest.
    let hits_unlisted = !reads.hits.is_empty();
    add_reads(&mut value, reads);
    // The index scope is stated once, on the first page of a ref search.
    if !items.is_empty() && current == 1 {
        code_output::disclose_index_ref(provider, &mut value, query, resolved_sha, context).await?;
    }
    disclose_completeness(&mut value, query, &data, current, hits_unlisted);
    let mut output = ToolData::from(value);
    if data.incomplete {
        output.diagnostics.add(
            "providerIncompleteResults",
            "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
            true,
        );
    }
    if data.fetched == 0 {
        output.status = Some("empty");
    }
    if data.fetched == 0 && data.incomplete && query.repo.is_none() {
        code_output::owner_exists(&query.owner, &provider.transport, context).await?;
    }
    if data.fetched == 0 && !past_end {
        empty_page(provider, query, context, &mut output).await?;
    }
    crate::tools::gh_shared::finish_calls(&mut output.data);
    Ok(output)
}

/// The page's reads: `read` (the top hit) and `readHits`, `readHits2`, …,
/// one per file whose hit lines were capped or cut.
fn add_reads(value: &mut serde_json::Value, reads: code_output::PageReads) {
    if let Some(read) = reads.top {
        value["calls"]["read"] = read;
    }
    for (position, read) in reads.hits.into_iter().enumerate() {
        value["calls"][crate::id::numbered("readHits", position)] = read;
    }
}

/// What the page says about its own completeness: provider-index
/// completeness on every page, apart from whether another page exists; a
/// last page with unlisted hit lines (it has no other way to say why it is
/// partial: an earlier page states its remainder through `pagination`); the
/// estimate and repeat facts of a paged search; and SAML-withheld
/// organizations.
fn disclose_completeness(
    value: &mut serde_json::Value,
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    current: usize,
    hits_unlisted: bool,
) {
    let more = current < data.pages;
    apply_partial(
        value,
        query,
        &SearchCompleteness {
            tool: ToolId::GhSearchCode,
            matched: data.matched,
            reachable: data.total,
            incomplete: data.incomplete,
            page: current,
            has_more: more,
            subject: "code",
        },
    );
    if hits_unlisted && !more {
        mark_partial(value, "hitLinesNotListed", &[]);
    }
    if data.pages > 1 {
        push_warnings(value, data.paging_warnings());
    }
    if let Some(warning) = octocode_github::sso_partial_warning(&data.sso_partial) {
        push_warning_front(value, warning);
    }
}

/// Why a search is empty, with the route that verifies it.
async fn empty_page(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    output: &mut ToolData,
) -> Result<(), ProviderError> {
    let broader = leads::broaden_search(query, provider.transport.code_search_room(context));
    let value = &mut output.data;
    code_output::empty_scope(
        value,
        &mut output.diagnostics,
        query,
        broader.is_some(),
        &provider.transport,
        context,
    )
    .await?;
    if value.get("hints").is_none() && code_output::path_mode_given_code(query) {
        // Path matching never sees file contents: search them instead.
        let mut content = query_row(query, &[]);
        content["matchIn"] = json!(["content"]);
        content["page"] = json!(1);
        value["calls"]["findCode"] = Continuation::related(ToolId::GhSearchCode, content).build();
    }
    let hints = code_output::empty_hints(query);
    if value.get("hints").is_none() && !hints.is_empty() {
        value["hints"] = json!(hints);
        if value["calls"].get("read").is_none()
            && let Some(read) = leads::unsearched_ref_read(query)
        {
            value["calls"]["read"] = read;
        }
    }
    if let Some(broader) = broader {
        crate::tools::leads::broaden::set_broadened(value, &query_row(query, &[]), broader);
    }
    if let Some(split) = leads::or_split(query) {
        crate::tools::leads::push_hint(value, "keywords take no OR; send one row per alternative.");
        value["calls"]["repair"] = split;
    }
    Ok(())
}

/// What a `fields` pick always keeps of a code row: its identity (path, and
/// repository on owner-wide pages) and the facts that qualify its lines
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

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn selects_provider_page(&self) -> bool {
        true
    }

    fn finish_selection(
        &self,
        data: &mut serde_json::Value,
        query: &serde_json::Value,
        hidden: &[serde_json::Value],
    ) {
        finish_code_selection(data, query, hidden);
    }
}
