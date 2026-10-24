//! The changed-file section of a pull-request read: its page, totals, and
//! the cross-tool reads (merge commit, both diff sides) it offers.
use super::PrPlan;
use crate::num::usize_of;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::inventory::shape_pr_files;
use crate::tools::gh_get_history_item::pr_menu;
use crate::tools::gh_get_history_item::sections::Section;
use crate::tools::gh_get_history_item::util::str_at;
use crate::tools::gh_get_history_item::window::{Loaded, reconcile_file_totals};
use crate::tools::gh_get_history_item::{HistoryItemRequest, Sections};
use serde_json::{Map, Value};

/// What the changed-file section of a read found.
#[derive(Default)]
pub(in crate::tools::gh_get_history_item) struct FileOutcome {
    pub(super) review: Vec<String>,
    pub(in crate::tools::gh_get_history_item) unsearched: Vec<String>,
    /// The first unsearched path and the commit holding its text (the
    /// base for a removed file, else the head).
    pub(in crate::tools::gh_get_history_item) first_unsearched: Option<(String, Option<String>)>,
    /// `(dropped, listed)` changed files of the `matchString` filter.
    pub(in crate::tools::gh_get_history_item) match_dropped: Option<(usize, usize)>,
    pub(in crate::tools::gh_get_history_item) scope_unmatched: bool,
    /// Changed files the read's `include`/`status`/`minChanges` scope
    /// kept (every listed file without one), when the scope or literal
    /// matched nothing.
    pub(in crate::tools::gh_get_history_item) in_scope: usize,
    pub(in crate::tools::gh_get_history_item) merge_read: Option<Value>,
    /// `readAtCommit`/`readParent`: the top file at the head (when no merge
    /// read covers it) and at the base.
    pub(in crate::tools::gh_get_history_item) side_reads: Vec<(&'static str, Value)>,
    /// Files whose `matchString` context the hunks cut short.
    pub(in crate::tools::gh_get_history_item) clipped: Vec<(String, Vec<(usize, usize)>)>,
    /// Files of the patch window not fully delivered.
    pub(super) unfinished_patches: usize,
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
    // A cross-tool read: one check at the merge commit, on the first page,
    // over every loaded file (a later patch window does not repeat it, and
    // the shown window may hold no code file).
    let merge_read = (!gh_get_history_item_query::later_page(query))
        .then(|| {
            pr_menu::read_at_merge(query, raw, &loaded.items, |file| file_filter.matches(file))
        })
        .flatten();
    // Both sides of the numbered diff for its top file: the head (a merged
    // PR's merge read stands for its new side) and the base, whose old-side
    // numbers match while the base has not moved past the diff's merge base.
    // A file GitHub sent without a patch comes first: no merge read covers
    // it, so its head read stays.
    let side_reads = if gh_get_history_item_query::later_page(query) || !wants.has(Section::Patches)
    {
        Vec::new()
    } else {
        let unpatched =
            pr_menu::first_unpatched(&loaded.items, |file| file_filter.matches(file)).is_some();
        pr_menu::change_reads(
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            &loaded.items,
            &pr_menu::ChangeSides {
                new_ref: str_at(raw, "/head/sha").filter(|_| merge_read.is_none() || unpatched),
                old_ref: str_at(raw, "/base/sha").filter(|sha| !sha.is_empty()),
            },
            |file| file_filter.matches(file),
        )
    };
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
    if let Some(page) = pagination.get_mut("files") {
        let changed_files = raw
            .get("changed_files")
            .and_then(Value::as_u64)
            .map(usize_of);
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
        first_unsearched: shaped.first_unsearched.map(|(path, removed)| {
            let side = if removed { "/base/sha" } else { "/head/sha" };
            let sha = str_at(raw, side).filter(|sha| !sha.is_empty());
            (path, sha.map(str::to_owned))
        }),
        match_dropped: shaped.match_dropped,
        scope_unmatched,
        in_scope,
        merge_read,
        side_reads,
        clipped: shaped.clipped,
        unfinished_patches: shaped.unfinished_patches,
    }
}
