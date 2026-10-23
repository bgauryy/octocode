//! Patch pages: changed files packed into one patch char window, the
//! page-stream cursor that continues it, and the file rows it shows.
use super::super::HistoryItemRequest;
use super::super::inventory::*;
use super::super::text::needle;
use super::super::util::str_at;
use super::super::window::{literal_patch_window, patch_window};
use super::hunks::history_patch_view;
use crate::response::pages::SpanFacts;
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use serde_json::{Value, json};

mod stream;

use stream::{PatchStream, window_end};

/// A page of changed files sharing one patch char window.
pub(in crate::tools::gh_get_history_item) struct PatchPage {
    pub(in crate::tools::gh_get_history_item) rows: Vec<Value>,
    /// Files whose patch is not fully delivered yet, in page order.
    pub(in crate::tools::gh_get_history_item) unfinished: Vec<String>,
    /// The page-stream `offset` that continues this window; `None` once
    /// every patch on the page is delivered.
    pub(in crate::tools::gh_get_history_item) cursor: Option<usize>,
    /// This window over the page's whole patch stream, in chars.
    pub(in crate::tools::gh_get_history_item) stream: SpanFacts,
    /// Files this window starts whose `matchString` context the hunks cut
    /// short (no head text), with the new-side windows they wanted.
    pub(in crate::tools::gh_get_history_item) clipped: Vec<(String, Vec<String>)>,
}

/// Shape a page of changed files. Patches are packed whole, in file order,
/// into one char window over the page's concatenated patch stream: a window
/// ends at the last file it holds whole, so small patches arrive complete;
/// a file no window holds whole is cut, at the last line end the window
/// holds. `offset` is the stream position, so the next window repeats the
/// same file page and completed files are not re-emitted. The first window
/// also lists binary and empty files (metadata only).
pub(in crate::tools::gh_get_history_item) fn shape_patch_page(
    files: Vec<Value>,
    query: &HistoryItemRequest,
) -> PatchPage {
    shape_patch_page_reserving(files, query, 0)
}

