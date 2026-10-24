//! The changed-file page a commit and a comparison share: its rows, patch
//! windows, unsearched files, scope warnings and leads.
//! `operation: "commit"` and `operation: "compare"`: commit metadata or a
//! ref comparison, each with one page of changed files.
use super::HistoryItemRequest;
use super::axis::Axis;
use super::filter::{FileScope, FilteredPage};
use super::patch_hop::DiffCursors;
use super::pr_menu::{ChangeSides, change_reads, without_match_string};
use super::remaining::Remainder;
use super::util::str_at;
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Continuation;
use crate::response::result::Remaining;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::ensure_calls;
use serde_json::{Value, json};

/// What a commit's or comparison's file page leaves for its continuations
/// and leads.
pub(super) struct FilePage {
    pub(super) cursors: DiffCursors,
    /// The reads of the page's top file at both sides of the numbered diff.
    pub(super) side_reads: Vec<Lead>,
    /// `withoutMatchString`: the same read without the literal, when a
    /// `matchString` hit none of the listed files it could search.
    pub(super) without_match: Option<Lead>,
}

/// The two commits a commit's or comparison's numbered diff reads at: the
/// commit (or comparison head) for ` `/`+` lines, its first parent (or the
/// merge base) for `-` lines.
#[derive(Clone, Copy)]
pub(super) struct DiffSides<'a> {
    pub(super) new: Option<&'a str>,
    pub(super) old: Option<&'a str>,
}

/// One page of changed files on `out` (`files`, `contentPagination.files`, and the
/// page's warnings), for a commit and a comparison alike. Without the
/// patches section the page lists paths and line stats (no patches) so the
/// agent can pick files before paying for diffs. A `matchString` that hit
/// none of the listed files it could search is an empty answer, never a bare
/// `files: []`: the row says so, and `same_read` (the read that paged here)
/// without the literal is its recovery.
pub(super) fn shape_file_page(
    out: &mut Value,
    filtered: FilteredPage,
    query: &HistoryItemRequest,
    sides: DiffSides<'_>,
    same_read: impl FnOnce() -> Value,
) -> FilePage {
    let FilteredPage {
        files,
        mut page,
        kept,
        unsearched,
        dropped,
        in_scope,
        loaded,
    } = filtered;
    let patches = query.include_diff();
    // A file page's first patch window reads its top file at both sides of
    // the numbered diff (a file the filter kept); the filter's facts are
    // stated once, on the first page.
    let first_window =
        gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0);
    let side_reads = if patches && first_window {
        change_reads(
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            &files,
            &ChangeSides {
                new_ref: sides.new,
                old_ref: sides.old,
            },
            |_| true,
        )
    } else {
        Vec::new()
    };
    let (rows, cursor) = file_rows(files, query);
    let listed = rows.as_array().is_some_and(|rows| !rows.is_empty());
    // Files it could not search may hold the literal: no empty answer then.
    let match_removed_all = patches && in_scope > 0 && unsearched.is_empty() && kept == 0;
    let cursors = DiffCursors::of_file_page(&mut page, cursor, listed);
    out["files"] = rows;
    super::window::lift_page_limits(out, &mut page);
    out["contentPagination"][Axis::Files.key()] = page;
    let mut without_match = None;
    if first_page(query) {
        if !gh_get_history_item_query::include_patterns(query).is_empty()
            && in_scope > 0
            && in_scope < loaded
        {
            push_warnings(out, [super::filter::include_kept_warning(in_scope, loaded)]);
        }
        if match_removed_all && let Some(needle) = gh_get_history_item_query::match_strings(query) {
            push_warnings(
                out,
                [super::filter::match_removed_all_warning(needle, in_scope)],
            );
            out["status"] = json!("empty");
            // A literal alone resolved the sections to patches: without it,
            // the read lists the files.
            let mut same = same_read();
            if !gh_get_history_item_query::has_sections(query) {
                same["sections"] = json!(["files"]);
            }
            without_match = Some(("withoutMatchString", without_match_string(same)));
        } else if dropped > 0 {
            push_warnings(
                out,
                [super::filter::match_dropped_warning(dropped, in_scope)],
            );
        }
        mark_unsearched(out, query, &unsearched, (sides.new, sides.old));
    }
    FilePage {
        cursors,
        side_reads,
        without_match,
    }
}

/// The note of the next changed-file page of a commit or comparison, from
/// its file page's facts ([`Remainder`]).
pub(super) fn file_page_note(out: &Value) -> Vec<Remaining> {
    out.pointer("/next/nextFilePage")
        .and(out.pointer("/contentPagination/files"))
        .and_then(|page| Remainder::default().note(Axis::Files, page))
        .into_iter()
        .collect()
}

/// The rows of a filtered file page and the patch-stream offset that
/// continues them: without patches the pull-request inventory
/// (`M +3 -1 path`, grouped by directory); with patches, one row per file,
/// each narrowed to its `matchString` hits.
fn file_rows(files: Vec<Value>, query: &HistoryItemRequest) -> (Value, Option<usize>) {
    if !query.include_diff() {
        return (
            Value::Array(super::inventory::compact_inventory(&files)),
            None,
        );
    }
    let page = super::patch::page::shape_patch_page(files, query).compact();
    let rows = Value::Array(page.rows);
    (rows, page.cursor)
}

