//! The response for one page: rows, pagination, leads, warnings and the
//! partial state.
use super::{HistoryOperation, HistorySearch, leads, rows};
use crate::gh_search_history::gh_search_history_query;
use crate::qualifier_keywords::{Qualifier, qualifier};
use octocode_github::HistoryPage;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::remove_null_fields;
use octocode_tools_shared::gh_shared::commit;
use octocode_tools_shared::gh_shared::{
    SearchCompleteness, SearchWindow, apply_partial, lean_rows, pick_rows,
};
use serde_json::{Value, json};

mod detail;
use detail::{COMMIT_DETAIL, ISSUE_DETAIL, PULL_REQUEST_DETAIL, detail_unless_sorted};

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
    /// Rows this page shows.
    shown: usize,
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
            shown: result.items.len(),
            listed: result.listed,
            incomplete: result.incomplete_results,
            past_end: result.items.is_empty()
                && page > 1
                && (result.listed || reach.past_last(page)),
        }
    }
    /// The page facts: where the page starts and the rows it shows, with
    /// the total when one is known. A search total the provider caps is a
    /// floor (`countScope:"capped"`), one it reported incomplete is
    /// `partial`, and an exact one says `complete`; a REST list with more
    /// pages states no total (the rows listed so far read as the list's
    /// size). The cursor rides
    /// `next.nextPage`, bounded by the contract's `page` maximum.
    fn pagination(&self) -> Value {
        use octocode_runtime_base::response::pages::PageFacts;
        let facts = PageFacts::open(
            PageFacts::start_of(self.page, self.per),
            self.shown,
            self.more,
        )
        .out_of_range(self.past_end);
        let (total, scope) = if !self.listed {
            let scope = if self.capped {
                Some("capped")
            } else if self.incomplete {
                Some("partial")
            } else {
                Some("complete")
            };
            (Some(self.total), scope)
        } else if let Some(total) = self.exact_list_total {
            (Some(total), None)
        } else {
            (None, None)
        };
        let mut block = match total {
            Some(total) => facts.with_total(total),
            None => facts,
        }
        .to_value();
        if let Some(scope) = scope {
            block["countScope"] = json!(scope);
        }
        block
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
    if paging.more
        && let Some(next) = leads::next_page(query, paging.page + 1)
    {
        octocode_runtime_base::response::collection::insert_walk(
            &mut value["calls"],
            octocode_runtime_base::id::ToolId::GhSearchHistory,
            None,
            next,
        );
    }
    remove_null_fields(&mut value);
    mark_empty(&mut value, paging.more);
    // A page past the end is restarted by the shared response stage, not
    // broadened: the search has results.
    if value["status"] == "empty" && !paging.past_end {
        match leads::broaden_search(query) {
            Some(lead) => octocode_tools_shared::leads::broaden::set_broadened(
                &mut value,
                &octocode_runtime_base::response::result::query_row(&**query, &[]),
                lead,
            ),
            None => octocode_tools_shared::leads::broaden::set_no_broader(&mut value),
        }
    }
    // A path scope that listed nothing may name no path at all: its
    // parent's listing checks it.
    if value["status"] == "empty"
        && !paging.past_end
        && let Some(check) = octocode_tools_shared::gh_shared::path_check(
            &octocode_runtime_base::response::result::query_row(&**query, &[]),
        )
    {
        value["calls"]["viewTree"] = check;
    }
    if value["status"] == "empty"
        && !paging.past_end
        && gh_search_history_query::operation(query) == HistoryOperation::Commit
        && only_the_path_narrows(query)
        && let Some(path) = gh_search_history_query::path(query)
    {
        // GitHub lists pushed history only: a file that exists only in the
        // caller's checkout reads the same as a path that never existed.
        let at = gh_search_history_query::reference(query).unwrap_or("the default branch");
        octocode_runtime_base::response::result::add_hints(
            &mut value,
            [format!(
                "No commit touches `{path}` on {at} of GitHub: history covers only what was pushed there, so a file or commit that exists only in your checkout (never pushed, or on an unpushed branch) has none here; read it with local git history, or retry with the ref that holds it. The related[] ghStructure request lists the path's parent to check it exists at that ref."
            )],
        );
    }
    if value["status"] == "empty" && !paging.past_end {
        octocode_runtime_base::response::result::add_hints(
            &mut value,
            qualifier_keyword_hints(&gh_search_history_query::keywords(query)),
        );
    }
    disclose_provider_limits(&mut value, query, paging, narrow);
    // `effectiveQuery` is the request in GitHub's search syntax: it explains
    // an answer the rows do not (none matched, or GitHub capped or cut the
    // search). Beside a whole page of rows it only restates the request.
    if value["status"] != "empty"
        && !paging.capped
        && !paging.incomplete
        && let Some(fields) = value.as_object_mut()
    {
        fields.remove("effectiveQuery");
    }
    value
}

