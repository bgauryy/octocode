//! Changed-file inventory: compact rows, directory summaries, the review
//! pick, and the pull-request file page.
use super::axis::Axis;
use super::filter::*;
use super::patch::page::{shape_patch_page, shape_patch_page_reserving};
use super::text::needle;
use super::util::{str_at, string, usize_at};
use super::window::{WindowState, auto_page, patch_window};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest};
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use octocode_github::MAX_PER_PAGE;
use serde_json::{Map, Value, json};

mod missing_patch;
mod review;

use missing_patch::inventory_patch_flag;
pub(super) use missing_patch::missing_patch_reason;
pub(super) use review::review_selection;

/// Largest patch-free inventory page (contract pullRequest `pageSize` maximum).
pub(super) fn max_inventory_page() -> usize {
    crate::contracts::query_schema_max(
        crate::id::ToolId::GhGetHistoryItem,
        Some("pullRequest"),
        "pageSize",
    )
}

/// Rendered chars budgeted per compact inventory row when sizing the default
/// inventory page to the automatic response page.
pub(super) const INVENTORY_ROW_CHARS: usize = 60;

/// Changed files per page. A patch-free inventory defaults to as many compact
/// rows as fit one automatic response page (100–1000); a page carrying
/// patches defaults to one provider batch.
pub(super) fn file_page_size(query: &HistoryItemRequest, patches: bool) -> usize {
    match (gh_get_history_item_query::page_size(query), patches) {
        // A literal search returns every hit file of the PR on one page:
        // hits are bounded by matches (and the patch window), not by files.
        (None, true) if gh_get_history_item_query::match_string(query).is_some() => {
            super::window::MAX_FILE_BATCHES * MAX_PER_PAGE
        }
        // Patches pack into one char window whatever the page size, so a
        // patch read lists a whole provider batch: one continuation stream
        // walks every patch of a PR of up to that many files.
        (None, true) => MAX_PER_PAGE,
        (Some(_), true) => gh_get_history_item_query::collection_page_size(query),
        (Some(size), false) => size.clamp(1, max_inventory_page()),
        (None, false) => budget_inventory_page(query),
    }
}

/// The patch-free inventory page an omitted `pageSize` fills: as many
/// compact rows as fit this row's share of the response page (100–1000).
fn budget_inventory_page(query: &HistoryItemRequest) -> usize {
    (auto_page(query.auto_page_chars) / INVENTORY_ROW_CHARS)
        .clamp(MAX_PER_PAGE, max_inventory_page())
        .max(DEFAULT_PAGE_SIZE)
}

/// The file page size a file-page continuation must carry: the budget-sized
/// inventory page of a read that omitted `pageSize`. That size follows the
/// row's share of the response budget (batch size, `responseLength`), so a
/// continuation that left it out would re-cut the file list on another grid
/// and skip or repeat files. Every other file page size is the caller's or
/// a fixed batch, which the continuation reproduces as is.
pub(super) fn pinned_file_page_size(query: &HistoryItemRequest, patches: bool) -> Option<usize> {
    (gh_get_history_item_query::page_size(query).is_none() && !patches)
        .then(|| budget_inventory_page(query))
}

/// GitHub file status as one inventory letter (git's diff letters; T is
/// GitHub's `changed`, U its `unchanged`).
pub(super) fn status_code(status: &str) -> &str {
    match status {
        "added" => "A",
        "removed" => "D",
        "modified" => "M",
        "renamed" => "R",
        "copied" => "C",
        "changed" => "T",
        "unchanged" => "U",
        other => other,
    }
}

/// A file row's status letter and change counts: `M +3 -1`. A file GitHub
/// sent with neither patch nor counts (`omitted`) shows its letter only:
/// its 0/0 is not evidence.
pub(super) fn change_counts(file: &Value) -> String {
    let status = status_code(str_at(file, "/status").unwrap_or(""));
    if is_omitted(file) {
        return status.to_owned();
    }
    format!(
        "{status} +{} -{}",
        usize_at(file, "/additions"),
        usize_at(file, "/deletions")
    )
}

/// A file GitHub sent with neither patch nor line counts: a shaped row
/// flagged `patchUnavailable: "omitted"`, or such a REST provider entry
/// (GraphQL file nodes never carry a patch).
fn is_omitted(file: &Value) -> bool {
    str_at(file, "/patchUnavailable") == Some("omitted")
        || (file.get("patch").is_none()
            && file.get("sha").is_some()
            && missing_patch_reason(file) == Some("omitted"))
}

/// One compact inventory row: `M +3 -1 [!reason ]name[ <- full/old/path]`.
pub(super) fn inventory_row(file: &Value, name: &str) -> String {
    let mut row = change_counts(file);
    if let Some(reason) = str_at(file, "/patchUnavailable") {
        row.push_str(" !");
        row.push_str(reason);
    }
    row.push(' ');
    row.push_str(name);
    if let Some(previous) = str_at(file, "/previousFilename") {
        row.push_str(" <- ");
        row.push_str(previous);
    }
    row
}

