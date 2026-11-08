//! ghSearchCode: indexed default-branch code or path search.
mod code_output;
mod code_selection;
mod empty;
mod fragments;
mod leads;
mod lines;
mod listed;
#[cfg(test)]
mod provider_tests;
mod query;
mod ranking;
mod search_page;
#[cfg(test)]
mod selection_tests;

use crate::id::ToolId;
use crate::response::paging::mark_partial;
use crate::response::result::ToolData;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{
    GhFailure, Rerun, SearchCompleteness, SearchWindow, add_next, apply_partial, field_names,
    pick_rows, push_warning_front, rerun_renamed, search_failure, search_may_be_renamed,
    with_ref_recovery, with_search_repair,
};
use octocode_engine::num::saturating_usize;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::json;

use octocode_contract_types::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem};

use code_selection::finish_code_selection;
use search_page::{SearchPage, search_page};

/// Whether the path policy withholds a repository file under the current
/// settings ([`crate::policy::path::PathPolicy::withholds_repository_file`]):
/// such a row lists its path only, with no line read or fragment.
pub(crate) type Withholds<'a> = &'a (dyn Fn(&str) -> bool + Sync);

/// Run one ghSearchCode row.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    request: Result<&RequestContext, ProviderError>,
    security: &(impl ContentScan + Sync),
    withholds: Withholds<'_>,
) -> Result<ToolData, GhFailure> {
    let context = request.as_ref().ok().copied();
    let result = match request {
        Ok(context) => execute(provider, query, context, security, withholds).await,
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

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
    withholds: Withholds<'_>,
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
            Box::pin(execute(provider, &renamed, context, security, withholds)).await
        };
        match rerun_renamed(provider, query, &query.owner, repo, context, rerun).await? {
            Rerun::Renamed(output) => return Ok(output),
            Rerun::Stands(metadata) => checked = metadata,
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
        withholds,
    )
    .await?;
    let resolved_sha = resolution
        .as_ref()
        .and_then(|resolution| resolution.sha.clone());
    // The core-quota cost of acquiring this page's source line numbers:
    // the files read through the contents API.
    let content_reads = resolution.as_ref().map_or(0, |resolution| resolution.reads);
    if resolution.is_none() {
        code_output::index_commits(&mut items, &data.items, query);
    }
    let reads = code_output::shape_files(&mut items, query, withholds, resolution);
    if reads.redacted > 0 {
        crate::response::paging::push_warnings(
            &mut value,
            [crate::security::redacted_content_warning(reads.redacted)],
        );
    }
    if reads.withheld > 0 {
        crate::response::paging::push_warnings(
            &mut value,
            [format!(
                "{} file(s) on this page are withheld by the security path policy (a credential file name or a sensitive directory): their rows list the path only, with no lines, and nothing of them was read. No query field lifts it.",
                reads.withheld
            )],
        );
    }
    if reads.oversized > 0 {
        crate::response::paging::push_warnings(
            &mut value,
            [format!(
                "{} file(s) on this page are larger than the line-scan read bound ({} bytes: the scan limit, or the request's smaller response budget), so their lines were not resolved (`lineResolved:false`): read them with ghGetFileContent at the commit.",
                reads.oversized,
                lines::read_bound(context).budget.max_body_bytes
            )],
        );
    }
    if content_reads > 0 {
        value["contentReads"] = json!(content_reads);
    }
    code_output::merge_identical(&mut items);
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
    // Every page of a ref search states the index scope (a page can be read
    // on its own); the head the first page read rides the continuation.
    let head = if items.is_empty() {
        None
    } else {
        code_output::index_head(provider, query, context).await?
    };
    add_next(
        &mut value,
        ToolId::GhSearchCode,
        &walked(query, &data, head.as_deref()),
        current,
        more,
    );
    // A file lists at most a few of its hit lines; `readHits` reads the rest.
    let hits_unlisted = !reads.hits.is_empty();
    add_reads(&mut value, reads);
    if !items.is_empty() {
        code_output::disclose_index_ref(provider, &mut value, query, resolved_sha, head, context)
            .await?;
    }
    disclose_completeness(&mut value, query, &data, current, hits_unlisted);
    let mut output = ToolData::from(value);
    if data.incomplete {
        output
            .diagnostics
            .partial(crate::response::paging::PartialReason::ProviderIncompleteResults);
        output.diagnostics.hint(
            "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
        );
    }
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

/// The searched page's `coverage`: `state` reads the page's own partial
/// decision once it is final (the `isPartial` the page marked and the
/// reasons its diagnostics carry, which the row stage marks the same way),
/// so the two never disagree; `indexLimit` states the index's file-size
/// boundary, since a larger file is absent from every search without a trace.
fn disclose_coverage(output: &mut ToolData, endpoint: &octocode_github::GitHubEndpoint) {
    let partial =
        output.data["isPartial"] == true || !output.diagnostics.partial_reasons.is_empty();
    output.data["coverage"] = json!({
        "state": if partial { "partial" } else { "complete" },
        "indexLimit": {"maxFileBytes": index_max_file_bytes(endpoint)},
    });
}

/// The largest file GitHub's code index holds on `endpoint`'s deployment,
/// in bytes: a larger file is absent from every code search, a file-name
/// search included. GitHub's cloud (github.com and GHE.com) indexes files
/// under 384 KB (<https://docs.github.com/en/rest/search/search#search-code>);
/// an Enterprise Server, files under 5 MB
/// (<https://docs.github.com/en/enterprise-server@latest/search-github/searching-on-github/searching-code#considerations-for-code-search>).
pub(crate) fn index_max_file_bytes(endpoint: &octocode_github::GitHubEndpoint) -> u64 {
    match endpoint.deployment() {
        octocode_github::Deployment::Cloud => 384 * 1024,
        octocode_github::Deployment::Server => 5 * 1024 * 1024,
    }
}

/// `query` carrying the walk through `data`'s page, for `next.nextPage`:
/// every file listed so far, the match count GitHub stated and the index
/// head a ref search read, private state the issued continuation carries
/// across independent requests.
fn walked(query: &GhSearchCodeQuery, data: &SearchPage, head: Option<&str>) -> GhSearchCodeQuery {
    GhSearchCodeQuery {
        listed: data.listed.state().and_then(|state| state.try_into().ok()),
        matched: Some(u64::try_from(data.matched).unwrap_or(u64::MAX)),
        index_head: head
            .and_then(|head| head.try_into().ok())
            .or_else(|| query.index_head.clone()),
        ..query.clone()
    }
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
        mark_partial(
            value,
            crate::response::paging::PartialReason::HitLinesNotListed,
            &[],
        );
    }
    let before = query.matched.map(saturating_usize);
    if let Some(warnings) = data.shift_warnings(before) {
        mark_partial(
            value,
            crate::response::paging::PartialReason::ProviderResultsShifted,
            &warnings,
        );
    }
    if let Some(warning) = octocode_github::sso_partial_warning(&data.sso_partial) {
        push_warning_front(value, warning);
    }
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
    ) {
        finish_code_selection(data, query, hidden);
    }
}
