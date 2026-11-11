//! What a searched page states about itself: its coverage, the walk state
//! its continuation carries, its read leads and its completeness.
use super::search_page::SearchPage;
use super::{GhSearchCodeQuery, code_output};
use crate::id::ToolId;
use crate::response::paging::mark_partial;
use crate::response::result::{Continuation, ToolData};
use crate::tools::gh_shared::{SearchCompleteness, apply_partial, push_warning_front};
use octocode_engine::num::{saturating_u64, saturating_usize};
use serde_json::json;

/// The searched page's `coverage`: `state` reads the page's own partial
/// decision once it is final (the `isPartial` the page marked and the
/// reasons its diagnostics carry, which the row stage marks the same way),
/// so the two never disagree; `indexLimit` states the index's file-size
/// boundary, since a larger file is absent from every search without a trace.
pub(super) fn disclose_coverage(output: &mut ToolData, endpoint: &octocode_github::GitHubEndpoint) {
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
pub(super) fn index_max_file_bytes(endpoint: &octocode_github::GitHubEndpoint) -> u64 {
    match endpoint.deployment() {
        octocode_github::Deployment::Cloud => 384 * 1024,
        octocode_github::Deployment::Server => 5 * 1024 * 1024,
    }
}

/// `query` carrying the walk through `data`'s page, for `next.nextPage`:
/// every file listed so far (`within`: the walk state of a page only part of
/// which is shown, [`listed::Listed::state_within`]), the match count GitHub
/// stated and the index head a ref search read, private state the issued
/// continuation carries across independent requests.
pub(super) fn walked(
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    head: Option<&str>,
    within: Option<String>,
) -> GhSearchCodeQuery {
    GhSearchCodeQuery {
        listed: within
            .or_else(|| data.listed.state())
            .and_then(|state| state.try_into().ok()),
        matched: Some(saturating_u64(data.matched)),
        index_head: head
            .and_then(|head| head.try_into().ok())
            .or_else(|| query.index_head.clone()),
        ..query.clone()
    }
}

/// The page's reads: `read` (the top hit) and `readHits`, `readHits2`, …,
/// one per file whose hit lines were capped or cut.
pub(super) fn add_reads(value: &mut serde_json::Value, reads: code_output::PageReads) {
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
pub(super) fn disclose_completeness(
    value: &mut serde_json::Value,
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    (current, more): (usize, bool),
    hits_unlisted: bool,
) {
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
    if let Some(mut warnings) = data.shift_warnings(before) {
        // A later part lists the same page again from the same answer: its
        // first part stated the shift, so this one keeps only the reason.
        if data.listed.continues_page() {
            warnings.clear();
        }
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

/// The page's `pagination` when the search pages: the merged searches page
/// together, so the page count is the longest search's, not
/// `totalItems / pageSize` (once a short search runs out, the later pages
/// hold the long one's share only). `hasMore` says whether another search
/// page follows; a page shown in parts continues as the response.
pub(super) fn pagination(
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    current: usize,
    past_end: bool,
) -> Option<serde_json::Value> {
    (data.pages > 1 || past_end).then(|| {
        crate::response::pages::PageFacts::sized(
            current,
            data.pages,
            data.total,
            current < data.pages,
        )
        .with_page_size(super::limit(query))
        .out_of_range(past_end)
        .to_value()
    })
}

/// A search GitHub reported incomplete: the row is partial, with its
/// recovery hint.
pub(super) fn disclose_incomplete(output: &mut ToolData, data: &SearchPage) {
    if data.incomplete {
        output
            .diagnostics
            .partial(crate::response::paging::PartialReason::ProviderIncompleteResults);
        output.diagnostics.hint(
            "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
        );
    }
}

/// The response chars the page's row states besides its `rows` file rows,
/// shown whole or, `continues`, as a part that continues the page: built
/// from the facts the page states before any file is read (its
/// `pagination`, continuation, read count, completeness and coverage), so
/// the room left for its rows is the window less what this page adds. A
/// part's continuation is the response's: the channel moves it to
/// `responsePagination` ([`crate::response::continuations::hoist`]), so it
/// is measured there, not as the row's `next.nextPage`.
pub(super) fn skeleton(
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    (current, past_end, continues): (usize, bool, bool),
    endpoint: &octocode_github::GitHubEndpoint,
) -> usize {
    let more = current < data.pages || continues;
    let mut value = json!({"files": []});
    if let Some(pagination) = pagination(query, data, current, past_end) {
        value["pagination"] = pagination;
    }
    let walk = Continuation::cursor(
        ToolId::GhSearchCode.as_str(),
        crate::response::cursor::issued::placeholder(),
    );
    // `,"responsePagination":` around the hoisted block.
    let hoisted = if continues {
        octocode_engine::json_size::utf16_units(&json!({"hasMore": true, "next": walk}))
            .unwrap_or(0)
            + "responsePagination".len()
            + 4
    } else {
        if more {
            value["next"]["nextPage"] = walk;
        }
        0
    };
    disclose_completeness(&mut value, query, data, (current, more), false);
    let mut output = ToolData::from(value);
    disclose_incomplete(&mut output, data);
    disclose_coverage(&mut output, endpoint);
    let row = crate::response::rows::finish_row(
        0,
        output.data,
        None,
        crate::response::rows::RowParts {
            diagnostics: output.diagnostics,
        },
    );
    octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + hoisted
}

/// The page's warnings about what its lines show: redacted values, rows the
/// path policy withholds, and files over the line-scan read bound.
pub(super) fn disclose_reads(
    value: &mut serde_json::Value,
    reads: &code_output::PageReads,
    context: &octocode_github::RequestContext,
) {
    if reads.redacted > 0 {
        crate::response::paging::push_warnings(
            value,
            [crate::security::redacted_content_warning(reads.redacted)],
        );
    }
    if reads.withheld > 0 {
        crate::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page are withheld by the security path policy (a credential file name or a sensitive directory): their rows list the path only, with no lines, and nothing of them was read. No query field lifts it.",
                reads.withheld
            )],
        );
    }
    if reads.panicked > 0 {
        crate::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page failed inside the line scan (a defect, not an unreadable file; please report it): their lines were not resolved (`lineResolved:false`): read them with ghGetFileContent at the commit.",
                reads.panicked
            )],
        );
    }
    if reads.oversized > 0 {
        crate::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page are larger than the line-scan read bound ({} bytes: the scan limit, or the request's smaller response budget), so their lines were not resolved (`lineResolved:false`): read them with ghGetFileContent at the commit.",
                reads.oversized,
                super::lines::read_bound(context).budget.max_body_bytes
            )],
        );
    }
}