/// The literal a read filters changed files by: its `matchString`, when it
/// reads patches (an inventory lists every file).
pub(super) fn patch_needle(
    query: &HistoryItemRequest,
) -> Option<std::sync::Arc<super::matcher::Matcher>> {
    super::text::needle(query).filter(|_| query.include_diff())
}

/// List the files a `matchString` could not search (`!reason path`, as on
/// a pull request), mark the row partial, and lead to the literal search of
/// the first one's source: at `old` for a removed file, else at `new`.
fn mark_unsearched(
    out: &mut Value,
    query: &HistoryItemRequest,
    unsearched: &[(&'static str, String, bool)],
    (new, old): (Option<&str>, Option<&str>),
) {
    let Some((_, path, removed)) = unsearched.first() else {
        return;
    };
    out["unsearchedFiles"] = json!(
        unsearched
            .iter()
            .map(|(reason, path, _)| format!("!{reason} {path}"))
            .collect::<Vec<_>>()
    );
    let sha = if *removed { old } else { new };
    super::pr_finish::attach_unsearched_read(out, query, path, sha);
}

/// The read of one commit at its default sections.
pub(super) fn read_commit_lead(query: &HistoryItemRequest, sha: &str) -> Value {
    Continuation::related(
        ToolId::GhGetHistoryItem,
        json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"ref":sha}),
    )
    .build()
}

/// Put `leads` ahead of the row's other leads, in order (pages keep their
/// places: the response caps leads only).
pub(super) fn lead_first(out: &mut Value, leads: Vec<(impl Into<String>, Value)>) {
    if leads.is_empty() {
        return;
    }
    ensure_calls(out);
    if let Some(next) = out["calls"].as_object_mut() {
        for (name, lead) in leads.into_iter().rev() {
            next.shift_insert(0, name.into(), lead);
        }
    }
}

/// A named lead: its `next` key and the call.
pub(super) type Lead = (&'static str, Value);

/// Whether the read is the comparison's first page (no later `page`, no
/// patch window past the start): where its static notices are stated.
pub(super) fn first_page(query: &HistoryItemRequest) -> bool {
    gh_get_history_item_query::page(query).unwrap_or(1) <= 1
        && gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0)
}

/// An `include` scope that matched none of the listed changed files:
/// `include matched 0 of N changed files`, with up to
/// [`NEAREST_DIRS`] changed directories, closest to the scope first.
/// `complete` says whether `files` is the whole list (else N is a floor).
pub(super) fn empty_scope_warning(
    files: &[Value],
    scope: Option<&FileScope>,
    query: &HistoryItemRequest,
    complete: bool,
) -> Option<String> {
    let scope = scope?;
    if files.is_empty() || files.iter().any(|file| scope.matches(file)) {
        return None;
    }
    let target = gh_get_history_item_query::include_patterns(query)
        .into_iter()
        .find_map(literal_prefix)
        .unwrap_or_default();
    let mut dirs: Vec<&str> = Vec::new();
    for file in files {
        let name = str_at(file, "/filename").unwrap_or("");
        let dir = name.rsplit_once('/').map_or("", |(dir, _)| dir);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let shared = |dir: &str| {
        dir.split('/')
            .zip(target.split('/'))
            .take_while(|(a, b)| a == b && !a.is_empty())
            .count()
    };
    // Most leading segments shared with the scope first; GitHub's order
    // among equals.
    let mut ranked = dirs.into_iter().enumerate().collect::<Vec<_>>();
    ranked.sort_by_key(|(index, dir)| (std::cmp::Reverse(shared(dir)), *index));
    let nearest = ranked
        .into_iter()
        .take(NEAREST_DIRS)
        .map(|(_, dir)| {
            if dir.is_empty() {
                "(root)".to_owned()
            } else {
                format!("{dir}/")
            }
        })
        .collect::<Vec<_>>();
    let count = if complete {
        files.len().to_string()
    } else {
        format!("at least {}", files.len())
    };
    Some(format!(
        "include matched 0 of {count} changed files; changed directories: {}.",
        nearest.join(", ")
    ))
}

/// The literal path prefix of an `include` pattern: the pattern itself
/// without glob characters, else the directory before its first glob
/// (`packages/react/**` → `packages/react/`); `None` for a bare glob.
pub(super) fn literal_prefix(pattern: &str) -> Option<String> {
    let glob = pattern.find(octocode_engine::search::glob::GLOB_META);
    let prefix = match glob {
        None => pattern,
        Some(at) => pattern[..at].rsplit_once('/').map_or("", |(dir, _)| dir),
    };
    let prefix = prefix.trim_start_matches("./");
    if prefix.is_empty() {
        return None;
    }
    Some(if glob.is_some() {
        format!("{prefix}/")
    } else {
        prefix.to_owned()
    })
}

/// Changed directories an empty-scope warning names at most.
const NEAREST_DIRS: usize = 5;