/// Compact a patch-free page of provider files, for a pull request, a
/// commit, a comparison and a PR commit alike: files of one directory share
/// a `{"dir/": [rows]}` group (see [`group_by_directory`]); every other file
/// is a full-path row. A file GitHub sent without a patch says why
/// (`A +0 -0 !binary logo.png`), so its zero counts never read as an empty
/// change.
pub(super) fn compact_inventory(files: &[Value]) -> Vec<Value> {
    let rows = files.iter().map(metadata_row).collect::<Vec<_>>();
    group_by_directory(&rows, inventory_row)
}

/// A provider file's metadata row (no patch), flagged when GitHub sent no
/// patch ([`inventory_patch_flag`]).
pub(super) fn metadata_row(file: &Value) -> Value {
    let mut row = file_metadata(file);
    inventory_patch_flag(&mut row, file);
    remove_nulls(&mut row);
    row
}

/// One patch-page summary row of a provider file: the inventory row with
/// the patch's hunk count after the change counts
/// (`M +3 -1 2 hunks name[ <- old/path]`), or why GitHub sent no patch
/// (`M +3 -1 !tooLarge name`).
pub(super) fn summary_row(file: &Value, name: &str) -> String {
    let mut row = change_counts(file);
    match str_at(file, "/patch") {
        Some(patch) => {
            let hunks = patch.lines().filter(|line| line.starts_with("@@")).count();
            let unit = if hunks == 1 { "hunk" } else { "hunks" };
            row.push_str(&format!(" {hunks} {unit}"));
        }
        None => {
            if file.get("sha").is_some()
                && let Some(reason) = missing_patch_reason(file)
            {
                row.push_str(" !");
                row.push_str(reason);
            }
        }
    }
    row.push(' ');
    row.push_str(name);
    if let Some(previous) = str_at(file, "/previous_filename") {
        row.push_str(" <- ");
        row.push_str(previous);
    }
    row
}

/// Group a page's rows by directory: a directory with two or more files
/// becomes one `{"dir/": [rows]}` group, where it first appears, whose rows
/// name files relative to it and keep provider order; every other file is a
/// full-path row, in provider order.
pub(super) fn group_by_directory(
    files: &[Value],
    render: fn(&Value, &str) -> String,
) -> Vec<Value> {
    let dir_of = |file: &Value| {
        str_at(file, "/filename")
            .unwrap_or("")
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir)
            .to_owned()
    };
    let dirs: Vec<String> = files.iter().map(dir_of).collect();
    let mut grouped = std::collections::HashMap::<&str, usize>::new();
    for dir in dirs.iter().filter(|dir| !dir.is_empty()) {
        *grouped.entry(dir.as_str()).or_default() += 1;
    }
    let mut emitted = std::collections::HashSet::<&str>::new();
    let mut out = Vec::new();
    for (file, dir) in files.iter().zip(&dirs) {
        let path = str_at(file, "/filename").unwrap_or("");
        if grouped.get(dir.as_str()).is_none_or(|count| *count < 2) {
            out.push(json!(render(file, path)));
        } else if emitted.insert(dir.as_str()) {
            let rows = files
                .iter()
                .zip(&dirs)
                .filter(|(_, other)| *other == dir)
                .map(|(member, _)| {
                    let name = str_at(member, "/filename").unwrap_or("");
                    json!(render(member, &name[dir.len() + 1..]))
                })
                .collect::<Vec<_>>();
            let mut group = Map::new();
            group.insert(format!("{dir}/"), Value::Array(rows));
            out.push(Value::Object(group));
        }
    }
    out
}

/// What shaping a pull request's changed-file page found.
pub(super) struct ShapedFiles {
    /// The page's review pick (`next.readSelectedPatches`).
    pub(super) review: Vec<String>,
    /// In-scope text files a `matchString` could not search because GitHub
    /// sent no patch for them, as compact `!reason path` rows.
    pub(super) unsearched: Vec<String>,
    /// The first of those paths (the source-read template), and whether
    /// the change removed it (its text is at the base).
    pub(super) first_unsearched: Option<(String, bool)>,
    /// `(dropped, listed)`: in-scope searched files a `matchString` dropped
    /// for holding no hit, beside files it kept; `None` when it dropped none
    /// or kept none (an unmatched literal is stated on its own).
    pub(super) match_dropped: Option<(usize, usize)>,
    /// Files whose `matchString` context the hunks cut short, with the
    /// new-side windows to read at the head.
    pub(super) clipped: Vec<(String, Vec<String>)>,
    /// Files of the patch window not fully delivered (a working count, never
    /// an output field).
    pub(super) unfinished_patches: usize,
}

