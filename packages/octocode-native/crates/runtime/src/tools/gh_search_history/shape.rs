//! The response for one page: rows, pagination, leads, warnings and the
//! partial state.
use super::{HistoryOperation, HistorySearch, leads, rows};
use crate::id::ToolId;
use crate::response::result::remove_null_fields;
use crate::tools::gh_search_history::gh_search_history_query;
use crate::tools::gh_shared::{SearchCompleteness, SearchWindow, apply_partial, pick_rows};
use octocode_github::HistoryPage;
use serde_json::{Value, json};

/// Where a page sits in the result.
pub(super) struct Paging {
    page: usize,
    per: usize,
    more: bool,
    /// The reachable total (search caps it).
    total: usize,
    /// The provider total exceeds the reachable total.
    capped: bool,
    /// The provider's own match count (a search's `total_count`).
    matched: usize,
    /// A complete one-page REST list counts its rows exactly.
    exact_list_total: Option<usize>,
    listed: bool,
    incomplete: bool,
    /// The page lies past the last one: no rows, though the search has some
    /// (or the list has an earlier page).
    past_end: bool,
}

impl Paging {
    pub(super) fn new(result: &HistoryPage, page: usize, per: usize) -> Self {
        // A REST list has no search window and states no total.
        let reach = if result.listed {
            SearchWindow::PageStart.reach(0, per)
        } else {
            SearchWindow::PageStart.reach(result.total_count, per)
        };
        let more = if result.listed {
            result.has_more
        } else {
            page < reach.pages
        };
        Self {
            page,
            per,
            more,
            total: reach.total,
            capped: reach.capped,
            matched: result.total_count,
            exact_list_total: (result.listed && page == 1 && !more).then_some(result.items.len()),
            listed: result.listed,
            incomplete: result.incomplete_results,
            past_end: result.items.is_empty()
                && page > 1
                && (result.listed || reach.past_last(page)),
        }
    }
    /// The total a page reports: the search total, or an exact list count.
    fn reported_total(&self) -> Option<usize> {
        if self.listed {
            self.exact_list_total
        } else {
            Some(self.total)
        }
    }
    /// The total when it is exact: an uncapped, complete search count or an
    /// exact list count. A capped or incomplete count is no page count.
    fn exact_total(&self) -> Option<usize> {
        if self.listed {
            self.exact_list_total
        } else {
            (!self.capped && !self.incomplete).then_some(self.total)
        }
    }
    /// The page facts; `totalPages` only beside an exact total (a capped
    /// one states `partialReasons:["providerResultCap"]` instead). The
    /// cursor rides `next.nextPage`, bounded by the contract's `page`
    /// maximum.
    fn pagination(&self) -> Value {
        let facts = crate::response::pages::PageFacts::open(self.page, Some(self.per), self.more)
            .out_of_range(self.past_end);
        match self.exact_total() {
            Some(total) => facts.with_total(total),
            None => facts,
        }
        .to_value()
    }
}

