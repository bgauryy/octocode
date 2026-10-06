//! The response for one page: rows, pagination, leads, warnings and the
//! partial state.
use super::{HistoryOperation, HistorySearch, leads, rows};
use crate::providers::github::HistoryPage;
use crate::tools::gh_shared::SEARCH_RESULT_CAP;
use crate::tools::result::remove_null_fields;
use serde_json::{Value, json};

/// GitHub search stops at 1,000 results; the cap is not the end of history.
const CAP_PARTITION_HINT: &str = "GitHub search returns at most 1,000 results; partition by date (qualifiers created:/merged:/closed: ranges, or since/until for commits) or narrow keywords to reach the rest.";

/// Where a page sits in the result.
pub(super) struct Paging {
    page: usize,
    per: usize,
    /// The page the provider answered (a REST list may differ).
    current: usize,
    more: bool,
    /// The reachable total (search caps it).
    total: usize,
    /// The provider total exceeds the reachable total.
    capped: bool,
    /// A complete one-page REST list counts its rows exactly.
    exact_list_total: Option<usize>,
    listed: bool,
    incomplete: bool,
}

impl Paging {
    pub(super) fn new(result: &HistoryPage, page: usize, per: usize) -> Self {
        let total = if result.listed {
            0
        } else {
            result.total_count.min(SEARCH_RESULT_CAP)
        };
        let current = if result.listed {
            result.provider_page
        } else {
            page
        };
        let more = if result.listed {
            result.has_more
        } else {
            current < total.div_ceil(per).max(1)
        };
        Self {
            page,
            per,
            current,
            more,
            total,
            capped: !result.listed && result.total_count > SEARCH_RESULT_CAP,
            exact_list_total: (result.listed && current == 1 && !more)
                .then_some(result.items.len()),
            listed: result.listed,
            incomplete: result.incomplete_results,
        }
    }
    fn pages(&self) -> usize {
        self.total.div_ceil(self.per).max(1)
    }
    /// The total a page reports: the search total, or an exact list count.
    fn reported_total(&self) -> Option<usize> {
        if self.listed {
            self.exact_list_total
        } else {
            Some(self.total)
        }
    }
    fn pagination(&self, current: usize) -> Value {
        json!({"currentPage":current,"pageSize":self.per,"hasMore":self.more,
            "nextPage":self.more.then_some(current + 1)})
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
    let mut value = match query.operation() {
        HistoryOperation::PullRequest => pull_requests(query, &items, paging, terms),
        HistoryOperation::Issue => issues(query, &items, paging, terms),
        HistoryOperation::Commit => commits(query, items, paging),
    };
    if query.operation() == HistoryOperation::Commit
        && let Some(sha) = value.pointer("/commits/0/sha").and_then(Value::as_str)
        && let Some(read) = leads::read_commit(query, sha)
    {
        value["next"]["readCommit"] = read;
    }
    if !warnings.is_empty() {
        value["warnings"] = json!(warnings);
    }
    if query.operation() == HistoryOperation::Issue
        && !paging.more
        && let Some(map) = value.as_object_mut()
    {
        map.remove("pagination");
    }
    let reachable =
        paging.current < crate::tools::id::query_limits::gh_search_history::PAGE_MAXIMUM;
    if query.operation() == HistoryOperation::Commit && paging.more && reachable {
        value["pagination"]["nextPage"] = json!(paging.current + 1);
    }
    if paging.more && reachable {
        value["next"]["nextPage"] = leads::next_page(query, paging.current, paging.per);
    }
    if paging.more && !reachable {
        value["terminalLimit"] = json!(true);
    }
    remove_null_fields(&mut value);
    mark_empty(&mut value, paging.more);
    if value["status"] == "empty"
        && let Some(lead) = leads::broaden_search(query)
    {
        value["next"]["broadenSearch"] = lead;
    }
    mark_partial(&mut value, paging);
    value
}

fn pull_requests(query: &HistorySearch, items: &[Value], paging: &Paging, terms: String) -> Value {
    // A search beyond one repository names each row's repository: the
    // row's number alone cannot be read.
    let scoped = query.owner().zip(query.repo()).is_some();
    let concise = query.concise() == Some(true);
    let rows = items
        .iter()
        .map(|item| {
            let repository = (!scoped).then(|| rows::item_repo(item)).flatten();
            match (concise, &repository) {
                (true, Some((owner, repo))) => match rows::concise_row(item) {
                    Value::String(text) => json!(format!("{owner}/{repo}{text}")),
                    row => row,
                },
                (true, None) => rows::concise_row(item),
                (false, _) => {
                    let mut row = rows::map_pr(item.clone());
                    if let Some((owner, repo)) = repository {
                        row["repository"] = json!(format!("{owner}/{repo}"));
                    }
                    row
                }
            }
        })
        .collect::<Vec<_>>();
    let mut v = json!({"pullRequests":rows,"effectiveQuery":terms,
        "pagination":paging.pagination(paging.current)});
    if !paging.listed {
        v["pagination"]["totalPages"] = json!(paging.pages());
        v["pagination"]["totalItems"] = json!(paging.total);
        v["pagination"]["totalItemsCapped"] = json!(paging.capped);
    } else if let Some(total) = paging.exact_list_total {
        v["pagination"]["totalItems"] = json!(total);
        v["pagination"]["totalPages"] = json!(1);
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
    let by_update = query.sort().as_deref() == Some("updated");
    let concise = query.concise() == Some(true);
    let issues = items
        .iter()
        .map(|item| {
            if concise {
                rows::concise_row(item)
            } else {
                rows::map_issue(item.clone(), by_update)
            }
        })
        .collect::<Vec<_>>();
    let mut v = json!({"owner":query.owner(),"repo":query.repo(),"issues":issues,
        "effectiveQuery":terms,"pagination":paging.pagination(paging.current)});
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
    if query.keywords().is_empty() {
        let commits = items
            .into_iter()
            .map(rows::map_commit_list)
            .collect::<Vec<_>>();
        let mut v = json!({"owner":query.owner(),"repo":query.repo(),"commits":commits});
        remove_null_fields(&mut v);
        if paging.more {
            v["pagination"] = json!({"currentPage":paging.page,"pageSize":paging.per,
                "hasMore":true,"nextPage":paging.page + 1});
        }
        return v;
    }
    let commits = items.into_iter().map(rows::map_commit).collect::<Vec<_>>();
    let mut v = json!({"owner":query.owner(),"repo":query.repo(),"scope":"defaultBranch",
        "commits":commits,"pagination":{"currentPage":paging.page,"pageSize":paging.per,
        "hasMore":paging.more}});
    if let Some(total) = paging.reported_total() {
        v["pagination"]["totalItems"] = json!(total);
    }
    if !paging.listed {
        v["pagination"]["totalItemsCapped"] = json!(paging.capped);
    }
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

/// A capped or incomplete search is partial; the cap gets a partition tip
/// (success rows keep warnings; hints are reserved for empty/error rows).
fn mark_partial(value: &mut Value, paging: &Paging) {
    if !paging.incomplete && !paging.capped {
        return;
    }
    value["isPartial"] = json!(true);
    if !paging.more {
        value["terminalLimit"] = json!(true);
    }
    value["partialReasons"] = json!([if paging.capped {
        "providerResultCap"
    } else {
        "providerIncompleteResults"
    }]);
    if paging.capped {
        match value.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(json!(CAP_PARTITION_HINT)),
            None => value["warnings"] = json!([CAP_PARTITION_HINT]),
        }
    }
}
