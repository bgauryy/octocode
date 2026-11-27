//! Continuations: the read of a likely row, the next page, and the
//! recovery from an empty search.
use super::{GhSearchHistoryQuery, HistorySearch};
use crate::gh_search_history::gh_search_history_query;
use octocode_contract_types::GhGetHistoryItemQuery;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::wire_query;
use octocode_runtime_base::response::result::{Continuation, edited_query, query_row};
use serde_json::{Value, json};

/// This search with `changes` applied ([`edited_query`]); a change the
/// contract rejects yields no lead (debug and test builds panic).
#[track_caller]
fn edited(q: &GhSearchHistoryQuery, changes: &Value) -> Option<GhSearchHistoryQuery> {
    match edited_query(q, changes) {
        Ok(edited) => Some(edited),
        Err(error) => {
            debug_assert!(
                false,
                "an edited history search breaks its contract at {}: {error}",
                std::panic::Location::caller()
            );
            None
        }
    }
}

/// A PR search whose keywords are one bare issue number (`13786`, `#13786`)
/// is an issue → fix-PR hop: the issue read lists the PRs that closed it.
pub(super) fn issue_links_read(q: &GhSearchHistoryQuery) -> Option<Value> {
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    let [keyword] = gh_search_history_query::keywords(q)[..] else {
        return None;
    };
    let number = keyword
        .trim()
        .trim_start_matches('#')
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)?;
    let read = wire_query::<GhGetHistoryItemQuery>(
        json!({"operation":"issue","owner":owner,"repo":repo,"number":number}),
    )?;
    Some(Continuation::related_query(read).build())
}

/// The same query as a wire row without `page`, the base of every
/// same-tool continuation.
fn same_query(q: &GhSearchHistoryQuery) -> Value {
    query_row(q, &["page"])
}

/// The search again without its person qualifiers: the way on when GitHub
/// cannot find a login they name (`None` when the query names none, or
/// when nothing would scope the search without them: no owner, repository,
/// keyword or other qualifier).
pub(super) fn without_users(q: &GhSearchHistoryQuery) -> Option<Value> {
    let mut filters = same_query(q).get_mut("filters")?.take();
    let filters = filters.as_object_mut()?;
    let mut removed = false;
    for key in [
        "author",
        "committer",
        "assignee",
        "mentions",
        "commenter",
        "reviewedBy",
    ] {
        removed |= filters.remove(key).is_some();
    }
    if !removed {
        return None;
    }
    let filters = if filters.is_empty() {
        Value::Null
    } else {
        Value::Object(std::mem::take(filters))
    };
    let search = edited(q, &json!({"page": null, "filters": filters}))?;
    Some(Continuation::related_query(search).build())
}

/// A path history that ends at the commit which renamed the path: the
/// warning names the previous path and the commit, and the same listing under
/// the previous path, from the commit's first parent, is the way on.
pub(super) fn disclose_rename(
    q: &HistorySearch,
    value: &mut Value,
    rename: &super::fetch::RenameEnd,
) {
    let Some(path) = gh_search_history_query::path(q) else {
        return;
    };
    let short = rename.commit.get(..7).unwrap_or(&rename.commit);
    octocode_runtime_base::response::paging::push_warnings(
        value,
        [format!(
            "The history of `{path}` ends at {short}, which renamed it from `{}`; the related search continues the history under that path.",
            rename.previous_path
        )],
    );
    if let Some(parent) = &rename.parent {
        let mut row = same_query(q);
        row["path"] = json!(rename.previous_path);
        row["ref"] = json!(parent);
        if let Some(search) = wire_query::<GhSearchHistoryQuery>(row) {
            // The path's history goes on under its previous name.
            value["calls"]["olderPath"] = Continuation::related_query(search).required().build();
        }
    }
}

/// Result page `page` of the same query.
pub(super) fn next_page(q: &GhSearchHistoryQuery, page: usize) -> Option<Continuation> {
    Some(Continuation::continued_query(edited(
        q,
        &json!({"page": page}),
    )?))
}

/// The hint beside [`broaden_search`]'s variants.
pub(super) const DROP_ONE_FILTER: &str = "No match with these filters; a search without one answers only if its wider scope still fits the question.";

/// The hint of an empty search by `keywords` that no qualifier hint names a
/// fix for: GitHub ANDs the terms.
pub(super) const TERMS_ANDED: &str =
    "No match for these terms (ANDed; a spaced entry is one phrase): try fewer or other words.";

