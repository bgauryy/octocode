//! Changed files: selection filters, path scopes, per-file shaping and the
//! shared patch char window.
use super::util::{minified_view, needle, str_at, string, usize_at};
use super::window::WindowState;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, MAX_COLLECTION_PAGE};
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
            .map(|pattern| PathPattern::parse(pattern))
            .collect::<Result<Vec<_>, _>>()?;
        let status = filter.status.iter().map(ToString::to_string).collect();
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

/// Largest patch-free inventory page.
pub(super) const MAX_INVENTORY_PAGE: usize = 1_000;
/// Rendered chars budgeted per compact inventory row when sizing the default
/// inventory page to the automatic response page.
const INVENTORY_ROW_CHARS: usize = 60;

/// Changed files per page. A patch-free inventory defaults to as many compact
/// rows as fit one automatic response page (100–1000); a page carrying
/// patches keeps provider-sized pages.
pub(super) fn file_page_size(query: &HistoryItemRequest, patches: bool) -> usize {
    match (query.page_size(), patches) {
        (_, true) => query.collection_page_size(),
        (Some(size), false) => size.clamp(1, MAX_INVENTORY_PAGE),
        (None, false) => (auto_page(query.auto_page_chars) / INVENTORY_ROW_CHARS)
            .clamp(MAX_COLLECTION_PAGE, MAX_INVENTORY_PAGE)
            .max(DEFAULT_PAGE_SIZE),
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
    /// The page's most reviewable file (the `getSelectedPatches` pick).
    pub(super) patch_target: Option<String>,
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

/// The page's file whose patch best answers "what changed": the most
/// reviewable tier, then the most changed lines, then page order.
fn patch_target(files: &[Value]) -> Option<String> {
    files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let path = str_at(file, "/filename")?;
            let changed = usize_at(file, "/additions") + usize_at(file, "/deletions");
            Some(((review_tier(path), std::cmp::Reverse(changed), index), path))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, path)| path.to_owned())
}

/// Shape a pull request's changed-file page into `row`: compact inventory
/// rows without patches, one object per file with them.
#[allow(clippy::too_many_arguments)]
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
    let patch_target = patch_target(&slice);
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
            patch_target,
            unsearched,
            first_unsearched,
        };
    }
    let shaped = patches
        .rows
        .into_iter()
        .map(|mut shaped| {
            if let Some(fields) = shaped.as_object_mut() {
                if let Some(name) = fields.remove("filename") {
                    fields.insert("path".into(), name);
                }
                // A rename's origin path (PR rows use the `path` vocabulary).
                if let Some(previous) = fields.remove("previousFilename") {
                    fields.insert("previousPath".into(), previous);
                }
            }
            shaped
        })
        .collect::<Vec<_>>();
    if !shaped.is_empty() {
        row["changedFiles"] = Value::Array(shaped);
    }
    // One continuation covers the page's patch stream; it lists every
    // unfinished file (the cut one plus those not yet started).
    if patch_mode != "none"
        && let Some(cursor) = shaped_cursor(row.get("changedFiles"))
    {
        let mut patch_page = cursor;
        patch_page["files"] = json!(patches.unfinished);
        pagination.insert("patches".into(), patch_page);
    }
    pagination.insert("changedFiles".into(), page);
    ShapedFiles {
        no_selected_match: selection_requested && !selected_path_matched && state.exhausted,
        patch_target,
        unsearched,
        first_unsearched,
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

/// Diff lines kept around each `matchString` hit when `matchContext` is omitted.
const MATCH_CONTEXT_LINES: usize = 3;
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
    // A `matchString` view narrowed to matching hunks names the whole
    // patch's size, so the caller knows more exists.
    let narrowed = files
        .iter()
        .zip(&views)
        .map(|(file, view)| {
            let patch = str_at(file, "/patch")?;
            (needle(query).is_some() && view.as_deref() != Some(patch))
                .then(|| patch.chars().count())
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<usize>();
    let offset = query.char_offset().unwrap_or(0).min(total);
    let end = (offset + patch_window(query.char_length(), query.auto_page_chars)).min(total);
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
                // Files not reached yet are listed on the first window only.
                if !started && !is_cursor && !first_window {
                    continue;
                }
                if let Some(full) = narrowed[i] {
                    row["fullPatchChars"] = json!(full);
                }
                if started || is_cursor {
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
                    if is_cursor && let Some(cursor) = cursor {
                        page["nextCharOffset"] = json!(cursor);
                    }
                    row["patchPagination"] = page;
                }
            }
        }
        remove_nulls(&mut row);
        rows.push(row);
    }
    PatchPage { rows, unfinished }
}

/// The patch window row that carries the continuation cursor.
fn shaped_cursor(rows: Option<&Value>) -> Option<Value> {
    rows.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|row| row.pointer("/patchPagination/nextCharOffset").is_some())
        .map(|row| row["patchPagination"].clone())
}

