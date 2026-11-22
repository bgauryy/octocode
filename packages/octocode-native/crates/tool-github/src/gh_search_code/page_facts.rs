//! What a searched page states about itself: its coverage, the walk state
//! its continuation carries, its read leads and its completeness.
use super::search_page::SearchPage;
use super::{GhSearchCodeQuery, code_output};
use octocode_engine::num::{saturating_u64, saturating_usize};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::paging::mark_partial;
use octocode_runtime_base::response::result::ToolData;
use octocode_tools_shared::gh_shared::{SearchCompleteness, apply_partial, push_warning_front};
use serde_json::json;

/// The searched page's `coverage`: `state` reads the page's own partial
/// decision once it is final (the `isPartial` the page marked and the
/// reasons its diagnostics carry, which the row stage marks the same way),
/// so the two never disagree. It is stated where it qualifies something: a
/// partial page, or an empty one (an absence is proven only by complete
/// coverage). A page of hits that is not partial is complete by what it
/// lacks. `indexLimit` states the index's file-size boundary only where the
/// host's differs from github.com's (the tool description states that
/// one), since a larger file is absent from every search without a trace.
pub(super) fn disclose_coverage(output: &mut ToolData, endpoint: &octocode_github::GitHubEndpoint) {
    // A scope the index does not cover at all already says so.
    if output.data["coverage"]["state"].is_string() {
        return;
    }
    let partial =
        output.data["isPartial"] == true || !output.diagnostics.partial_reasons.is_empty();
    let empty = output.status == Some("empty");
    if let Some(mut coverage) = coverage(partial, partial || empty, endpoint) {
        // An empty search is complete over what GitHub indexes, never over
        // every file: an unindexed one was not searched.
        if empty && !partial {
            coverage["reason"] = json!(
                "complete over GitHub's default-branch code index; a file it does not index was not searched"
            );
        }
        output.data["coverage"] = coverage;
    }
}

pub(super) fn coverage(
    partial: bool,
    state: bool,
    endpoint: &octocode_github::GitHubEndpoint,
) -> Option<serde_json::Value> {
    let mut coverage = serde_json::Map::new();
    if state {
        coverage.insert(
            "state".into(),
            json!(if partial { "partial" } else { "complete" }),
        );
    }
    let limit = index_max_file_bytes(endpoint);
    if limit != GITHUB_COM_INDEX_MAX_FILE_BYTES {
        coverage.insert("indexLimit".into(), json!({"maxFileBytes": limit}));
    }
    (!coverage.is_empty()).then_some(serde_json::Value::Object(coverage))
}

/// github.com's (and GHE.com's) index limit, the value the tool description
/// states and a page omits.
const GITHUB_COM_INDEX_MAX_FILE_BYTES: u64 = 384 * 1024;

/// The largest file GitHub's code index holds on `endpoint`'s deployment,
/// in bytes: a larger file is absent from every code search, a file-name
/// search included. GitHub's cloud (github.com and GHE.com) indexes files
/// under 384 KB (<https://docs.github.com/en/rest/search/search#search-code>);
/// an Enterprise Server, files under 5 MB
/// (<https://docs.github.com/en/enterprise-server@latest/search-github/searching-on-github/searching-code#considerations-for-code-search>).
/// An Enterprise Server also searches only each file's first 500 KB: a fact of
/// the deployment, not of a page, so the tool description states it once.
pub(super) fn index_max_file_bytes(endpoint: &octocode_github::GitHubEndpoint) -> u64 {
    match endpoint.deployment() {
        octocode_github::Deployment::Cloud => GITHUB_COM_INDEX_MAX_FILE_BYTES,
        octocode_github::Deployment::Server => 5 * 1024 * 1024,
    }
}

/// `query` carrying the walk through `data`'s page, for `next.nextPage`:
/// every file listed so far (`within`: the walk state of a page only part of
/// which is shown, [`listed::Listed::state_within`](super::listed::Listed::state_within)), the match count GitHub
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