/// Shape a pull request's changed-file page into `row`: compact inventory
/// rows without patches, one object per file with them.
pub(super) fn shape_pr_files(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    files: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    patches: bool,
    scope: Option<&FileScope>,
) -> ShapedFiles {
    let needle = needle(query);
    let FilteredPage {
        files: slice,
        page,
        kept,
        unsearched,
        dropped,
        in_scope,
    } = FileFilter {
        needle: needle.as_deref(),
        scope,
    }
    .page(files, state, query, patches);
    let first_unsearched = unsearched
        .first()
        .map(|(_, path, removed)| (path.clone(), *removed));
    let unsearched = unsearched
        .into_iter()
        .map(|(reason, path, _)| format!("!{reason} {path}"))
        .collect::<Vec<_>>();
    let match_dropped = (needle.is_some()
        && !gh_get_history_item_query::later_page(query)
        && dropped > 0
        && kept > 0)
        .then_some((dropped, in_scope));
    let review = review_selection(&slice, patch_window(None, query.auto_page_chars));
    if !patches {
        let rows = compact_inventory(&slice);
        if !rows.is_empty() {
            row["files"] = Value::Array(rows);
        }
        pagination.insert(Axis::Files.key().into(), page);
        return ShapedFiles {
            review,
            unsearched,
            first_unsearched,
            match_dropped,
            clipped: Vec::new(),
            unfinished_patches: 0,
        };
    }
    // A patch read that does not fit one response lists its page's files
    // first (status, change counts, hunks), then the patches in file order.
    let summary = gh_get_history_item_query::char_offset(query)
        .is_none()
        .then(|| group_by_directory(&slice, summary_row));
    // The summary rides the first window of a page whose patches do not
    // all fit it: its chars come out of that window, so the row (summary
    // plus patches) still fits one response page and never splits into a
    // part without patch rows. A page whose patches fit shows them alone.
    let summary_chars = summary
        .as_ref()
        .filter(|summary| !summary.is_empty())
        .map_or(0, |value| {
            octocode_engine::json_size::utf16_units(value).unwrap_or(0)
        });
    let patches = if summary_chars == 0 {
        shape_patch_page(slice, query)
    } else {
        let whole = shape_patch_page(slice.clone(), query);
        if whole.cursor.is_none() {
            whole
        } else {
            shape_patch_page_reserving(slice, query, summary_chars)
        }
    };
    let patches = patches.compact();
    let shaped = patches.rows;
    if let Some(summary) = summary
        && patches.cursor.is_some()
        && !summary.is_empty()
    {
        row["fileSummary"] = Value::Array(summary);
    }
    if !shaped.is_empty() {
        row["files"] = Value::Array(shaped);
    }
    // One continuation covers the page's patch stream: the same file page
    // at the stream cursor, the window over the page's patch stream.
    if patches.cursor.is_some() {
        pagination.insert(Axis::Patches.key().into(), patches.stream.to_value());
    }
    pagination.insert(Axis::Files.key().into(), page);
    ShapedFiles {
        review,
        unsearched,
        first_unsearched,
        match_dropped,
        clipped: patches.clipped,
        unfinished_patches: patches.unfinished.len(),
    }
}

/// One compact patch-row header: `path` plus the inventory's `M +3 -1`
/// change `stat` (and `previousPath` for a rename), replacing the separate
/// `filename`/`status`/`additions`/`deletions` fields.
pub(super) fn compact_file_header(row: &mut Value) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let count = |fields: &Map<String, Value>, key: &str| {
        fields.get(key).and_then(Value::as_u64).unwrap_or(0)
    };
    let status = status_code(fields.get("status").and_then(Value::as_str).unwrap_or(""));
    // An `omitted` file's 0/0 is not evidence: its letter only.
    let stat = if fields.get("patchUnavailable").and_then(Value::as_str) == Some("omitted") {
        status.to_owned()
    } else {
        format!(
            "{status} +{} -{}",
            count(fields, "additions"),
            count(fields, "deletions")
        )
    };
    let rest = std::mem::take(fields);
    fields.insert(
        "path".into(),
        rest.get("filename").cloned().unwrap_or_default(),
    );
    fields.insert("stat".into(), json!(stat));
    for (key, value) in rest {
        match key.as_str() {
            "filename" | "status" | "additions" | "deletions" => {}
            "previousFilename" => {
                fields.insert("previousPath".into(), value);
            }
            _ => {
                fields.insert(key, value);
            }
        }
    }
}

pub(super) fn file_metadata(file: &Value) -> Value {
    json!({"filename":str_at(file,"/filename").unwrap_or(""),"status":string(file.get("status")),"additions":usize_at(file,"/additions"),"deletions":usize_at(file,"/deletions"),"previousFilename":file.get("previous_filename")})
}