/// [`shape_patch_page`] with `reserve` chars of the window left to other
/// evidence the same row carries (a pull request's `fileSummary`), so the
/// row still fits one response page. The window keeps at least a quarter
/// of itself, so a walk always moves.
pub(in crate::tools::gh_get_history_item) fn shape_patch_page_reserving(
    files: Vec<Value>,
    query: &HistoryItemRequest,
    reserve: usize,
) -> PatchPage {
    let PatchViews {
        views,
        narrowed,
        hits,
        clipped,
        same_as,
    } = PatchViews::of(&files, query);
    // A patch identical to an earlier one of the page is delivered once:
    // the repeat takes no stream chars and names the first.
    let lengths = views
        .iter()
        .zip(&same_as)
        .map(|(view, same)| match same {
            Some(_) => 0,
            None => view.as_deref().map_or(0, |v| v.chars().count()),
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<usize>();
    let offset = gh_get_history_item_query::char_offset(query)
        .unwrap_or(0)
        .min(total);
    let window = match (needle(query), gh_get_history_item_query::char_length(query)) {
        // A literal search returns many short hit runs: their row headers
        // are small, so the hits may fill the page share without the fixed
        // metadata reserve.
        (Some(_), None) => literal_patch_window(query.auto_page_chars),
        _ => patch_window(
            gh_get_history_item_query::char_length(query),
            query.auto_page_chars,
        ),
    };
    let window = window.saturating_sub(reserve).max(window / 4).max(1);
    // The first window also lists every patch-less file of the page as a
    // metadata row on the same response page: a window that would push the
    // row past the page's patch share shrinks by those rows (an explicit
    // smaller `length` already fits). The quarter floor keeps a walk moving.
    let window = if offset == 0 {
        let share = match needle(query) {
            Some(_) => literal_patch_window(query.auto_page_chars),
            None => patch_window(None, query.auto_page_chars),
        };
        let used = reserve + patchless_rows_chars(&files, &views, &lengths);
        window.min(share.saturating_sub(used).max(share / 4)).max(1)
    } else {
        window
    };
    // Stream start of every file.
    let starts = lengths
        .iter()
        .scan(0usize, |acc, len| {
            let start = *acc;
            *acc += len;
            Some(start)
        })
        .collect::<Vec<_>>();
    let end = window_end(&views, &starts, &lengths, offset, window);
    // A file whose patch runs past the window's end leaves the stream to
    // continue there.
    let stream_window = SpanFacts::window("chars", offset, end.saturating_sub(offset), total);
    let cursor = stream_window.next_offset();
    let stream = PatchStream {
        views: &views,
        same_as: &same_as,
        narrowed: &narrowed,
        hits: &hits,
        clipped: &clipped,
        starts: &starts,
        lengths: &lengths,
        offset,
        end,
    };
    let (rows, unfinished, clipped) = stream.rows(&files);
    PatchPage {
        rows,
        unfinished,
        cursor,
        stream: stream_window,
        clipped,
    }
}

/// The patch view of every file of a page (`None`: GitHub sent no patch),
/// with what a `matchString` narrowing left of each.
struct PatchViews {
    views: Vec<Option<String>>,
    /// Whole-patch sizes of views narrowed to hit hunks: the row marker
    /// selects the lossless re-read (`next.readFullPatches`).
    narrowed: Vec<Option<usize>>,
    /// Hit-line starts of each narrowed view.
    hits: Vec<Option<Vec<usize>>>,
    /// New-side windows each view's context could not reach.
    clipped: Vec<Vec<String>>,
    /// The earlier file of the page whose view is identical (a README
    /// copied to four packages), by index: the repeat references it.
    same_as: Vec<Option<usize>>,
}

impl PatchViews {
    fn of(files: &[Value], query: &HistoryItemRequest) -> Self {
        let mut out = Self {
            views: Vec::with_capacity(files.len()),
            narrowed: Vec::with_capacity(files.len()),
            hits: Vec::with_capacity(files.len()),
            clipped: Vec::with_capacity(files.len()),
            same_as: Vec::with_capacity(files.len()),
        };
        for file in files {
            let patch = str_at(file, "/patch");
            let view = patch.map(|patch| {
                history_patch_view(patch, str_at(file, "/filename").unwrap_or(""), query)
            });
            match (patch, view) {
                (Some(patch), Some(view)) => {
                    out.narrowed
                        .push(view.narrowed.then(|| patch.chars().count()));
                    out.hits.push(view.narrowed.then_some(view.hits));
                    out.clipped.push(view.clipped);
                    out.views.push(Some(view.text));
                }
                _ => {
                    out.narrowed.push(None);
                    out.hits.push(None);
                    out.clipped.push(Vec::new());
                    out.views.push(None);
                }
            }
        }
        let mut first = std::collections::HashMap::<&str, usize>::new();
        for (index, view) in out.views.iter().enumerate() {
            let same = view
                .as_deref()
                .filter(|view| !view.is_empty())
                .and_then(|view| match first.entry(view) {
                    std::collections::hash_map::Entry::Occupied(earlier) => Some(*earlier.get()),
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(index);
                        None
                    }
                });
            out.same_as.push(same);
        }
        out
    }
}

/// Serialized chars of the metadata rows a first window shows for files
/// without patch text (no provider patch, or an empty one), in the compact
/// header form the response carries.
fn patchless_rows_chars(files: &[Value], views: &[Option<String>], lengths: &[usize]) -> usize {
    files
        .iter()
        .zip(views.iter().zip(lengths))
        .filter(|(_, (view, len))| view.is_none() || **len == 0)
        .map(|(file, (view, _))| {
            let mut row = file_metadata(file);
            match (view, missing_patch_reason(file)) {
                (None, Some(reason)) => {
                    row["isPartial"] = json!(true);
                    row["terminalLimit"] = json!(true);
                    row["patchUnavailable"] = json!(reason);
                }
                _ => row["patch"] = json!(""),
            }
            remove_nulls(&mut row);
            compact_file_header(&mut row);
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1
        })
        .sum()
}

impl PatchPage {
    /// The page with each row in the compact header form the response
    /// carries: every file page (commit, comparison, pull request) ships
    /// its patch rows this way.
    pub(in crate::tools::gh_get_history_item) fn compact(mut self) -> Self {
        for row in &mut self.rows {
            compact_file_header(row);
        }
        self
    }
}