/// An empty search, broadened by its `filters` alone: one variant per
/// filter, each without it, so the variant that returns rows names the
/// filter that emptied the search (a lone filter drops the whole object).
/// `path` and `keywords` stay: without its path a listing no longer answers
/// the path's question, and [`TERMS_ANDED`] covers the terms. `None` when the
/// search sets no filter or no variant passes the contract.
pub(super) fn broaden_search(q: &GhSearchHistoryQuery) -> Option<Value> {
    let row = same_query(q);
    let filters = row.get("filters")?.as_object()?;
    let variants = filters
        .keys()
        .map(|dropped| {
            let mut variant = row.clone();
            let kept = filters
                .iter()
                .filter(|(key, _)| *key != dropped)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<serde_json::Map<_, _>>();
            if kept.is_empty() {
                if let Some(fields) = variant.as_object_mut() {
                    fields.remove("filters");
                }
            } else {
                variant["filters"] = Value::Object(kept);
            }
            variant
        })
        .filter(|variant| {
            octocode_runtime_base::contracts::validate(
                ToolId::GhSearchHistory.as_str(),
                json!({ "queries": [variant] }),
            )
            .is_ok()
        })
        .take(octocode_runtime_base::response::result::rows_per_call(
            ToolId::GhSearchHistory,
        ))
        .collect::<Vec<_>>();
    (!variants.is_empty()).then(|| {
        Continuation::related_input(ToolId::GhSearchHistory, json!({ "queries": variants })).build()
    })
}

/// A capped search's narrowing (`narrowScope`), one call. Rows in a date
/// order continue past GitHub's 1,000-result window from this page's last
/// row: creation order narrows `filters.createdAt`, and update order narrows
/// `filters.updatedAt`. Newest first sets an upper bound; oldest first sets
/// a lower bound. A row at that instant may show again; none is skipped. An
/// order with no date to resume from (best match, comments, reactions), or
/// an update order whose existing filter owns the window, is re-sorted
/// newest first, whose pages end in that continuation. `None` when a
/// `filters.createdAt` owns a creation-ordered window or no row is dated.
pub(super) fn narrow_capped(q: &HistorySearch, items: &[Value]) -> Option<(PastWindow, Value)> {
    let (sort, order) = super::query::search_order(q);
    let newest_first = order.as_deref() != Some("asc");
    let last = |pointers: &[&str]| {
        items.iter().rev().find_map(|item| {
            pointers
                .iter()
                .find_map(|pointer| item.pointer(pointer))
                .and_then(Value::as_str)
                .map(|date| octocode_tools_shared::gh_shared::dates::utc_date(Some(date)))
        })
    };
    let original = same_query(q);
    let mut row = original.clone();
    let mut route = PastWindow::FromLastRow;
    match sort.as_deref() {
        Some("committer-date") => {
            let date = last(&["/commit/committer/date", "/commit/author/date"])?;
            row["filters"]["committedAt"]["to"] = json!(date);
        }
        Some("created" | "updated") => {
            let (field, pointer) = if sort.as_deref() == Some("created") {
                ("createdAt", "/created_at")
            } else {
                ("updatedAt", "/updated_at")
            };
            let date = last(&[pointer])?;
            let bound = if newest_first { "to" } else { "from" };
            row["filters"][field][bound] = json!(date);
        }
        _ => {
            row["sort"] = json!({"by":"createdAt","order":"desc"});
            route = PastWindow::Resorted;
        }
    }
    if row == original {
        return None;
    }
    octocode_runtime_base::contracts::validate(
        ToolId::GhSearchHistory.as_str(),
        json!({ "queries": [row] }),
    )
    .ok()?;
    // The only call that reaches the results past the window: the row
    // needs it.
    Some((
        route,
        Continuation::related_query(wire_query::<GhSearchHistoryQuery>(row)?)
            .required()
            .build(),
    ))
}

/// How a capped search's related call reaches past the reachable window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PastWindow {
    /// The same search bounded at this page's last row's date: it continues
    /// where the page ends (that instant included, so none is skipped).
    FromLastRow,
    /// The search has no date order to continue from: the call re-runs it
    /// newest first, from its first row; that walk's own pages then narrow
    /// by date. It replaces this walk instead of continuing it.
    Resorted,
}

impl PastWindow {
    /// What the row says of the call, in words that match what it runs.
    pub(super) fn warning(self) -> &'static str {
        match self {
            Self::FromLastRow => {
                "Past the reachable window: related[] continues from this page's last row's date (that instant repeats)."
            }
            Self::Resorted => {
                "For every match, run related[] instead of these pages: it re-sorts newest first, and its last reachable page continues by date."
            }
        }
    }

    /// Whether a page states the route. A continuation from the last row
    /// reaches past the window only from the last reachable page: before it,
    /// the page walk reaches the same rows. The re-sorted search is one call
    /// for the whole walk: stated where the walk starts (it saves the walk)
    /// and where it ends (the reader who walked needs it), never between.
    pub(super) fn stated_on(self, page: usize, more: bool) -> bool {
        match self {
            Self::FromLastRow => !more,
            Self::Resorted => page <= 1 || !more,
        }
    }
}