/// Shape a commit or comparison file page (see [`shape_patch_page`]).
pub(super) fn shape_files(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
) -> Value {
    Value::Array(shape_patch_page(files, include_patch, query, PatchCursor::Page).rows)
}

/// Patch characters one call carries across a page's files by default, and
/// the ceiling for an explicit `charLength`, derived from the effective
/// automatic response page (`output.pagination.defaultCharLength`, 1k–50k).
/// Rendered text prints patches verbatim (no escaping), so a window takes
/// most of the page (4/5 by default, 9/10 at most) less a fixed reserve for
/// the row header and metadata, never below 2/5 and 3/5 of it. A window of patches plus row
/// metadata then fits one response page, so responsePagination rarely splits
/// the row; when it does, the row's `next.*` rides only its first `rowPart`.
const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);
const PATCH_BUDGET_SHARE: (usize, usize) = (9, 10);
const PATCH_DEFAULT_RESERVE: usize = 5_000;
const PATCH_BUDGET_RESERVE: usize = 3_000;
/// Page assumed when the runtime did not supply one (direct callers, tests).
const FALLBACK_AUTO_PAGE: usize = 20_000;

fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(FALLBACK_AUTO_PAGE)
}

fn patch_window(char_length: Option<usize>, auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let share = |(num, den): (usize, usize), reserve: usize, floor: usize| {
        (page * num / den)
            .min(page.saturating_sub(reserve))
            .max(page * floor / 5)
    };
    let default = share(PATCH_DEFAULT_SHARE, PATCH_DEFAULT_RESERVE, 2);
    let budget = share(PATCH_BUDGET_SHARE, PATCH_BUDGET_RESERVE, 3);
    match char_length {
        Some(length) => length.clamp(1, budget.max(1)),
        None => default.max(1),
    }
}

/// Whether a file (or its pre-rename path) sits at or under `path`.
pub(super) fn in_path_scope(file: &Value, path: Option<&str>) -> bool {
    path.is_none_or(|path| {
        let name = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename").unwrap_or("");
        name == path
            || previous == path
            || name.starts_with(
                if path.ends_with('/') {
                    path.to_owned()
                } else {
                    format!("{path}/")
                }
                .as_str(),
            )
    })
}

pub(super) fn scope_files(files: Vec<Value>, path: Option<&str>) -> Vec<Value> {
    files
        .into_iter()
        .filter(|file| in_path_scope(file, path))
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
            "goal": "test", "reasoning":"test",
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
            "operation":"commit","goal":"test","reasoning":"test",
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
            let Some(next) = shaped_cursor(Some(&Value::Array(page.rows)))
                .and_then(|p| p["nextCharOffset"].as_u64())
            else {
                return (calls, patches, patchless);
            };
            assert!(next as usize > offset, "cursor must advance");
            offset = next as usize;
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
        let window_chars = patch_window(None, None);
        let big = &first.rows[3];
        assert_eq!(big["patchPagination"]["charOffset"], 0);
        assert_eq!(big["patchPagination"]["charLength"], window_chars - 900);
        assert_eq!(big["patchPagination"]["nextCharOffset"], window_chars);
        // The rest of the page is listed (metadata only) on the first window.
        assert_eq!(first.rows.len(), 30);
        assert!(first.rows[4].get("patch").is_none());
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
        assert_eq!(calls, 20_000usize.div_ceil(patch_window(None, None)));
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
            let Some(next) = shaped_cursor(Some(&Value::Array(page.rows))) else {
                break;
            };
            offset = next["nextCharOffset"].as_u64().unwrap_or(0) as usize;
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
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
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
        assert_eq!(patch_window(None, None), 15_000);
        assert_eq!(patch_window(Some(50_000), None), 17_000);
        assert_eq!(patch_window(Some(2), None), 2);
        assert_eq!(patch_window(None, Some(50_000)), 40_000);
        assert_eq!(patch_window(Some(100_000), Some(50_000)), 45_000);
        assert_eq!(100_071usize.div_ceil(patch_window(None, Some(50_000))), 3);
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
        let query = patch_request(json!({"matchString":"NEEDLE"}));
        let page = shape_patch_page(
            vec![file("src/a.rs", &patch)],
            true,
            &query,
            PatchCursor::FirstUnfinished,
        );
        assert_eq!(page.rows[0]["patch"], view);
        assert_eq!(page.rows[0]["fullPatchChars"], patch.chars().count());
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
        assert_eq!(file_page_size(&query, true), DEFAULT_PAGE_SIZE);
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
        assert_eq!(patch_window(None, Some(1_000)), 400);
        assert_eq!(patch_window(Some(5_000), Some(1_000)), 600);
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