/// The search qualifiers a keyword can spell, with the `filters` field that
/// carries each: a keyword matches text, so `author:ada` finds nothing a
/// filter would.
const QUALIFIER_FILTERS: &[Qualifier] = &[
    qualifier!(["author"] => "filters.author"),
    qualifier!(["committer"] => "filters.committer"),
    qualifier!(["assignee"] => "filters.assignee"),
    qualifier!(["mentions"] => "filters.mentions"),
    qualifier!(["commenter"] => "filters.commenter"),
    qualifier!(["reviewed-by"] => "filters.reviewedBy"),
    qualifier!(["review-requested"] => "filters.reviewRequested"),
    qualifier!(["label"] => "filters.label"),
    qualifier!(["base"] => "filters.base"),
    qualifier!(["head"] => "filters.head"),
];

/// A hint per keyword spelled as a qualifier (`author:sharkdp`) that names
/// the `filters` field carrying it.
pub(super) fn qualifier_keyword_hints(keywords: &[&str]) -> Vec<String> {
    crate::qualifier_keywords::hints(keywords, QUALIFIER_FILTERS)
}

/// Whether a commit listing is narrowed by its path alone: no author,
/// committer or date window. Only then does an empty listing say that no
/// commit touches the path; with any of them, commits touching it may lie
/// outside them, and the broadening hint names each one to drop.
fn only_the_path_narrows(query: &HistorySearch) -> bool {
    query.author().is_none()
        && query.committer().is_none()
        && gh_search_history_query::committed_at(query).is_none()
}

/// Whether pull-request rows keep `createdAt` beside `mergedAt`: when they
/// are sorted by creation, or filtered on it (`since`/`until` set the
/// `created:` window, or a `created:` qualifier).
pub(super) fn keeps_created(query: &HistorySearch) -> bool {
    gh_search_history_query::sort(query).as_deref() == Some("created")
        || query.filters().created.is_some()
}

/// Whether rows carry `key`: the caller picked it in `fields`.
fn carries(fields: &[String], key: &str) -> bool {
    fields.iter().any(|field| field == key)
}

/// Puts each non-empty `text` of an item on its row as `key`: rows and items
/// are one to one, in order.
fn attach_text(
    value: &mut Value,
    rows: &str,
    items: &[Value],
    key: &str,
    text: impl Fn(&Value) -> Option<String>,
) {
    let listed = value.get_mut(rows).and_then(Value::as_array_mut);
    for (row, item) in listed.into_iter().flatten().zip(items) {
        if let (Some(row), Some(text)) = (row.as_object_mut(), text(item)) {
            row.insert(key.to_owned(), json!(text));
        }
    }
}

fn item_body(item: &Value) -> Option<String> {
    item.get("body")
        .and_then(Value::as_str)
        .filter(|body| !body.trim().is_empty())
        .map(str::to_owned)
}

fn commit_body(item: &Value) -> Option<String> {
    commit::message_body(item).map(str::to_owned)
}

