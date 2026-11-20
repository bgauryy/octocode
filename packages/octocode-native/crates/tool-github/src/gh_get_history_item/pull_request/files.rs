//! The changed-file section of a pull-request read: its page, totals, and
//! source recovery and the optional reads at each side of its diff.
use super::PrPlan;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::inventory::shape_pr_files;
use crate::gh_get_history_item::pr_menu;
use crate::gh_get_history_item::sections::Section;
use crate::gh_get_history_item::util::str_at;
use crate::gh_get_history_item::window::{Loaded, reconcile_file_totals};
use crate::gh_get_history_item::{HistoryItemRequest, Sections};
use octocode_engine::num::saturating_usize;
use serde_json::{Map, Value};

/// What the changed-file section of a read found.
#[derive(Default)]
pub(in crate::gh_get_history_item) struct FileOutcome {
    pub(super) review: Vec<String>,
    pub(in crate::gh_get_history_item) unsearched: Vec<String>,
    /// The first unsearched path and the commit holding its text (the
    /// base for a removed file, else the head).
    pub(in crate::gh_get_history_item) leads_unsearched: Vec<(String, Option<String>)>,
    /// `(dropped, listed)` changed files of the `matchString` filter.
    pub(in crate::gh_get_history_item) match_dropped: Option<(usize, usize)>,
    pub(in crate::gh_get_history_item) scope_unmatched: bool,
    /// Changed files the read's `include`/`status`/`minChanges` scope
    /// kept (every listed file without one), when the scope or literal
    /// matched nothing.
    pub(in crate::gh_get_history_item) in_scope: usize,
    /// `readAtCommit`/`readParent`: the top shown file at the head and base.
    pub(in crate::gh_get_history_item) side_reads: Vec<(String, Value)>,
    /// Files whose `matchString` context the hunks cut short.
    pub(in crate::gh_get_history_item) clipped: Vec<(String, Vec<(usize, usize)>)>,
}

pub(super) fn shape_files_section(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    loaded: Loaded,
    raw: &Value,
    query: &HistoryItemRequest,
    wants: &Sections,
    plan: &PrPlan,
) -> FileOutcome {
    let file_filter = plan.file_filter();
    // The cross-tool reads below are first-page reads over the page's files
    // as loaded: shaping consumes them.
    let lead_files = (!gh_get_history_item_query::later_page(query)).then(|| loaded.items.clone());
    let listed = loaded.state.skipped + loaded.items.len();
    let state = loaded.state;
    // An `include`/`status`/`minChanges` scope that matched no changed file at all.
    let scope_unmatched = (plan.scope.is_some() || plan.needle.is_some())
        && state.exhausted
        && gh_get_history_item_query::page(query).unwrap_or(1) == 1
        && !loaded.items.iter().any(|file| file_filter.matches(file));
    let in_scope = if scope_unmatched {
        loaded
            .items
            .iter()
            .filter(|file| plan.scope.as_ref().is_none_or(|scope| scope.matches(file)))
            .count()
    } else {
        0
    };
    let shaped = shape_pr_files(
        row,
        pagination,
        loaded.items,
        state,
        query,
        wants.has(Section::Patches),
        plan.scope.as_ref(),
    );
    // The diff's side reads name only a file the page shows (the filter keeps
    // more files than one patch window packs): their hunk bounds are the
    // page's numbered lines.
    let shown = |file: &Value| {
        file_filter.matches(file)
            && shaped.shown.as_ref().is_none_or(|paths| {
                paths
                    .iter()
                    .any(|path| Some(path.as_str()) == str_at(file, "/filename"))
            })
    };
    // Source reads use the shown diff's head and base identities. An
    // optional code pick at the merge commit is no requested evidence.
    let side_reads = match lead_files
        .as_deref()
        .filter(|_| wants.has(Section::Patches))
    {
        None => Vec::new(),
        Some(files) => pr_menu::change_reads(
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            files,
            &pr_menu::ChangeSides {
                new_ref: str_at(raw, "/head/sha").filter(|sha| !sha.is_empty()),
                old_ref: str_at(raw, "/base/sha").filter(|sha| !sha.is_empty()),
                needle: plan.needle.as_deref(),
            },
            shown,
        ),
    };
    if let Some(page) = pagination.get_mut("files") {
        let changed_files = raw
            .get("changed_files")
            .and_then(Value::as_u64)
            .map(saturating_usize);
        reconcile_file_totals(
            page,
            state,
            listed,
            changed_files,
            !file_filter.is_trivial(),
        );
    }
    FileOutcome {
        review: shaped.review,
        unsearched: shaped.unsearched,
        leads_unsearched: shaped
            .leads_unsearched
            .into_iter()
            .map(|(path, removed)| {
                let side = if removed { "/base/sha" } else { "/head/sha" };
                let sha = str_at(raw, side).filter(|sha| !sha.is_empty());
                (path, sha.map(str::to_owned))
            })
            .collect(),
        match_dropped: shaped.match_dropped,
        scope_unmatched,
        in_scope,
        side_reads,
        clipped: shaped.clipped,
    }
}