/// The response for one page.
pub(super) fn shape(
    query: &HistorySearch,
    result: HistoryPage,
    paging: &Paging,
    terms: String,
) -> Value {
    let HistoryPage {
        items, warnings, ..
    } = result;
    let narrow = if paging.capped {
        leads::narrow_capped(query, &items)
    } else {
        None
    };
    let mut value = match gh_search_history_query::operation(query) {
        HistoryOperation::PullRequest => pull_requests(query, &items, paging, terms),
        HistoryOperation::Issue => issues(query, &items, paging, terms),
        HistoryOperation::Commit => commits(query, items, paging),
    };
    if gh_search_history_query::operation(query) == HistoryOperation::Commit
        && let Some(sha) = value
            .pointer("/commits/0/commitSha")
            .and_then(Value::as_str)
        && let Some(read) = leads::read_commit(query, sha)
    {
        value["next"]["readCommit"] = read;
    }
    if let Some(rows) = value.get("commits").and_then(Value::as_array) {
        if let Some(read) = leads::read_commit_pull_request(query, rows) {
            value["next"]["readPullRequest"] = read;
        } else if let Some(sha) = value
            .pointer("/commits/0/commitSha")
            .and_then(Value::as_str)
            && let Some(find) = leads::find_commit_pull_request(query, sha)
        {
            value["next"]["findPullRequest"] = find;
        }
    }
    if !warnings.is_empty() {
        value["warnings"] = json!(warnings);
    }
    if gh_search_history_query::operation(query) == HistoryOperation::Issue
        && !paging.more
        && !paging.past_end
        && paging.page <= 1
        && let Some(map) = value.as_object_mut()
    {
        map.remove("pagination");
    }
    let next_page = paging.more.then(|| {
        crate::response::paging::next_cursor(
            &mut value,
            ToolId::GhSearchHistory,
            None,
            "page",
            paging.page + 1,
        )
    });
    if let Some(next_page) = next_page.flatten() {
        value["next"]["nextPage"] = leads::next_page(query, next_page, paging.per);
    }
    remove_null_fields(&mut value);
    mark_empty(&mut value, paging.more);
    // A page past the end is restarted by the shared response stage, not
    // broadened: the search has results.
    if value["status"] == "empty"
        && !paging.past_end
        && let Some(lead) = leads::broaden_search(query)
    {
        crate::tools::leads::broaden::set_broadened(
            &mut value,
            &crate::response::result::query_row(&**query, &[]),
            lead,
        );
    }
    // A path scope that listed nothing may name no path at all: its
    // parent's listing checks it.
    if value["status"] == "empty"
        && !paging.past_end
        && let Some(check) =
            crate::tools::gh_shared::path_check(&crate::response::result::query_row(&**query, &[]))
    {
        value["next"]["viewTree"] = check;
    }
    disclose_provider_limits(&mut value, query, paging, narrow);
    value
}

/// Whether pull-request rows keep `createdAt` beside `mergedAt`: when they
/// are sorted by creation, or filtered on it (`since`/`until` set the
/// `created:` window, or a `created:` qualifier).
pub(super) fn keeps_created(query: &HistorySearch) -> bool {
    gh_search_history_query::sort(query).as_deref() == Some("created")
        || gh_search_history_query::since(query).is_some()
        || gh_search_history_query::until(query).is_some()
        || gh_search_history_query::qualifiers(query)
            .is_some_and(|qualifiers| qualifiers.contains("created:"))
}

fn pull_requests(query: &HistorySearch, items: &[Value], paging: &Paging, terms: String) -> Value {
    let fields = gh_search_history_query::fields(query);
    let state = query.state();
    let keep_created = keeps_created(query);
    let rows = items
        .iter()
        .map(|item| {
            let mut row = rows::map_pr(item.clone());
            rows::drop_implied(&mut row, state.as_deref(), keep_created, &fields);
            rows::name_repository(
                &mut row,
                item,
                gh_search_history_query::owner(query),
                gh_search_history_query::repo(query),
            );
            row
        })
        .collect::<Vec<_>>();
    let mut v = json!({"pullRequests":rows,"effectiveQuery":terms,
        "pagination":paging.pagination()});
    pick_rows(
        &mut v,
        "pullRequests",
        &["number", "owner", "repo"],
        &fields,
    );
    // An exact total states totalPages (see `Paging::pagination`); a capped
    // total is `partialReasons:["providerResultCap"]`, not a second flag.
    if !paging.listed {
        v["pagination"]["totalItems"] = json!(paging.total);
    } else if let Some(total) = paging.exact_list_total {
        v["pagination"]["totalItems"] = json!(total);
    }
    // List mode runs the REST endpoint, not the search terms.
    if paging.listed
        && let Some(map) = v.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    if let Some(read) = leads::read_pull_request(query, items) {
        v["next"]["readPullRequest"] = read;
    }
    if let Some(read) = leads::issue_links_read(query) {
        v["next"]["readIssueLinks"] = read;
    }
    v
}

