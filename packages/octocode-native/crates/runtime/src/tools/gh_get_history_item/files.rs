//! Changed files: selection filters, path scopes, per-file shaping and the
//! shared patch char window.
use super::util::{minified_view, needle, str_at, string, usize_at};
use super::window::WindowState;
use super::{HistoryItemRequest, MAX_COLLECTION_PAGE, default_page_size};
use crate::tools::result::remove_nulls;
use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Changed-file selection shared by the provider scan and output shaping.
#[derive(Clone, Copy)]
pub(super) struct FileFilter<'a> {
    pub(super) selected: &'a [String],
    pub(super) needle: Option<&'a str>,
    pub(super) scope: Option<&'a InventoryFilter>,
}

impl FileFilter<'_> {
    pub(super) fn is_trivial(&self) -> bool {
        self.selected.is_empty() && self.needle.is_none() && self.scope.is_none()
    }
    pub(super) fn matches(&self, file: &Value) -> bool {
        let path = str_at(file, "/filename").unwrap_or("");
        (self.selected.is_empty() || self.selected.iter().any(|selected| selected == path))
            && self.scope.is_none_or(|scope| scope.matches(file))
            && self.needle.is_none_or(|n| {
                path.to_lowercase().contains(n)
                    || str_at(file, "/patch").is_some_and(|v| v.to_lowercase().contains(n))
            })
    }
}

/// One `fileFilter.paths` entry.
enum PathPattern {
    /// A plain path: that file, or every file under that directory.
    Scope(String),
    /// A glob over the whole path, or over the file name when it has no `/`.
    Glob {
        matcher: GlobMatcher,
        file_name: bool,
    },
}

impl PathPattern {
    fn parse(pattern: &str) -> Result<Self, String> {
        if !pattern.contains(['*', '?', '[', '{']) {
            return Ok(Self::Scope(pattern.to_owned()));
        }
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map(|glob| Self::Glob {
                matcher: glob.compile_matcher(),
                file_name: !pattern.contains('/'),
            })
            .map_err(|error| format!("fileFilter.paths: invalid glob {pattern:?}: {error}"))
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Scope(scope) => {
                path == scope
                    || path
                        .strip_prefix(scope.trim_end_matches('/'))
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            Self::Glob { matcher, file_name } => {
                let subject = if *file_name {
                    path.rsplit('/').next().unwrap_or(path)
                } else {
                    path
                };
                matcher.is_match(subject)
            }
        }
    }
}

/// A pull request's `fileFilter`: status, path and change-count narrowing of
/// the changed-file list. Pages and counts then cover matching files only.
pub(super) struct InventoryFilter {
    paths: Vec<PathPattern>,
    status: Vec<String>,
    min_changes: Option<usize>,
}

impl InventoryFilter {
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let Some(filter) = query.file_filter() else {
            return Ok(None);
        };
        let paths = filter
            .paths
            .iter()
            .flat_map(|paths| paths.iter())
            .map(|pattern| PathPattern::parse(pattern))
            .collect::<Result<Vec<_>, _>>()?;
        let status = filter
            .status
            .iter()
            .flat_map(|status| status.iter())
            .map(ToString::to_string)
            .collect();
        let min_changes = filter
            .min_changes
            .map(|n| usize::try_from(n.get()).unwrap_or(usize::MAX));
        Ok(Some(Self {
            paths,
            status,
            min_changes,
        }))
    }

    /// A rename matches by its new or previous path. Files GitHub sent
    /// without line counts (no patch, 0/0) pass `minChanges`: their size is
    /// unknown, not zero.
    fn matches(&self, file: &Value) -> bool {
        let status = str_at(file, "/status").unwrap_or("");
        let path = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename");
        let changes = usize_at(file, "/additions") + usize_at(file, "/deletions");
        let countless = changes == 0 && file.get("patch").is_none();
        (self.status.is_empty() || self.status.iter().any(|s| s == status))
            && (self.paths.is_empty()
                || self.paths.iter().any(|pattern| {
                    pattern.matches(path) || previous.is_some_and(|p| pattern.matches(p))
                }))
            && self
                .min_changes
                .is_none_or(|min| countless || changes >= min)
    }
}

/// Largest patch-free inventory page (contract pullRequest `pageSize` maximum).
pub(super) fn max_inventory_page() -> usize {
    crate::contracts::query_schema_max(
        crate::tools::id::ToolId::GhGetHistoryItem,
        Some("pullRequest"),
        "pageSize",
    )
}
/// Rendered chars budgeted per compact inventory row when sizing the default
/// inventory page to the automatic response page.
const INVENTORY_ROW_CHARS: usize = 60;

/// Changed files per page. A patch-free inventory defaults to as many compact
/// rows as fit one automatic response page (100–1000); a page carrying
/// patches keeps provider-sized pages.
pub(super) fn file_page_size(query: &HistoryItemRequest, patches: bool) -> usize {
    match (query.page_size(), patches) {
        // A literal search returns every hit file of the PR on one page:
        // hits are bounded by matches (and the patch window), not by files.
        (None, true) if query.match_string().is_some() => {
            super::window::MAX_FILE_BATCHES * MAX_COLLECTION_PAGE
        }
        (_, true) => query.collection_page_size(),
        (Some(size), false) => size.clamp(1, max_inventory_page()),
        (None, false) => (auto_page(query.auto_page_chars) / INVENTORY_ROW_CHARS)
            .clamp(MAX_COLLECTION_PAGE, max_inventory_page())
            .max(default_page_size()),
    }
}