fn pull_requests(query: &HistorySearch, items: &[Value], paging: &Paging, terms: String) -> Value {
    let fields = gh_search_history_query::fields(query);
    let state = query.state();
    let keep_created = keeps_created(query);
    let by_update = gh_search_history_query::sort(query).as_deref() == Some("updated");
    let rows = items
        .iter()
        .map(|item| {
            let mut row = rows::map_pr(item.clone());
            if by_update {
                rows::state_updated(&mut row, item);
            }
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
    if carries(&fields, "body") {
        attach_text(&mut v, "pullRequests", items, "body", item_body);
    }
    lean_rows(
        &mut v,
        "pullRequests",
        &detail_unless_sorted(query, PULL_REQUEST_DETAIL),
        &fields,
    );
    pick_rows(
        &mut v,
        "pullRequests",
        &["number", "owner", "repo"],
        &fields,
    );
    rows::drop_filtered_labels(&mut v, "pullRequests", &query.filters().label);
    // List mode runs the REST endpoint, not the search terms.
    if paging.listed
        && let Some(map) = v.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    if let Some(read) = leads::issue_links_read(query) {
        v["calls"]["readIssueLinks"] = read;
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
            rows::name_repository(
                &mut row,
                item,
                gh_search_history_query::owner(query),
                gh_search_history_query::repo(query),
            );
            row
        })
        .collect::<Vec<_>>();
    let mut v = json!({"issues":issues,
        "effectiveQuery":terms,"pagination":paging.pagination()});
    if carries(&fields, "body") {
        attach_text(&mut v, "issues", items, "body", item_body);
    }
    lean_rows(
        &mut v,
        "issues",
        &detail_unless_sorted(query, ISSUE_DETAIL),
        &fields,
    );
    pick_rows(&mut v, "issues", &["number", "owner", "repo"], &fields);
    rows::drop_filtered_labels(&mut v, "issues", &query.filters().label);
    // List mode runs the REST endpoint, not the search terms.
    if paging.listed
        && let Some(map) = v.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    v
}

fn commits(query: &HistorySearch, items: Vec<Value>, paging: &Paging) -> Value {
    let fields = gh_search_history_query::fields(query);
    if gh_search_history_query::keywords(query).is_empty() {
        let commits = items
            .iter()
            .map(|item| {
                let mut row = rows::map_commit_list(item.clone());
                rows::name_repository(
                    &mut row,
                    item,
                    gh_search_history_query::owner(query),
                    gh_search_history_query::repo(query),
                );
                row
            })
            .collect::<Vec<_>>();
        let mut v = json!({"commits":commits});
        shape_commit_rows(&mut v, &items, &fields);
        // `shape` adds the reachable `next.nextPage`. A complete list says
        // so too (`hasMore:false`): the REST list has no total, so a missing
        // block would leave the end of the history unstated.
        // Commit search needs keywords: a commit list offers no count call.
        v["pagination"] = paging.pagination();
        return v;
    }
    let commits = items
        .iter()
        .map(|item| {
            let mut row = commit::row(item);
            rows::name_repository(
                &mut row,
                item,
                gh_search_history_query::owner(query),
                gh_search_history_query::repo(query),
            );
            row
        })
        .collect::<Vec<_>>();
    let mut v = json!({"commits":commits,"pagination":paging.pagination()});
    shape_commit_rows(&mut v, &items, &fields);
    v
}

/// The default row, `fields` pick and judged text of a commit page.
fn shape_commit_rows(v: &mut Value, items: &[Value], fields: &[String]) {
    if carries(fields, "messageBody") {
        attach_text(v, "commits", items, "messageBody", commit_body);
    }
    lean_rows(v, "commits", COMMIT_DETAIL, fields);
    pick_rows(v, "commits", &["commitSha", "owner", "repo"], fields);
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
    narrow: Option<(leads::PastWindow, Value)>,
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
        && let Some((route, lead)) = narrow
        && route.stated_on(paging.page, paging.more)
    {
        value["calls"]["narrowScope"] = lead;
        octocode_runtime_base::response::paging::push_warnings(value, [route.warning().to_owned()]);
    }
}