fn issues(query: &HistorySearch, items: &[Value], paging: &Paging, terms: String) -> Value {
    let by_update = gh_search_history_query::sort(query).as_deref() == Some("updated");
    let fields = gh_search_history_query::fields(query);
    let state = query.state();
    let issues = items
        .iter()
        .map(|item| {
            let mut row = rows::map_issue(item.clone(), by_update);
            rows::drop_implied(&mut row, state.as_deref(), true, &fields);
            row
        })
        .collect::<Vec<_>>();
    // The rows' repository is the query's own owner/repo: not echoed.
    let mut v = json!({"issues":issues,
        "effectiveQuery":terms,"pagination":paging.pagination()});
    pick_rows(&mut v, "issues", &["number"], &fields);
    if let Some(total) = paging.reported_total() {
        v["pagination"]["totalItems"] = json!(total);
    }
    // List mode runs the REST endpoint, not the search terms.
    if paging.listed
        && let Some(map) = v.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    if let Some(read) = leads::read_issue(query, items) {
        v["next"]["readIssue"] = read;
    }
    v
}

fn commits(query: &HistorySearch, items: Vec<Value>, paging: &Paging) -> Value {
    let fields = gh_search_history_query::fields(query);
    if gh_search_history_query::keywords(query).is_empty() {
        let commits = items
            .into_iter()
            .map(rows::map_commit_list)
            .collect::<Vec<_>>();
        let mut v = json!({"commits":commits});
        pick_rows(&mut v, "commits", &["commitSha"], &fields);
        // `shape` adds the reachable `next.nextPage`. A complete list says
        // so too (`hasMore:false`): the REST list has no total, so a missing
        // block would leave the end of the history unstated.
        v["pagination"] =
            crate::response::pages::PageFacts::open(paging.page, Some(paging.per), paging.more)
                .out_of_range(paging.past_end)
                .to_value();
        return v;
    }
    let commits = items.into_iter().map(rows::map_commit).collect::<Vec<_>>();
    let mut facts =
        crate::response::pages::PageFacts::open(paging.page, Some(paging.per), paging.more)
            .out_of_range(paging.past_end);
    if let Some(total) = paging.reported_total() {
        facts = facts.with_items(total);
    }
    let mut v = json!({"commits":commits,"pagination":facts.to_value()});
    pick_rows(&mut v, "commits", &["commitSha"], &fields);
    v
}

/// A complete page with no rows is empty, so the shared fallback hint fires.
pub(super) fn mark_empty(value: &mut Value, more: bool) {
    let rows = ["pullRequests", "issues", "commits"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array));
    if !more && rows.is_some_and(Vec::is_empty) {
        value["status"] = json!("empty");
    }
}

/// Provider-side incompleteness, disclosed by the one rule every GitHub
/// search shares ([`apply_partial`]); a capped search also gets its
/// narrowing lead (`narrowScope`). A REST list has no window and no
/// incomplete flag, so it states neither.
fn disclose_provider_limits(
    value: &mut Value,
    query: &HistorySearch,
    paging: &Paging,
    narrow: Option<Value>,
) {
    let subject = match gh_search_history_query::operation(query) {
        HistoryOperation::PullRequest => "pull request",
        HistoryOperation::Issue => "issue",
        HistoryOperation::Commit => "commit",
    };
    apply_partial(
        value,
        &**query,
        &SearchCompleteness {
            tool: ToolId::GhSearchHistory,
            matched: if paging.capped {
                paging.matched
            } else {
                paging.total
            },
            reachable: paging.total,
            incomplete: paging.incomplete,
            page: paging.page,
            has_more: paging.more,
            subject,
        },
    );
    if paging.capped
        && let Some(lead) = narrow
    {
        value["next"]["narrowScope"] = lead;
    }
}