/// GitHub file status as one inventory letter (git's diff letters; T is
/// GitHub's `changed`, U its `unchanged`).
fn status_code(status: &str) -> &str {
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

/// One compact inventory row: `M +3 -1 [!reason ]name[ <- full/old/path]`.
fn inventory_row(file: &Value, name: &str) -> String {
    let mut row = format!(
        "{} +{} -{}",
        status_code(str_at(file, "/status").unwrap_or("")),
        usize_at(file, "/additions"),
        usize_at(file, "/deletions")
    );
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

/// Compact a patch-free file page, in provider order: a run of two or more
/// consecutive files in one directory becomes `{"dir/": [rows]}` whose rows
/// name files relative to it; every other file is a full-path row.
fn compact_inventory(files: &[Value]) -> Vec<Value> {
    let dir_of = |file: &Value| {
        str_at(file, "/filename")
            .unwrap_or("")
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir)
            .to_owned()
    };
    let mut out = Vec::new();
    let mut start = 0;
    while start < files.len() {
        let dir = dir_of(&files[start]);
        let end = start
            + files[start..]
                .iter()
                .take_while(|file| dir_of(file) == dir)
                .count();
        let run = &files[start..end];
        if run.len() >= 2 && !dir.is_empty() {
            let rows = run
                .iter()
                .map(|file| {
                    let path = str_at(file, "/filename").unwrap_or("");
                    json!(inventory_row(file, &path[dir.len() + 1..]))
                })
                .collect::<Vec<_>>();
            let mut group = Map::new();
            group.insert(format!("{dir}/"), Value::Array(rows));
            out.push(Value::Object(group));
        } else {
            out.extend(
                run.iter().map(|file| {
                    json!(inventory_row(file, str_at(file, "/filename").unwrap_or("")))
                }),
            );
        }
        start = end;
    }
    out
}

type PatchRanges = HashMap<String, (Option<Vec<i64>>, Option<Vec<i64>>)>;

/// Selected file names (files plus range targets) and per-file line ranges.
pub(super) fn patch_selection(selector: Option<&Map<String, Value>>) -> (Vec<String>, PatchRanges) {
    let mut selected_names = selector
        .and_then(|v| v.get("files"))
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let lines = |range: &Value, key: &str| {
        range
            .get(key)
            .and_then(Value::as_array)
            .map(|lines| lines.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
    };
    let ranges = selector
        .and_then(|v| v.get("ranges"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|range| {
            let file = range.get("file")?.as_str()?.to_owned();
            Some((file, (lines(range, "additions"), lines(range, "deletions"))))
        })
        .collect::<HashMap<_, _>>();
    for file in ranges.keys() {
        if !selected_names.contains(file) {
            selected_names.push(file.clone());
        }
    }
    (selected_names, ranges)
}

/// What shaping a pull request's changed-file page found.
pub(super) struct ShapedFiles {
    /// A selected path matched no changed file and the provider has no more.
    pub(super) no_selected_match: bool,
    /// The page's review pick (`next.reviewPatches`).
    pub(super) review: Vec<String>,
    /// In-scope text files a `matchString` could not search because GitHub
    /// sent no patch for them, as compact `!reason path` rows.
    pub(super) unsearched: Vec<String>,
    /// The first of those paths (the source-read template).
    pub(super) first_unsearched: Option<String>,
}

/// Review priority of a changed path: source first, then tests, then other
/// files; docs, changesets, lockfiles, and generated output last.
fn review_tier(path: &str) -> u8 {
    use crate::content::{FileType, classify_file_type, is_test_path};
    let lower = path.to_ascii_lowercase();
    let generated = lower.starts_with(".changeset/")
        || lower.contains("/.changeset/")
        || lower.contains("generated")
        || lower.contains(".min.")
        || lower.ends_with(".snap")
        || lower.contains("__snapshots__/");
    match classify_file_type(path) {
        _ if generated => 3,
        Some(FileType::Doc | FileType::Lock) => 3,
        Some(FileType::Code) if is_test_path(path) => 1,
        Some(FileType::Code) => 0,
        _ => 2,
    }
}

/// Estimated rendered patch chars per changed line (diff marker, text,
/// newline), and per file header, for packing a review into one budget.
const REVIEW_LINE_CHARS: usize = 45;
const REVIEW_FILE_CHARS: usize = 80;
/// Files one review read names at most: one provider-sized patch page.
const MAX_REVIEW_FILES: usize = 30;

/// `next.reviewPatches`: the files whose patches answer "what changed" —
/// source files first (then other non-test, non-doc files), most changed
/// lines first — packed into one call's patch budget. Tests, docs,
/// changesets, lockfiles and generated output are left to the inventory.
/// Always names at least the top file when one qualifies.
pub(super) fn review_selection(files: &[Value], budget: usize) -> Vec<String> {
    let mut ranked = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let path = str_at(file, "/filename")?;
            let binary = path.rsplit_once('.').is_some_and(|(_, ext)| {
                BINARY_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
            });
            let tier = if binary { 3 } else { review_tier(path) };
            let changed = usize_at(file, "/additions") + usize_at(file, "/deletions");
            (tier == 0 || tier == 2).then_some((
                (tier, std::cmp::Reverse(changed), index),
                path,
                changed,
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(rank, _, _)| *rank);
    // Files that fit one call's budget come first (packed, most changed
    // first), so the first read delivers whole patches; the rest follow in
    // rank order and arrive through `continuePatch`.
    let mut used = 0usize;
    let mut packed = Vec::new();
    let mut rest = Vec::new();
    for (_, path, changed) in ranked {
        let cost = changed.saturating_mul(REVIEW_LINE_CHARS) + REVIEW_FILE_CHARS;
        if used + cost <= budget {
            used += cost;
            packed.push(path.to_owned());
        } else {
            rest.push(path.to_owned());
        }
    }
    packed.extend(rest);
    packed.truncate(MAX_REVIEW_FILES);
    packed
}

/// Shape a pull request's changed-file page into `row`: compact inventory
/// rows without patches, one object per file with them.
pub(super) fn shape_pr_files(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    files: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    selector: Option<&Map<String, Value>>,
    patch_mode: &str,
    scope: Option<&InventoryFilter>,
) -> ShapedFiles {
    let (selected_names, ranges) = patch_selection(selector);
    let selection_requested = patch_mode == "selected" && !selected_names.is_empty();
    let selected_path_matched = !selection_requested
        || files.iter().any(|file| {
            let path = str_at(file, "/filename").unwrap_or("");
            selected_names.iter().any(|selected| selected == path)
        });
    let needle = needle(query);
    let filter = FileFilter {
        selected: &selected_names,
        needle: needle.as_deref(),
        scope,
    };
    let unsearched = match needle.as_deref() {
        Some(needle) if patch_mode != "none" => unsearched_files(&files, &filter, needle),
        _ => Vec::new(),
    };
    let first_unsearched = unsearched.first().map(|(_, path)| path.clone());
    let unsearched = unsearched
        .into_iter()
        .map(|(reason, path)| format!("!{reason} {path}"))
        .collect::<Vec<_>>();
    let filtered = files
        .into_iter()
        .filter(|file| filter.matches(file))
        .collect::<Vec<_>>();
    let per_page = file_page_size(query, patch_mode != "none");
    let (slice, page) = state.paginate(filtered, query.file_page(), Some(per_page));
    let review = review_selection(&slice, patch_window(None, query.auto_page_chars, 1));
    let slice = slice
        .into_iter()
        .map(|mut file| {
            if let Some((additions, deletions)) =
                str_at(&file, "/filename").and_then(|path| ranges.get(path))
            {
                let filtered_patch = octocode_engine::portable::filter_patch(
                    str_at(&file, "/patch").unwrap_or(""),
                    Some(octocode_engine::types::FilterPatchOptions {
                        additions: additions.clone(),
                        deletions: deletions.clone(),
                        ..Default::default()
                    }),
                );
                file["patch"] = Value::String(filtered_patch);
            }
            file
        })
        .collect::<Vec<_>>();
    // The patch continuation narrows the selection to the unfinished files,
    // so its cursor is relative to the first of them.
    let patches = shape_patch_page(
        slice,
        patch_mode != "none",
        query,
        PatchCursor::FirstUnfinished,
    );
    if patch_mode == "none" {
        let rows = compact_inventory(&patches.rows);
        if !rows.is_empty() {
            row["changedFiles"] = Value::Array(rows);
        }
        pagination.insert("changedFiles".into(), page);
        return ShapedFiles {
            no_selected_match: false,
            review,
            unsearched,
            first_unsearched,
        };
    }
    let shaped = patches
        .rows
        .into_iter()
        .map(|mut shaped| {
            compact_file_header(&mut shaped);
            shaped
        })
        .collect::<Vec<_>>();
    if !shaped.is_empty() {
        row["changedFiles"] = Value::Array(shaped);
    }
    // One continuation covers the page's patch stream; it lists every
    // unfinished file (the cut one plus those not yet started).
    if patch_mode != "none"
        && let Some(cursor) = patches.cursor
    {
        // The cut file's own window when it started on this page; else
        // the next file starts the continuation stream.
        let mut patch_page = shaped_cursor(row.get("changedFiles"))
            .unwrap_or_else(|| json!({"hasMore":true,"nextCharOffset":cursor}));
        patch_page["files"] = json!(patches.unfinished);
        pagination.insert("patches".into(), patch_page);
    }
    pagination.insert("changedFiles".into(), page);
    ShapedFiles {
        no_selected_match: selection_requested && !selected_path_matched && state.exhausted,
        review,
        unsearched,
        first_unsearched,
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
    let stat = format!(
        "{} +{} -{}",
        status_code(fields.get("status").and_then(Value::as_str).unwrap_or("")),
        count(fields, "additions"),
        count(fields, "deletions")
    );
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

/// Scanned files a patch search skipped: selected and in scope, but GitHub
/// sent no patch (too large, or omitted past the diff budget) and the path
/// itself does not match. Binary files hold no text to search. Only REST
/// entries (with a blob `sha`) say anything about patches.
fn unsearched_files(
    files: &[Value],
    filter: &FileFilter<'_>,
    needle: &str,
) -> Vec<(&'static str, String)> {
    let scope_only = FileFilter {
        needle: None,
        ..*filter
    };
    files
        .iter()
        .filter(|file| {
            file.get("patch").is_none() && file.get("sha").is_some() && scope_only.matches(file)
        })
        .filter_map(|file| {
            let reason = missing_patch_reason(file).filter(|r| *r != "binary")?;
            let path = str_at(file, "/filename")?;
            (!path.to_lowercase().contains(needle)).then(|| (reason, path.to_owned()))
        })
        .collect()
}

fn history_patch_view(value: &str, query: &HistoryItemRequest) -> String {
    if let Some(needle) = needle(query)
        && let Some(hunks) = matching_hunks(
            value,
            &needle,
            query.match_context().unwrap_or(MATCH_CONTEXT_LINES),
        )
    {
        return hunks;
    }
    if minified_view(query) {
        octocode_engine::portable::filter_patch(
            value,
            Some(octocode_engine::types::FilterPatchOptions {
                trim_context: Some(true),
                context_lines: Some(2),
                ..Default::default()
            }),
        )
    } else {
        value.to_owned()
    }
}

/// Diff lines kept around each `matchString` hit when `matchContext` is
/// omitted: only the hit lines (context was over half the bytes of a literal
/// search); `next.readFullPatches` re-reads the whole patches.
const MATCH_CONTEXT_LINES: usize = 0;
/// A `matchString` view clips diff lines longer than this (generated or
/// minified text) to the characters around each hit.
const MATCH_LINE_CHARS: usize = 400;
/// Characters kept on each side of a hit inside a clipped line, and at the
/// start of a clipped line without one.
const MATCH_LINE_SIDE: usize = 150;

/// Clip a long diff line for a `matchString` view: keep the diff marker, the
/// text around each hit, and its line ending; each cut becomes
/// `[… N chars …]`. Lines up to [`MATCH_LINE_CHARS`] stay verbatim.
fn clip_line<'a>(text: &'a str, needle: &str) -> std::borrow::Cow<'a, str> {
    if text.chars().count() <= MATCH_LINE_CHARS {
        return text.into();
    }
    let lower = text.to_lowercase();
    let body_end = text.trim_end_matches(['\r', '\n']).len();
    let floor = |mut at: usize| {
        at = at.min(body_end);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let ceil = |mut at: usize| {
        at = at.min(body_end);
        while !text.is_char_boundary(at) {
            at += 1;
        }
        at
    };
    // Byte offsets carry over only when lowercasing kept every length.
    let hits = if lower.len() == text.len() && !needle.is_empty() {
        lower
            .match_indices(needle)
            .map(|(at, _)| (at, at + needle.len()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut keep: Vec<(usize, usize)> = vec![(0, ceil(1))];
    if hits.is_empty() {
        keep.push((0, ceil(MATCH_LINE_SIDE)));
    }
    for (start, end) in hits {
        let span = (
            floor(start.saturating_sub(MATCH_LINE_SIDE)),
            ceil(end + MATCH_LINE_SIDE),
        );
        match keep.last_mut() {
            Some(last) if span.0 <= last.1 => last.1 = last.1.max(span.1),
            _ => keep.push(span),
        }
    }
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in keep {
        let start = start.max(at);
        if start > at {
            out.push_str(&format!("[… {} chars …]", text[at..start].chars().count()));
        }
        out.push_str(&text[start..end.max(start)]);
        at = end.max(start);
    }
    if at < body_end {
        out.push_str(&format!(
            "[… {} chars …]",
            text[at..body_end].chars().count()
        ));
    }
    out.push_str(&text[body_end..]);
    out.into()
}

/// One diff body line: its text (with its line ending) and the old/new line
/// numbers it occupies (`None` on the side it is absent from).
struct DiffLine<'a> {
    text: &'a str,
    old: Option<usize>,
    new: Option<usize>,
}

/// A `matchString` patch view: only the diff lines containing `needle`
/// (lowercase) plus `context` lines around them, each run under a
/// recomputed `@@ -a,b +c,d @@` header (the original section heading kept),
/// line text verbatim. `None` when no diff line matches (the file matched by
/// path), so the caller keeps the whole patch.
fn matching_hunks(patch: &str, needle: &str, context: usize) -> Option<String> {
    let mut hunks: Vec<(&str, Vec<DiffLine<'_>>)> = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    for text in patch.split_inclusive('\n') {
        if let Some(rest) = text.strip_prefix("@@ -") {
            let mut sides = rest.split(' ');
            let start = |side: Option<&str>| {
                side.and_then(|s| s.split(',').next())
                    .and_then(|n| n.trim_start_matches(['-', '+']).parse().ok())
                    .unwrap_or(0)
            };
            old = start(sides.next());
            new = start(sides.next());
            let heading = rest
                .split_once(" @@")
                .map_or("", |(_, heading)| heading)
                .trim_end_matches(['\r', '\n']);
            hunks.push((heading, Vec::new()));
            continue;
        }
        let Some((_, lines)) = hunks.last_mut() else {
            continue;
        };
        let (at_old, at_new) = match text.as_bytes().first() {
            Some(b'+') => (None, Some(new)),
            Some(b'-') => (Some(old), None),
            Some(b'\\') => (None, None),
            _ => (Some(old), Some(new)),
        };
        old += usize::from(at_old.is_some());
        new += usize::from(at_new.is_some());
        lines.push(DiffLine {
            text,
            old: at_old,
            new: at_new,
        });
    }
    let mut out = String::new();
    for (heading, lines) in &hunks {
        let hits = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.text.to_lowercase().contains(needle))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for hit in hits {
            let (from, to) = (
                hit.saturating_sub(context),
                (hit + context).min(lines.len() - 1),
            );
            match runs.last_mut() {
                Some(run) if from <= run.1 + 1 => run.1 = run.1.max(to),
                _ => runs.push((from, to)),
            }
        }
        for (from, to) in runs {
            let run = &lines[from..=to];
            let side = |pick: fn(&DiffLine<'_>) -> Option<usize>,
                        after: fn(&DiffLine<'_>) -> usize| {
                let count = run.iter().filter(|line| pick(line).is_some()).count();
                let start = run
                    .iter()
                    .find_map(pick)
                    .unwrap_or_else(|| run.first().map_or(0, after).saturating_sub(1));
                (start, count)
            };
            let (old_start, old_count) = side(|l| l.old, |l| l.new.unwrap_or(0));
            let (new_start, new_count) = side(|l| l.new, |l| l.old.unwrap_or(0));
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!(
                "@@ -{old_start},{old_count} +{new_start},{new_count} @@{heading}\n"
            ));
            for line in run {
                out.push_str(&clip_line(line.text, needle));
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// How a patch page's continuation cursor (`nextCharOffset`) is counted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PatchCursor {
    /// Offset into the whole page's patch stream; the continuation repeats
    /// the same file page (commit and compare).
    Page,
    /// Offset into the stream of the unfinished files only; the continuation
    /// narrows the file selection to them (pull requests).
    FirstUnfinished,
}

/// A page of changed files sharing one patch char window.
pub(super) struct PatchPage {
    pub(super) rows: Vec<Value>,
    /// Files whose patch is not fully delivered yet, in page order.
    pub(super) unfinished: Vec<String>,
    /// The `charOffset` that continues this window, in the [`PatchCursor`]
    /// coordinate; `None` once every patch on the page is delivered.
    pub(super) cursor: Option<usize>,
}

fn file_metadata(file: &Value) -> Value {
    json!({"filename":str_at(file,"/filename").unwrap_or(""),"status":string(file.get("status")),"additions":usize_at(file,"/additions"),"deletions":usize_at(file,"/deletions"),"previousFilename":file.get("previous_filename")})
}

/// Extensions GitHub never diffs as text.
const BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "bz2", "class", "db", "dll", "dylib", "eot", "exe", "flac",
    "gif", "gz", "ico", "jar", "jpeg", "jpg", "lib", "mov", "mp3", "mp4", "node", "o", "ogg",
    "otf", "pdf", "png", "psd", "pyc", "rlib", "so", "sqlite", "tgz", "tiff", "ttf", "war", "wasm",
    "wav", "webm", "webp", "woff", "woff2", "xz", "zip",
];

/// Why a changed file has no provider patch, or `None` when there is nothing
/// to diff (a pure rename). `tooLarge`: GitHub omits a single oversized diff
/// but still counts its lines. `binary`: a binary extension. `omitted`:
/// GitHub sent neither patch nor line counts — binary, empty, or past the
/// PR's total diff budget — so `additions`/`deletions` of 0 are not evidence
/// of an unchanged file; read the file at `sourceSha` instead.
pub(super) fn missing_patch_reason(file: &Value) -> Option<&'static str> {
    let changes = usize_at(file, "/additions") + usize_at(file, "/deletions");
    if changes > 0 {
        return Some("tooLarge");
    }
    if str_at(file, "/status") == Some("renamed") {
        return None;
    }
    let name = str_at(file, "/filename").unwrap_or("");
    let binary = name
        .rsplit_once('.')
        .is_some_and(|(_, ext)| BINARY_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
    Some(if binary { "binary" } else { "omitted" })
}

/// Inventory rows flag files GitHub sent without a patch. Only REST entries
/// (which carry a blob `sha`) say anything about patches; GraphQL file nodes
/// never include one.
fn inventory_patch_flag(row: &mut Value, file: &Value) {
    if file.get("patch").is_none()
        && file.get("sha").is_some()
        && let Some(reason) = missing_patch_reason(file)
    {
        row["patchUnavailable"] = json!(reason);
    }
}

/// Shape a page of changed files. Patches are packed whole, in file order,
/// into one char window over the page's concatenated patch stream: small
/// patches arrive complete, and only the file that crosses the window end is
/// cut. `charOffset` is that stream position, so a patch larger than the
/// window continues inside the same file and completed files are not
/// re-emitted. The first window also lists files not reached yet (metadata
/// only), so the page's file list is visible up front.
pub(super) fn shape_patch_page(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
    cursor_mode: PatchCursor,
) -> PatchPage {
    if !include_patch {
        let rows = files
            .iter()
            .map(|file| {
                let mut row = file_metadata(file);
                inventory_patch_flag(&mut row, file);
                remove_nulls(&mut row);
                row
            })
            .collect();
        return PatchPage {
            rows,
            unfinished: Vec::new(),
            cursor: None,
        };
    }
    let views = files
        .iter()
        .map(|file| {
            file.get("patch")
                .and_then(Value::as_str)
                .map(|patch| history_patch_view(patch, query))
        })
        .collect::<Vec<_>>();
    let lengths = views
        .iter()
        .map(|view| view.as_deref().map_or(0, |v| v.chars().count()))
        .collect::<Vec<_>>();
    // A view that is not the raw patch (a `matchString` view narrowed to
    // matching hunks, or a minified view with context replaced by `...`)
    // names the whole patch's size: the row marker selects the lossless
    // re-read (`next.readFullPatches` / `next.readUntrimmed`).
    let reshaped = needle(query).is_some() || minified_view(query);
    let narrowed = files
        .iter()
        .zip(&views)
        .map(|(file, view)| {
            let patch = str_at(file, "/patch")?;
            (reshaped && view.as_deref() != Some(patch)).then(|| patch.chars().count())
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<usize>();
    let offset = query.char_offset().unwrap_or(0).min(total);
    let window = match (needle(query), query.char_length()) {
        // A literal search returns many short hit runs: their row headers
        // are small, so the hits may fill the page share without the fixed
        // metadata reserve.
        (Some(_), None) => literal_patch_window(query.auto_page_chars, query.patch_rows),
        _ => patch_window(query.char_length(), query.auto_page_chars, query.patch_rows),
    };
    let end = (offset + window).min(total);
    let first_window = offset == 0;
    // Stream start of every file, and of the first file not fully delivered.
    let starts = lengths
        .iter()
        .scan(0usize, |acc, len| {
            let start = *acc;
            *acc += len;
            Some(start)
        })
        .collect::<Vec<_>>();
    let cursor_file = (0..files.len()).find(|&i| lengths[i] > 0 && starts[i] + lengths[i] > end);
    let cursor = cursor_file.map(|i| match cursor_mode {
        PatchCursor::Page => end,
        PatchCursor::FirstUnfinished => end - starts[i],
    });
    let mut rows = Vec::new();
    let mut unfinished = Vec::new();
    for (i, (file, view)) in files.iter().zip(&views).enumerate() {
        let (start, len) = (starts[i], lengths[i]);
        let mut row = file_metadata(file);
        match view {
            // Binary or oversized files have no patch: nothing to window, so
            // they are reported once, on the first window.
            None => {
                if !first_window {
                    continue;
                }
                match missing_patch_reason(file) {
                    // A pure rename changes no content: its diff is empty.
                    None => row["patch"] = json!(""),
                    Some(reason) => {
                        row["isPartial"] = json!(true);
                        row["terminalLimit"] = json!(true);
                        row["patchUnavailable"] = json!(reason);
                    }
                }
            }
            Some(_) if len == 0 => {
                if !first_window {
                    continue;
                }
                row["patch"] = json!("");
            }
            Some(view) => {
                if start + len <= offset {
                    continue; // delivered by an earlier window
                }
                let local_start = offset.saturating_sub(start);
                let local_end = end.saturating_sub(start).min(len);
                let is_cursor = cursor_file == Some(i);
                let started = local_end > local_start;
                if local_end < len {
                    unfinished.push(str_at(file, "/filename").unwrap_or("").to_owned());
                }
                // Files not reached yet ride the continuation (`unfinished`),
                // not empty placeholder rows.
                if !started {
                    continue;
                }
                if let Some(full) = narrowed[i] {
                    row["fullPatchChars"] = json!(full);
                }
                {
                    let text = view
                        .chars()
                        .skip(local_start)
                        .take(local_end.saturating_sub(local_start))
                        .collect::<String>();
                    row["patch"] = json!(text);
                }
                let cut = local_end < len;
                if cut || local_start > 0 {
                    let taken = local_end.saturating_sub(local_start);
                    let mut page = json!({"charOffset":local_start,"charLength":taken,"totalChars":len,"hasMore":cut});
                    // Per-file coordinates: the cut file continues at its own
                    // window end. The page-stream cursor a commit/compare
                    // continuation copies rides `PatchPage::cursor` instead.
                    if is_cursor && cursor.is_some() {
                        page["nextCharOffset"] = json!(local_end);
                    }
                    row["patchPagination"] = page;
                }
            }
        }
        remove_nulls(&mut row);
        rows.push(row);
    }
    PatchPage {
        rows,
        unfinished,
        cursor,
    }
}

/// The patch window row that carries the continuation cursor.
fn shaped_cursor(rows: Option<&Value>) -> Option<Value> {
    rows.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|row| row.pointer("/patchPagination/nextCharOffset").is_some())
        .map(|row| row["patchPagination"].clone())
}

/// Shape a commit or comparison file page (see [`shape_patch_page`]):
/// the file rows and the page-stream `charOffset` that continues them.
pub(super) fn shape_files(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
) -> (Value, Option<usize>) {
    let mut page = shape_patch_page(files, include_patch, query, PatchCursor::Page);
    for row in &mut page.rows {
        compact_file_header(row);
    }
    (Value::Array(page.rows), page.cursor)
}

/// Name a commit/compare page's patch-stream cursor on its `filesPagination`
/// (`nextPatchCharOffset`), the offset `next.continuePatch` carries.
pub(super) fn attach_patch_cursor(files_pagination: &mut Value, cursor: Option<usize>) {
    if let (Some(cursor), Some(page)) = (cursor, files_pagination.as_object_mut()) {
        page.insert("nextPatchCharOffset".into(), json!(cursor));
    }
}

/// Patch characters one call carries across a page's files by default,
/// derived from the effective automatic response page
/// (`output.pagination.defaultCharLength`, 1k–50k). Rendered text prints
/// patches verbatim (no escaping), so the default window takes 4/5 of the
/// page less a fixed reserve for the row header and metadata, never below
/// 2/5 of it: a default window plus row metadata fits one response page.
/// An explicit `charLength` is honoured up to that one-row budget: a larger
/// window would split the row into response `rowPart`s whose patch cursor
/// rides only the first part, so following it would skip the unread parts.
/// A bigger window comes with a bigger response page (`responseCharLength`).
const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);
const PATCH_DEFAULT_RESERVE: usize = 5_000;
/// Page assumed when the runtime did not supply one (direct callers, tests).
const FALLBACK_AUTO_PAGE: usize = 20_000;

fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(FALLBACK_AUTO_PAGE)
}

/// The default window of a `matchString` view: the page share without the
/// fixed reserve, split across the call's patch rows.
fn literal_patch_window(auto_page_chars: Option<usize>, rows: usize) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    let share = (page * num / den).max(1);
    (share / rows.max(1)).max(MIN_SHARED_PATCH_WINDOW.min(share))
}

/// The patch window one row gets on a response page of `page` chars.
pub(super) fn page_patch_budget(page: usize) -> usize {
    patch_window(None, Some(page), 1)
}

/// The warning of a patch read whose explicit `charLength` was clamped to
/// one response page; `None` when it fit.
pub(super) fn clamp_warning(query: &HistoryItemRequest) -> Option<String> {
    let length = query.char_length()?;
    let window = patch_window(Some(length), query.auto_page_chars, 1);
    (window < length).then(|| {
        format!(
            "charLength {length} exceeds one response page; patch windows hold {window} chars. Follow next.continuePatch for the rest."
        )
    })
}

/// Append `text` to a response's `warnings` (success rows keep warnings;
/// prose hints are for empty and error rows).
pub(super) fn push_warning(out: &mut Value, text: String) {
    match out.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(json!(text)),
        None => out["warnings"] = json!([text]),
    }
}

/// Smallest default window a row gets when several rows share the budget.
const MIN_SHARED_PATCH_WINDOW: usize = 2_000;

/// The patch window of one row. The default window is one call's budget:
/// `rows` patch-reading rows of the same call split it evenly. An explicit
/// `charLength` is clamped to the whole call budget, so one row's window
/// always fits one response page.
fn patch_window(char_length: Option<usize>, auto_page_chars: Option<usize>, rows: usize) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    let call_budget = (page * num / den)
        .min(page.saturating_sub(PATCH_DEFAULT_RESERVE))
        .max(page * 2 / 5)
        .max(1);
    if let Some(length) = char_length {
        return length.clamp(1, call_budget);
    }
    let rows = rows.max(1);
    if rows == 1 {
        return call_budget;
    }
    (call_budget / rows).max(MIN_SHARED_PATCH_WINDOW.min(call_budget))
}

/// A commit or comparison file scope: the `path` prefix and the `files`
/// paths/globs (any-of). A rename matches by its new or previous path.
pub(super) struct PathScope(Vec<PathPattern>);

impl PathScope {
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let mut patterns = query
            .path()
            .map(|path| PathPattern::Scope(path.to_owned()))
            .into_iter()
            .collect::<Vec<_>>();
        for pattern in &query.file_scope {
            patterns.push(
                PathPattern::parse(pattern)
                    .map_err(|error| error.replace("fileFilter.paths", "files"))?,
            );
        }
        Ok((!patterns.is_empty()).then_some(Self(patterns)))
    }

    pub(super) fn matches(&self, file: &Value) -> bool {
        let name = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename");
        self.0.iter().any(|pattern| {
            pattern.matches(name) || previous.is_some_and(|previous| pattern.matches(previous))
        })
    }
}

/// Whether a file sits in the optional scope.
pub(super) fn in_path_scope(file: &Value, scope: Option<&PathScope>) -> bool {
    scope.is_none_or(|scope| scope.matches(file))
}

pub(super) fn scope_files(files: Vec<Value>, scope: Option<&PathScope>) -> Vec<Value> {
    files
        .into_iter()
        .filter(|file| in_path_scope(file, scope))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::continuations::promote_pr_continuations;
    use super::*;

    fn patch_query(offset: usize) -> HistoryItemRequest {
        patch_request(json!({"charOffset":offset,"charLength":2}))
    }

    fn patch_request(fields: serde_json::Value) -> HistoryItemRequest {
        let base = json!({
            "operation":"pullRequest",
            "mainGoal": "test", "reasoning":"test",
            "owner":"a",
            "repo":"b",
            "number":1,
            "minify":"none"
        });
        HistoryItemRequest::from_row(super::super::util::merge(base, fields))
            .expect("patch query fixture should be valid")
    }

    fn file(name: &str, patch: &str) -> Value {
        json!({"filename":name,"status":"modified","patch":patch})
    }

    fn window(fields: Value) -> HistoryItemRequest {
        let base = json!({
            "operation":"commit","mainGoal":"test","reasoning":"test",
            "owner":"a","repo":"b","ref":"abc","includeDiff":true
        });
        HistoryItemRequest::from_row(super::super::util::merge(base, fields))
            .expect("commit window fixture should be valid")
    }

    /// Follow a commit page's `nextCharOffset` cursor to the end and return
    /// (calls, per-file reassembled patches, files reported patchless).
    fn follow_commit_page(
        files: &[Value],
        char_length: Option<usize>,
    ) -> (usize, HashMap<String, String>, Vec<String>) {
        let mut offset = 0usize;
        let mut calls = 0;
        let mut patches = HashMap::<String, String>::new();
        let mut patchless = Vec::new();
        loop {
            calls += 1;
            assert!(calls < 1_000, "cursor did not advance");
            let mut fields = json!({"charOffset":offset});
            if let Some(length) = char_length {
                fields["charLength"] = json!(length);
            }
            let page = shape_patch_page(files.to_vec(), true, &window(fields), PatchCursor::Page);
            for row in &page.rows {
                let name = str_at(row, "/filename").unwrap_or("").to_owned();
                if row.get("patchUnavailable").is_some() {
                    patchless.push(name.clone());
                }
                if let Some(text) = row.get("patch").and_then(Value::as_str) {
                    patches.entry(name).or_default().push_str(text);
                }
            }
            let Some(next) = page.cursor else {
                return (calls, patches, patchless);
            };
            assert!(next > offset, "cursor must advance");
            offset = next;
        }
    }

    /// D5: the page budget is not split evenly across files. Small patches
    /// arrive whole, a patch larger than the window continues inside the same
    /// file, and the call count is the stream length over the window.
    #[test]
    fn commit_patches_pack_whole_files_and_continue_inside_a_large_one() {
        let mut files = (0..29)
            .map(|i| file(&format!("small{i}.rs"), &"s".repeat(300)))
            .collect::<Vec<_>>();
        files.insert(3, file("big.rs", &"b".repeat(35_000)));
        let first = shape_patch_page(files.clone(), true, &window(json!({})), PatchCursor::Page);
        // The first three small files arrive whole, not as 266-char slices.
        for row in &first.rows[..3] {
            assert_eq!(row["patch"].as_str().map(str::len), Some(300), "{row}");
            assert!(row.get("patchPagination").is_none(), "{row}");
        }
        let window_chars = patch_window(None, None, 1);
        let big = &first.rows[3];
        assert_eq!(big["patchPagination"]["charOffset"], 0);
        assert_eq!(big["patchPagination"]["charLength"], window_chars - 900);
        assert_eq!(big["patchPagination"]["nextCharOffset"], window_chars - 900);
        assert_eq!(first.cursor, Some(window_chars));
        // Files not reached yet ride the continuation, not placeholder rows.
        assert_eq!(first.rows.len(), 4);
        assert_eq!(first.unfinished.len(), 27);

        let second = shape_patch_page(
            files.clone(),
            true,
            &window(json!({"charOffset":window_chars})),
            PatchCursor::Page,
        );
        // Completed files are not re-emitted; the big patch continues in place.
        assert_eq!(second.rows.len(), 1);
        assert_eq!(second.rows[0]["filename"], "big.rs");
        assert_eq!(
            second.rows[0]["patchPagination"]["charOffset"],
            window_chars - 900
        );

        let total: usize = 29 * 300 + 35_000;
        let (calls, patches, _) = follow_commit_page(&files, None);
        assert_eq!(calls, total.div_ceil(window_chars));
        assert_eq!(patches["big.rs"], "b".repeat(35_000));
        for i in 0..29 {
            assert_eq!(patches[&format!("small{i}.rs")], "s".repeat(300));
        }
    }

    #[test]
    fn single_file_and_three_hundred_file_commits_stay_lossless() {
        let one = vec![file("only.rs", &"x".repeat(20_000))];
        let (calls, patches, _) = follow_commit_page(&one, None);
        assert_eq!(calls, 20_000usize.div_ceil(patch_window(None, None, 1)));
        assert_eq!(patches["only.rs"], "x".repeat(20_000));

        let many = (0..300)
            .map(|i| file(&format!("f{i}.rs"), &format!("+{i}\n")))
            .collect::<Vec<_>>();
        let page = shape_patch_page(many.clone(), true, &window(json!({})), PatchCursor::Page);
        assert_eq!(page.rows.len(), 300);
        assert!(page.unfinished.is_empty());
        assert!(shaped_cursor(Some(&Value::Array(page.rows))).is_none());
        let (calls, patches, _) = follow_commit_page(&many, Some(50));
        assert!(calls > 1);
        for (i, source) in many.iter().enumerate() {
            assert_eq!(
                patches[&format!("f{i}.rs")],
                source["patch"].as_str().unwrap_or("")
            );
        }
    }

    #[test]
    fn binary_files_are_reported_once_and_never_block_the_cursor() {
        let files = vec![
            file("a.rs", &"a".repeat(10)),
            json!({"filename":"logo.png","status":"added"}),
            file("b.rs", &"b".repeat(10)),
            json!({"filename":"tail.bin","status":"added"}),
        ];
        let (calls, patches, patchless) = follow_commit_page(&files, Some(4));
        assert_eq!(calls, 5);
        assert_eq!(patchless, ["logo.png", "tail.bin"]);
        assert_eq!(patches["a.rs"], "a".repeat(10));
        assert_eq!(patches["b.rs"], "b".repeat(10));
        let only_binary = vec![json!({"filename":"logo.png","status":"added"})];
        let page = shape_patch_page(only_binary, true, &window(json!({})), PatchCursor::Page);
        assert_eq!(page.rows[0]["patchUnavailable"], "binary");
        assert!(page.unfinished.is_empty());
    }

    #[test]
    fn missing_patches_name_why_github_sent_none() {
        let file = |name: &str, status: &str, additions: u64| json!({"filename":name,"status":status,"additions":additions,"deletions":0});
        assert_eq!(
            missing_patch_reason(&file("src/checker.ts", "modified", 9)),
            Some("tooLarge")
        );
        assert_eq!(
            missing_patch_reason(&file("app/favicon.ICO", "modified", 0)),
            Some("binary")
        );
        assert_eq!(
            missing_patch_reason(&file("src/core.ts", "modified", 0)),
            Some("omitted")
        );
        assert_eq!(
            missing_patch_reason(&file("src/new.rs", "renamed", 0)),
            None
        );
    }

    #[test]
    fn pull_request_patch_cursor_is_relative_to_the_narrowed_selection() {
        let query = patch_query(2);
        let mut row = json!({});
        let mut pagination = Map::new();
        let files = vec![file("short.rs", "ABCD"), file("long.rs", "abcdefgh")];
        let no_match = shape_pr_files(
            &mut row,
            &mut pagination,
            files.clone(),
            WindowState::COMPLETE,
            &query,
            None,
            "all",
            None,
        )
        .no_selected_match;
        assert!(!no_match);
        assert_eq!(row["changedFiles"][0]["patch"], "CD");
        assert_eq!(row["changedFiles"][0]["patchPagination"]["hasMore"], false);
        assert_eq!(pagination["patches"]["files"], json!(["long.rs"]));
        // The continuation selects only long.rs, whose stream starts at 0.
        assert_eq!(pagination["patches"]["nextCharOffset"], 0);

        // Follow the narrowed continuation (selection = unfinished files,
        // charOffset = cursor) and rebuild every patch losslessly.
        let mut selection = files.clone();
        let mut offset = 0;
        let mut rebuilt = HashMap::<String, String>::new();
        for _ in 0..100 {
            let page = shape_patch_page(
                selection.clone(),
                true,
                &patch_query(offset),
                PatchCursor::FirstUnfinished,
            );
            for row in &page.rows {
                if let Some(text) = row.get("patch").and_then(Value::as_str) {
                    rebuilt
                        .entry(str_at(row, "/filename").unwrap_or("").to_owned())
                        .or_default()
                        .push_str(text);
                }
            }
            // The page cursor continues the narrowed selection (no placeholder
            // row carries it when the next file has not started).
            let Some(next) = page.cursor else {
                break;
            };
            offset = next;
            selection.retain(|f| {
                page.unfinished
                    .iter()
                    .any(|name| Some(name.as_str()) == str_at(f, "/filename"))
            });
        }
        assert_eq!(rebuilt["short.rs"], "ABCD");
        assert_eq!(rebuilt["long.rs"], "abcdefgh");
    }

    #[test]
    fn selected_missing_path_is_distinct_from_a_valid_selection() {
        let query = patch_query(0);
        let files = vec![json!({
            "filename":"src/lib.rs",
            "status":"modified",
            "patch":"abcd"
        })];
        let shape = |selection: Value| {
            let mut row = json!({});
            let mut pagination = Map::new();
            let no_match = shape_pr_files(
                &mut row,
                &mut pagination,
                files.clone(),
                WindowState::COMPLETE,
                &query,
                selection.as_object(),
                "selected",
                None,
            )
            .no_selected_match;
            (row, no_match)
        };

        let (valid, valid_no_match) = shape(json!({"files":["src/lib.rs"]}));
        assert!(!valid_no_match);
        assert_eq!(valid["changedFiles"][0]["path"], "src/lib.rs");

        let (missing, missing_no_match) = shape(json!({"files":["src/missing.rs"]}));
        assert!(missing_no_match);
        assert!(missing.get("changedFiles").is_none());

        let incomplete_state = WindowState {
            exhausted: false,
            ..WindowState::COMPLETE
        };
        let mut incomplete = json!({});
        let mut incomplete_pagination = Map::new();
        assert!(
            !shape_pr_files(
                &mut incomplete,
                &mut incomplete_pagination,
                files.clone(),
                incomplete_state,
                &query,
                json!({"files":["src/missing.rs"]}).as_object(),
                "selected",
                None,
            )
            .no_selected_match
        );

        let mut output = json!({"type":"pullRequests","pullRequests":[missing]});
        if missing_no_match {
            output["status"] = json!("empty");
            output["errorCode"] = json!("noSelectedFilesMatched");
            output["hints"] = json!(["copy a changed file path"]);
        }
        assert_eq!(output["status"], "empty");
        assert_eq!(output["type"], "pullRequests");
        assert_eq!(output["errorCode"], "noSelectedFilesMatched");
        assert!(
            output["hints"]
                .as_array()
                .is_some_and(|hints| !hints.is_empty())
        );
    }

    #[test]
    fn patch_pagination_lists_every_unfinished_file_and_continues_only_them() {
        let query = patch_query(0);
        let mut row = json!({});
        let mut pagination = Map::new();
        let files = vec![
            json!({"filename":"a.rs","status":"modified","patch":"ABCD"}),
            json!({"filename":"next.rs","status":"modified","patch":"x"}),
            json!({"filename":"b.rs","status":"modified","patch":"abcdef"}),
        ];
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            &query,
            None,
            "all",
            None,
        );
        assert_eq!(
            pagination["patches"]["files"],
            json!(["a.rs", "next.rs", "b.rs"])
        );
        let request: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
            "content":{"patches":{"mode":"all"}},"filePage":2
        }))
        .expect("query");
        let mut out = json!({"type":"pullRequests","pullRequests":[{"contentPagination":{
            "patches": pagination["patches"].clone()
        }}]});
        promote_pr_continuations(&mut out, &request);
        let next = &out["next"]["continuePatch"]["query"];
        assert_eq!(
            next["content"]["patches"],
            json!({"mode":"selected","files":["a.rs","next.rs","b.rs"]}),
            "{out}"
        );
        assert!(next.get("filePage").is_none(), "{next}");
        assert_eq!(next["charOffset"], 2);
        assert!(next.get("collectionPages").is_none());
    }

    /// A patch window fills most of the automatic response page: a 100k-char
    /// patch reads in three calls at the default 50k page, not thirteen.
    #[test]
    fn patch_window_is_one_budget_for_the_whole_page() {
        assert_eq!(patch_window(None, None, 1), 15_000);
        assert_eq!(patch_window(Some(2), None, 1), 2);
        assert_eq!(patch_window(None, Some(50_000), 1), 40_000);
        assert_eq!(
            100_071usize.div_ceil(patch_window(None, Some(50_000), 1)),
            3
        );
        // Rows of one call split the budget;
        // an explicit charLength still wins; a share never drops below 2,000.
        assert_eq!(patch_window(None, None, 2), 7_500);
        assert_eq!(patch_window(None, Some(20_000), 2), 7_500);
        assert_eq!(patch_window(Some(9_000), None, 2), 9_000);
        assert_eq!(patch_window(Some(90_000), None, 2), 15_000);
        assert_eq!(patch_window(None, None, 50), 2_000);
        assert_eq!(patch_window(None, Some(1_000), 5), 400);
    }

    /// An explicit `charLength` above one response page is clamped to the
    /// page's one-row budget: the row never splits into response parts whose
    /// patch cursor would skip the unread parts.
    #[test]
    fn explicit_char_length_is_clamped_to_one_response_page() {
        assert_eq!(patch_window(Some(80_000), Some(20_000), 1), 15_000);
        assert_eq!(patch_window(Some(50_000), None, 1), 15_000);
        assert_eq!(patch_window(Some(100_000), Some(50_000), 1), 40_000);
        assert_eq!(patch_window(Some(9_000), Some(50_000), 3), 9_000);
        assert_eq!(patch_window(Some(150_000), Some(1_000), 1), 400);
        let patch = "+x\n".repeat(30_000);
        let mut query = window(json!({"charLength":80_000}));
        query.auto_page_chars = Some(20_000);
        let page = shape_patch_page(
            vec![file("big.rs", &patch)],
            true,
            &query,
            PatchCursor::Page,
        );
        let row = &page.rows[0];
        assert_eq!(
            row["patch"].as_str().map(|p| p.chars().count()),
            Some(15_000)
        );
        assert_eq!(row["patchPagination"]["charLength"], 15_000);
        assert_eq!(page.cursor, Some(15_000));
    }

    /// A walk with an explicit `charLength` far above the response page reads
    /// every patch exactly once: each window fits one page, and following
    /// the cursor never skips or repeats a character of a multi-file stream.
    #[test]
    fn oversized_char_length_walk_reads_every_patch_exactly_once() {
        let files = (0..6)
            .map(|i| {
                file(
                    &format!("f{i}.rs"),
                    &format!("+{i}\n").repeat(4_000 + i * 3_000),
                )
            })
            .collect::<Vec<_>>();
        let total = files
            .iter()
            .map(|f| f["patch"].as_str().map_or(0, |p| p.chars().count()))
            .sum::<usize>();
        for length in [25_000usize, 40_000, 80_000, 100_000] {
            let mut offset = 0usize;
            let mut read = HashMap::<String, String>::new();
            let mut calls = 0;
            loop {
                calls += 1;
                assert!(calls < 100, "cursor did not advance");
                let mut query = window(json!({"charOffset":offset,"charLength":length}));
                query.auto_page_chars = Some(20_000);
                let page = shape_patch_page(files.clone(), true, &query, PatchCursor::Page);
                let mut taken = 0;
                for row in &page.rows {
                    let name = str_at(row, "/filename").unwrap_or("").to_owned();
                    let text = row["patch"].as_str().unwrap_or("");
                    let have = read.entry(name).or_default();
                    let at = row["patchPagination"]["charOffset"].as_u64().unwrap_or(0);
                    assert_eq!(at as usize, have.chars().count(), "gap or repeat: {row}");
                    have.push_str(text);
                    taken += text.chars().count();
                }
                assert!(
                    taken <= patch_window(None, Some(20_000), 1),
                    "{length}: {taken}"
                );
                match page.cursor {
                    Some(next) => {
                        assert_eq!(next, offset + taken, "{length}");
                        offset = next;
                    }
                    None => break,
                }
            }
            assert_eq!(
                calls,
                total.div_ceil(patch_window(None, Some(20_000), 1)),
                "{length}"
            );
            for source in &files {
                let name = source["filename"].as_str().unwrap_or("");
                assert_eq!(
                    read.get(name).map(String::as_str),
                    source["patch"].as_str(),
                    "{length} {name}"
                );
            }
        }
    }

    /// D2: a commit row's `patchPagination` speaks per-file coordinates only
    /// (`charOffset + charLength == nextCharOffset`); the page-stream cursor
    /// that `charOffset` continues is named separately on the page.
    #[test]
    fn commit_patch_rows_report_per_file_offsets_and_the_page_cursor_separately() {
        let files = vec![file("a.rs", &"a".repeat(3)), file("b.rs", &"b".repeat(20))];
        let first = shape_patch_page(
            files.clone(),
            true,
            &window(json!({"charLength":10})),
            PatchCursor::Page,
        );
        let b = &first.rows[1]["patchPagination"];
        assert_eq!(b["charOffset"], 0);
        assert_eq!(b["charLength"], 7);
        assert_eq!(b["nextCharOffset"], 7, "{b}");
        assert_eq!(first.cursor, Some(10));
        let second = shape_patch_page(
            files,
            true,
            &window(json!({"charOffset":10,"charLength":10})),
            PatchCursor::Page,
        );
        let b = &second.rows[0]["patchPagination"];
        assert_eq!(b["charOffset"], 7);
        assert_eq!(b["charLength"], 10);
        assert_eq!(b["nextCharOffset"], 17, "{b}");
        assert_eq!(second.cursor, Some(20));
        let (rows, cursor) = shape_files(
            vec![file("a.rs", "aaa"), file("b.rs", &"b".repeat(20))],
            true,
            &window(json!({"charLength":10})),
        );
        assert_eq!(rows[1]["patchPagination"]["nextCharOffset"], 7);
        assert_eq!(cursor, Some(10));
    }

    /// `matchString` narrows a patch to the matching lines plus context under
    /// recomputed hunk headers; a path-only match keeps the whole patch.
    #[test]
    fn match_string_patch_view_keeps_only_matching_hunks() {
        let body = (1..=20)
            .map(|n| format!(" line {n}\r\n"))
            .collect::<String>();
        let patch = format!(
            "@@ -1,22 +1,22 @@ fn main\r\n{body}-old Needle\r\n+new needle\r\n{body}@@ -80,3 +80,3 @@\n x\n-y\n+z"
        );
        let view = matching_hunks(&patch, "needle", 3).expect("a line matches");
        assert_eq!(
            view,
            "@@ -18,4 +18,4 @@ fn main\n line 18\r\n line 19\r\n line 20\r\n-old Needle\r\n+new needle\r\n line 1\r\n line 2\r\n line 3\r\n"
                .replace("@@ -18,4 +18,4 @@", "@@ -18,7 +18,7 @@")
        );
        assert!(matching_hunks(&patch, "absent", 3).is_none());
        // matchContext 0: only the hit lines, under one header per run.
        assert_eq!(
            matching_hunks(&patch, "needle", 0).as_deref(),
            Some("@@ -21,1 +21,1 @@ fn main\n-old Needle\r\n+new needle\r\n")
        );
        // A generated one-line diff keeps only the text around each hit.
        let long = format!(
            "@@ -1 +1 @@\n+{}PointerEvent{}\r\n",
            "a".repeat(1_000),
            "b".repeat(1_000)
        );
        let clipped = matching_hunks(&long, "pointerevent", 3).expect("hit");
        assert_eq!(
            clipped,
            format!(
                "@@ -0,0 +1,1 @@\n+[… 850 chars …]{}PointerEvent{}[… 850 chars …]\r\n",
                "a".repeat(150),
                "b".repeat(150)
            )
        );
        let query = patch_request(json!({"matchString":"NEEDLE","matchContext":3}));
        let page = shape_patch_page(
            vec![file("src/a.rs", &patch)],
            true,
            &query,
            PatchCursor::FirstUnfinished,
        );
        assert_eq!(page.rows[0]["patch"], view);
        assert_eq!(page.rows[0]["fullPatchChars"], patch.chars().count());
    }

    /// The default minified PR view replaces long context runs with `...`:
    /// such a row is marked with its whole patch size (the marker selects
    /// `next.readUntrimmed`); an untouched patch and `minify:"none"` are not.
    #[test]
    fn minified_patch_rows_are_marked_for_the_untrimmed_read() {
        let long = (1..=40)
            .map(|n| format!(" ctx {n}\n"))
            .chain(["-old\n".to_owned(), "+new\n".to_owned()])
            .collect::<String>();
        let short = "@@ -1,2 +1,2 @@\n a\n-b\n+c\n";
        let query = patch_request(json!({"minify":"standard"}));
        let page = shape_patch_page(
            vec![file("big.rs", &long), file("small.rs", short)],
            true,
            &query,
            PatchCursor::FirstUnfinished,
        );
        let view = page.rows[0]["patch"].as_str().expect("patch");
        assert!(view.contains("..."), "{view}");
        assert_eq!(page.rows[0]["fullPatchChars"], long.chars().count());
        assert!(
            page.rows[1].get("fullPatchChars").is_none(),
            "{:?}",
            page.rows
        );
        let raw = shape_patch_page(
            vec![file("big.rs", &long)],
            true,
            &patch_request(json!({})),
            PatchCursor::FirstUnfinished,
        );
        assert_eq!(raw.rows[0]["patch"], long);
        assert!(raw.rows[0].get("fullPatchChars").is_none());
    }

    /// `matchString` keeps only the hit lines by default (matchContext
    /// 0) and pages every hit file of the PR at once, not 30 per call.
    #[test]
    fn match_string_defaults_to_hit_lines_and_one_page_of_hit_files() {
        let patch = "@@ -1,3 +1,3 @@\n a\n-old miri\n+new miri\n b\n";
        let query = patch_request(json!({"matchString":"miri"}));
        assert_eq!(
            history_patch_view(patch, &query),
            "@@ -2,1 +2,1 @@\n-old miri\n+new miri\n"
        );
        assert_eq!(
            file_page_size(&query, true),
            super::super::window::MAX_FILE_BATCHES * MAX_COLLECTION_PAGE
        );
        let explicit = patch_request(json!({"matchString":"miri","pageSize":5}));
        assert_eq!(file_page_size(&explicit, true), 5);
    }

    /// A patch page emits only rows that carry patch text (or say why a
    /// file has none); files not reached yet ride the continuation, not
    /// empty placeholder rows. PR rows carry one compact `stat`.
    #[test]
    fn patch_pages_emit_no_placeholder_rows_and_compact_headers() {
        let files = vec![
            json!({"filename":"a.rs","status":"modified","additions":2,"deletions":1,"patch":"A".repeat(10)}),
            json!({"filename":"b.rs","status":"added","additions":9,"deletions":0,"patch":"B".repeat(10)}),
            json!({"filename":"c.rs","status":"modified","additions":1,"deletions":1,"patch":"C".repeat(10)}),
        ];
        let page = shape_patch_page(
            files.clone(),
            true,
            &patch_request(json!({"charOffset":0,"charLength":10})),
            PatchCursor::FirstUnfinished,
        );
        assert_eq!(page.rows.len(), 1, "{:?}", page.rows);
        assert_eq!(page.unfinished, ["b.rs", "c.rs"]);
        assert_eq!(page.cursor, Some(0));
        let mut row = json!({});
        let mut pagination = Map::new();
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            &patch_request(json!({"charOffset":0,"charLength":15})),
            None,
            "all",
            None,
        );
        assert_eq!(
            row["changedFiles"],
            json!([
                {"path":"a.rs","stat":"M +2 -1","patch":"AAAAAAAAAA"},
                {"path":"b.rs","stat":"A +9 -0","patch":"BBBBB",
                 "patchPagination":{"charOffset":0,"charLength":5,"totalChars":10,"hasMore":true,"nextCharOffset":5}}
            ])
        );
        assert_eq!(pagination["patches"]["nextCharOffset"], 5);
        assert_eq!(pagination["patches"]["files"], json!(["b.rs", "c.rs"]));
    }

    /// The review pick names every source file by churn within
    /// one budget, skipping tests, docs and lockfiles.
    #[test]
    fn review_selection_packs_source_files_by_churn() {
        let f =
            |name: &str, changed: u64| json!({"filename":name,"additions":changed,"deletions":0});
        let files = vec![
            f(".changeset/x.md", 8),
            f("pydantic/json_schema.py", 40),
            f("tests/test_counter.py", 400),
            f("pydantic/_known_annotated_metadata.py", 12),
            f("pydantic/fields.py", 90),
            f("uv.lock", 900),
            f("pyproject.toml", 2),
            f("docs/img/diagram.png", 0),
        ];
        assert_eq!(
            review_selection(&files, 15_000),
            [
                "pydantic/fields.py",
                "pydantic/json_schema.py",
                "pydantic/_known_annotated_metadata.py",
                "pyproject.toml"
            ]
        );
        // A tight budget leads with the files that fit whole; the larger
        // ones follow (read through continuePatch), never dropped.
        assert_eq!(
            review_selection(&files, 2_000),
            [
                "pydantic/json_schema.py",
                "pydantic/fields.py",
                "pydantic/_known_annotated_metadata.py",
                "pyproject.toml"
            ]
        );
        assert!(review_selection(&[f("tests/a_test.py", 3)], 15_000).is_empty());
        // Tests named by their language's convention stay out of the review
        // pick wherever they live.
        let polyglot = vec![
            f("pkg/cmd/discussion/client/client_test.go", 3_842),
            f("pkg/cmd/discussion/create/create.go", 120),
            f("src/test/java/com/acme/WidgetTest.java", 300),
            f("src/main/java/com/acme/Latest.java", 10),
            f("spec/models/user_spec.rb", 80),
            f("app/test_api.py", 60),
        ];
        assert_eq!(
            review_selection(&polyglot, 15_000),
            [
                "pkg/cmd/discussion/create/create.go",
                "src/main/java/com/acme/Latest.java"
            ]
        );
        // A literal search fills the page share without the metadata reserve.
        assert_eq!(literal_patch_window(None, 1), 16_000);
        assert_eq!(literal_patch_window(Some(20_000), 2), 8_000);
    }

    /// Commit rows use the PR header (`path` + `stat`), and a
    /// commit's `files` scope takes paths and globs like a PR's.
    #[test]
    fn commit_rows_are_compact_and_files_scope_them() {
        let (rows, _) = shape_files(
            vec![
                json!({"filename":"src/a.rs","status":"added","additions":3,"deletions":0,"patch":"+a"}),
            ],
            true,
            &window(json!({})),
        );
        assert_eq!(
            rows,
            json!([{"path":"src/a.rs","stat":"A +3 -0","patch":"+a"}])
        );
        let scoped = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "ref":"abc","files":["*.md","src/"]
        }))
        .expect("commit files");
        let scope = PathScope::from_query(&scoped)
            .expect("valid")
            .expect("scope");
        let names = ["src/a.rs", "README.md", "lib/b.rs"]
            .into_iter()
            .filter(|name| scope.matches(&json!({"filename": name})))
            .collect::<Vec<_>>();
        assert_eq!(names, ["src/a.rs", "README.md"]);
    }

    fn inventory_request(fields: Value) -> HistoryItemRequest {
        patch_request(super::super::util::merge(
            json!({"content":{"changedFiles":true}}),
            fields,
        ))
    }

    fn listed(name: &str, status: &str, additions: u64, deletions: u64, patch: bool) -> Value {
        let mut file = json!({"sha":"1","filename":name,"status":status,
            "additions":additions,"deletions":deletions});
        if patch {
            file["patch"] = json!("@@ -1 +1 @@");
        }
        file
    }

    fn inventory(files: Vec<Value>, query: &HistoryItemRequest) -> (Value, Map<String, Value>) {
        let mut row = json!({});
        let mut pagination = Map::new();
        let scope = InventoryFilter::from_query(query).expect("valid fileFilter");
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            query,
            None,
            "none",
            scope.as_ref(),
        );
        (row, pagination)
    }

    /// A patch-free inventory row is one compact string; consecutive files in
    /// one directory share a `{"dir/": [...]}` group so the prefix is written
    /// once. Order, patchless reasons and rename origins survive.
    #[test]
    fn inventory_rows_are_compact_and_group_consecutive_directory_files() {
        let mut renamed = listed("src/new.rs", "renamed", 0, 0, false);
        renamed["previous_filename"] = json!("lib/old.rs");
        let files = vec![
            listed(".eslintrc", "modified", 1, 1, true),
            listed("src/a.ts", "added", 8, 0, true),
            listed("src/checker.ts", "modified", 39_550, 39_342, false),
            renamed,
            listed("src/ui/logo.png", "added", 0, 0, false),
            listed("src/z.ts", "removed", 0, 5, true),
            listed("docs/omitted.md", "modified", 0, 0, false),
        ];
        let (row, pagination) = inventory(files, &inventory_request(json!({})));
        assert_eq!(
            row["changedFiles"],
            json!([
                "M +1 -1 .eslintrc",
                {"src/": [
                    "A +8 -0 a.ts",
                    "M +39550 -39342 !tooLarge checker.ts",
                    "R +0 -0 new.rs <- lib/old.rs"
                ]},
                "A +0 -0 !binary src/ui/logo.png",
                "D +0 -5 src/z.ts",
                "M +0 -0 !omitted docs/omitted.md"
            ])
        );
        assert_eq!(pagination["changedFiles"]["totalItems"], 7);
    }

    /// `fileFilter` narrows the inventory by status, path glob or prefix and
    /// change count; totals count matches only. A file GitHub sent without
    /// line counts is not "small": it passes `minChanges`.
    #[test]
    fn file_filter_narrows_by_status_path_and_change_count() {
        let files = || {
            vec![
                listed("src/a.ts", "modified", 3, 1, true),
                listed("src/deep/b.ts", "added", 40, 0, true),
                listed("src/c.rs", "modified", 50, 0, true),
                listed("docs/readme.md", "modified", 9, 9, true),
                listed("src/huge.ts", "modified", 0, 0, false),
            ]
        };
        let names = |fields: Value| {
            let (row, pagination) = inventory(files(), &inventory_request(fields));
            let mut out = Vec::new();
            for item in row["changedFiles"].as_array().into_iter().flatten() {
                match item {
                    Value::String(row) => out.push(row.rsplit(' ').next().unwrap_or("").to_owned()),
                    Value::Object(group) => {
                        for (dir, rows) in group {
                            for row in rows.as_array().into_iter().flatten() {
                                let name =
                                    row.as_str().unwrap_or("").rsplit(' ').next().unwrap_or("");
                                out.push(format!("{dir}{name}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(pagination["changedFiles"]["totalItems"], out.len());
            out
        };
        // "**" spans zero or more directories; "*" stays in one segment.
        assert_eq!(
            names(json!({"fileFilter":{"paths":["src/**/*.ts"]}})),
            ["src/a.ts", "src/deep/b.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"fileFilter":{"paths":["src/*.ts"]}})),
            ["src/a.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"fileFilter":{"paths":["*.ts"]}})),
            ["src/a.ts", "src/deep/b.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"fileFilter":{"paths":["src/deep", "docs/"]}})),
            ["src/deep/b.ts", "docs/readme.md"]
        );
        assert_eq!(
            names(json!({"fileFilter":{"status":["added"]}})),
            ["src/deep/b.ts"]
        );
        assert_eq!(
            names(json!({"fileFilter":{"paths":["*.ts"],"status":["modified"],"minChanges":10}})),
            ["src/huge.ts"]
        );
        let invalid = inventory_request(json!({"fileFilter":{"paths":["src/[a"]}}));
        assert!(InventoryFilter::from_query(&invalid).is_err());
    }

    /// An omitted pageSize sizes a patch-free inventory to the response page;
    /// an explicit one may exceed a provider batch only without patches.
    #[test]
    fn inventory_page_size_fills_the_response_page() {
        let mut query = inventory_request(json!({}));
        query.auto_page_chars = Some(50_000);
        assert_eq!(file_page_size(&query, false), 833);
        assert_eq!(file_page_size(&query, true), default_page_size());
        query.auto_page_chars = Some(1_000);
        assert_eq!(file_page_size(&query, false), MAX_COLLECTION_PAGE);
        let explicit = inventory_request(json!({"pageSize":1000}));
        assert_eq!(file_page_size(&explicit, false), 1_000);
        assert_eq!(file_page_size(&explicit, true), MAX_COLLECTION_PAGE);
        let files = (0..700)
            .map(|i| listed(&format!("d{}/f{i}.rs", i % 2), "modified", 1, 1, true))
            .collect::<Vec<_>>();
        let (row, pagination) = inventory(files, &explicit);
        assert_eq!(row["changedFiles"].as_array().map(Vec::len), Some(700));
        assert_eq!(pagination["changedFiles"]["hasMore"], false);
    }

    /// H4: at `defaultCharLength` 1000 the patch window shrinks with the page,
    /// so a page of patches still fits one automatic response page.
    #[test]
    fn patch_window_derives_from_the_effective_auto_page() {
        assert_eq!(patch_window(None, Some(1_000), 1), 400);
        let mut query = patch_request(json!({"charOffset":0}));
        query.auto_page_chars = Some(1_000);
        let patch = "+x\n".repeat(2_000);
        let page = shape_patch_page(
            vec![json!({"filename":"a.rs","status":"modified","patch":patch})],
            true,
            &query,
            PatchCursor::FirstUnfinished,
        );
        let shaped = &page.rows[0];
        let text = shaped["patch"].as_str().expect("patch");
        assert!(text.encode_utf16().count() <= 400, "{}", text.len());
        assert_eq!(shaped["patchPagination"]["hasMore"], true);
    }
}