/// The page's reads: `readHits`, `readHits2`, …, one per file whose hit
/// lines were capped, cut or not resolved.
pub(super) fn add_reads(value: &mut serde_json::Value, reads: code_output::PageReads) {
    for (position, read) in reads.hits.into_iter().enumerate() {
        value["calls"][octocode_runtime_base::id::numbered("readHits", position)] = read;
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
    // An owner-wide search past GitHub's window reaches every match only
    // one repository at a time: the owner's repositories list the scopes
    // that split it (code search has no date or size field to split by).
    if data.matched > data.total && current == 1 && query.repo.is_none() {
        value["calls"]["listRepositories"] =
            octocode_runtime_base::response::result::Continuation::related(
                ToolId::GhSearchRepo,
                json!({"operation": "list", "owner": query.owner}),
            )
            // The only way to the matches past the window.
            .required()
            .build();
        octocode_runtime_base::response::paging::push_warnings(
            value,
            ["Past the window, this owner-wide search reaches every match one repository at a time: the related ghSearchRepo listing names the owner's repositories.".to_owned()],
        );
    }
    if hits_unlisted && !more {
        mark_partial(
            value,
            octocode_runtime_base::response::paging::PartialReason::HitLinesNotListed,
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
            octocode_runtime_base::response::paging::PartialReason::ProviderResultsShifted,
            &warnings,
        );
    }
    if let Some(warning) = octocode_github::sso_partial_warning(&data.sso_partial) {
        push_warning_front(value, warning);
    }
}

/// The page's `pagination` when the search pages or the page is shown in
/// parts (`part`): it starts after every file the walk has shown (the
/// earlier pages' files and those the earlier parts of this page showed) and
/// holds the files this response shows, out of the reachable total, so every
/// response states where it stands in the walk. `hasMore` is true while any
/// file of the walk is left: another search page, or the rest of this one
/// (which continues as the response).
pub(super) fn pagination(
    data: &SearchPage,
    current: usize,
    past_end: bool,
    part: Option<Part>,
) -> Option<serde_json::Value> {
    (data.pages > 1 || past_end || part.is_some()).then(|| {
        let part = part.unwrap_or(Part {
            shown: data.items.len(),
            continues: false,
        });
        let before = data
            .listed
            .earlier_count()
            .saturating_add(data.listed.shown_count());
        let more = current < data.pages || part.continues;
        let facts =
            octocode_runtime_base::response::pages::PageFacts::open(before, part.shown, more)
                .with_unit("files")
                .out_of_range(past_end);
        // Over several provider pages GitHub's match count is an estimate a
        // walk need not deliver (rows repeat or shift between page reads),
        // and the page's offset counts provider rows, not files delivered:
        // no total is stated. A search that fits one provider page holds
        // every file, so its count is exact on each part.
        if data.pages <= 1 && !past_end {
            facts.with_total(data.total).to_value()
        } else {
            facts.to_value()
        }
    })
}

/// A response that shows part of a search page.
#[derive(Clone, Copy)]
pub(super) struct Part {
    /// Files this response shows.
    pub(super) shown: usize,
    /// Whether files of the page are left after this response.
    pub(super) continues: bool,
}

/// A search GitHub reported incomplete: the row is partial, with its
/// recovery hint.
pub(super) fn disclose_incomplete(output: &mut ToolData, data: &SearchPage) {
    if data.incomplete {
        output.diagnostics.partial(
            octocode_runtime_base::response::paging::PartialReason::ProviderIncompleteResults,
        );
        octocode_runtime_base::response::paging::push_warnings(
            &mut output.data,
            ["GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.".to_owned()],
        );
    }
}

/// The response chars a page states besides its file rows, shown whole or,
/// `continues`, as a part that continues the page: built from the facts the
/// page states before any file is read (its `pagination`, continuation, read
/// count, completeness and coverage) inside the response envelope, so the
/// room left for its rows is the window less what this page adds. A part's
/// continuation is the response's: the channel states it as the reply's
/// `next` ([`octocode_runtime_base::response::continuations::hoist`]), beside
/// the saved page's expiry, so it is measured as the pager publishes it
/// ([`octocode_runtime_base::response::pager::continuing_block_chars`]); a
/// whole page's own walk is served the same way, and the page is measured
/// as served ([`octocode_runtime_base::response::read_share::served_page_chars`]).
pub(super) fn skeleton(
    query: &GhSearchCodeQuery,
    data: &SearchPage,
    (current, past_end, continues): (usize, bool, bool),
    endpoint: &octocode_github::GitHubEndpoint,
) -> usize {
    let more = current < data.pages || continues;
    let mut value = json!({"files": []});
    let part = (continues || data.listed.continues_page()).then_some(Part {
        shown: data.items.len(),
        continues,
    });
    if let Some(pagination) = pagination(data, current, past_end, part) {
        value["pagination"] = pagination;
    }
    let hoisted = if continues {
        octocode_runtime_base::response::pager::continuing_block_chars(ToolId::GhSearchCode)
    } else {
        if more {
            octocode_runtime_base::response::collection::insert_cursor_walk(
                &mut value,
                ToolId::GhSearchCode,
                octocode_runtime_base::response::cursor::issued::placeholder(),
            );
        }
        0
    };
    disclose_completeness(&mut value, query, data, (current, more), false);
    let mut output = ToolData::from(value);
    disclose_incomplete(&mut output, data);
    disclose_coverage(&mut output, endpoint);
    let row = octocode_runtime_base::response::rows::finish_row(
        0,
        output.data,
        None,
        octocode_runtime_base::response::rows::RowParts {
            diagnostics: output.diagnostics,
        },
    );
    let page = octocode_runtime_base::response::rows::envelope(ToolId::GhSearchCode, vec![row]);
    octocode_runtime_base::response::read_share::served_page_chars(page) + hoisted
}

/// The page's warnings about what its lines show: redacted values, rows the
/// path policy withholds, and files over the line-scan read bound.
pub(super) fn disclose_reads(
    value: &mut serde_json::Value,
    reads: &code_output::PageReads,
    context: &octocode_github::RequestContext,
) {
    if reads.redacted > 0 {
        octocode_runtime_base::security::mark_redacted(value);
    }
    if reads.withheld > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page are withheld by the security path policy (a credential file name or a sensitive directory): their rows list the path only, with no lines, and nothing of them was read. No query field lifts it.",
                reads.withheld
            )],
        );
    }
    if reads.panicked > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page failed inside the line scan (a defect, not an unreadable file; please report it): their lines were not resolved (`lineResolved:false`): read them with ghGetFileContent at the commit.",
                reads.panicked
            )],
        );
    }
    if reads.oversized > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) on this page are larger than the line-scan read bound ({} bytes: the scan limit, or the request's smaller response budget), so their lines were not resolved (`lineResolved:false`): read them with ghGetFileContent at the commit.",
                reads.oversized,
                super::lines::read_bound(context).budget.max_body_bytes
            )],
        );
    }
    if reads.unanchored > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) have no resolved index commit; their required reads resolve the ref when run and cannot verify the historical index snapshot.",
                reads.unanchored
            )],
        );
    }
    if reads.unverified_at_ref > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} indexed path(s) on this page could not be verified at the requested ref (`lineResolved:false`). Check those paths with ghStructure at the commit before relying on them.",
                reads.unverified_at_ref
            )],
        );
    }
}
