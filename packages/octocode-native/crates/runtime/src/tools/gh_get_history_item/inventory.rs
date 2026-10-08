//! Changed-file inventory: compact rows, directory summaries, the review
//! pick, and the pull-request file page.
use super::filter::*;
use super::patch::*;
use super::util::{needle, str_at, string, usize_at};
use super::window::WindowState;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, MAX_COLLECTION_PAGE};
use serde_json::{Map, Value, json};

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
pub(super) const INVENTORY_ROW_CHARS: usize = 60;

/// Changed files per page. A patch-free inventory defaults to as many compact
/// rows as fit one automatic response page (100–1000); a page carrying
/// patches defaults to one provider batch.
pub(super) fn file_page_size(query: &HistoryItemRequest, patches: bool) -> usize {
    match (query.page_size(), patches) {
        // A literal search returns every hit file of the PR on one page:
        // hits are bounded by matches (and the patch window), not by files.
        (None, true) if query.match_string().is_some() => {
            super::window::MAX_FILE_BATCHES * MAX_COLLECTION_PAGE
        }
        // Patches pack into one char window whatever the page size, so a
        // patch read lists a whole provider batch: one continuation stream
        // walks every patch of a PR of up to that many files.
        (None, true) => super::window::PROVIDER_BATCH,
        (Some(_), true) => query.collection_page_size(),
        (Some(size), false) => size.clamp(1, max_inventory_page()),
        (None, false) => (auto_page(query.auto_page_chars) / INVENTORY_ROW_CHARS)
            .clamp(MAX_COLLECTION_PAGE, max_inventory_page())
            .max(DEFAULT_PAGE_SIZE),
    }
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

/// Compact a patch-free file page, in provider order: a run of two or more
/// consecutive files in one directory becomes `{"dir/": [rows]}` whose rows
/// name files relative to it; every other file is a full-path row.
pub(super) fn compact_inventory(files: &[Value]) -> Vec<Value> {
    group_by_directory(files, inventory_row)
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

/// Group consecutive rows of one directory: a run of two or more files in
/// one directory becomes `{"dir/": [rows]}` whose rows name files relative
/// to it; every other file is a full-path row, in provider order.
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
                    json!(render(file, &path[dir.len() + 1..]))
                })
                .collect::<Vec<_>>();
            let mut group = Map::new();
            group.insert(format!("{dir}/"), Value::Array(rows));
            out.push(Value::Object(group));
        } else {
            out.extend(
                run.iter()
                    .map(|file| json!(render(file, str_at(file, "/filename").unwrap_or("")))),
            );
        }
        start = end;
    }
    out
}

/// What shaping a pull request's changed-file page found.
pub(super) struct ShapedFiles {
    /// A selected path matched no changed file and the provider has no more.
    pub(super) no_selected_match: bool,
    /// The page's review pick (`next.readSelectedPatches`).
    pub(super) review: Vec<String>,
    /// In-scope text files a `matchString` could not search because GitHub
    /// sent no patch for them, as compact `!reason path` rows.
    pub(super) unsearched: Vec<String>,
    /// The first of those paths (the source-read template).
    pub(super) first_unsearched: Option<String>,
    /// Files whose `matchString` context the hunks cut short, with the
    /// new-side windows to read at the head.
    pub(super) clipped: Vec<(String, Vec<String>)>,
}

/// Review priority of a changed path: source first, then tests, then other
/// files; docs, changesets, lockfiles, and generated output last.
pub(super) fn review_tier(path: &str) -> u8 {
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
pub(super) const REVIEW_LINE_CHARS: usize = 45;

pub(super) const REVIEW_FILE_CHARS: usize = 80;

/// Files one review read names at most: one provider-sized patch page.
pub(super) const MAX_REVIEW_FILES: usize = 30;

/// `next.readSelectedPatches`: the files whose patches answer "what changed" —
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
            let binary = is_binary_name(path);
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
    let review = review_selection(&slice, patch_window(None, query.auto_page_chars));
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
    // A patch read that does not fit one response lists its page's files
    // first (status, change counts, hunks), then the patches in file order.
    let summary = (patch_mode != "none" && query.char_offset().is_none())
        .then(|| group_by_directory(&slice, summary_row));
    let more_files = page.get("hasMore").and_then(Value::as_bool) == Some(true);
    // The summary rides the first window of a page whose patches do not
    // all fit it: its chars come out of that window, so the row (summary
    // plus patches) still fits one response page and never splits into a
    // part without patch rows.
    let summary_chars = summary
        .as_ref()
        .filter(|summary| !summary.is_empty())
        .map_or(0, crate::tools::stream_page::json_chars);
    let include_patch = patch_mode != "none";
    let patches = if summary_chars == 0 {
        shape_patch_page(slice, include_patch, query)
    } else if more_files {
        shape_patch_page_reserving(slice, include_patch, query, summary_chars)
    } else {
        let whole = shape_patch_page(slice.clone(), include_patch, query);
        if whole.cursor.is_none() {
            whole
        } else {
            shape_patch_page_reserving(slice, include_patch, query, summary_chars)
        }
    };
    if patch_mode == "none" {
        let rows = compact_inventory(&patches.rows);
        if !rows.is_empty() {
            row["files"] = Value::Array(rows);
        }
        pagination.insert("files".into(), page);
        return ShapedFiles {
            no_selected_match: false,
            review,
            unsearched,
            first_unsearched,
            clipped: Vec::new(),
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
    if let Some(summary) = summary
        && (patches.cursor.is_some() || more_files)
        && !summary.is_empty()
    {
        row["fileSummary"] = Value::Array(summary);
    }
    if !shaped.is_empty() {
        row["files"] = Value::Array(shaped);
    }
    // One continuation covers the page's patch stream: the same file page
    // at the stream cursor; the page counts the unfinished files.
    if let Some(cursor) = patches.cursor {
        pagination.insert(
            "patches".into(),
            json!({"hasMore":true,"nextOffset":cursor,"unfinishedFiles":patches.unfinished.len()}),
        );
    }
    pagination.insert("files".into(), page);
    ShapedFiles {
        no_selected_match: selection_requested && !selected_path_matched && state.exhausted,
        review,
        unsearched,
        first_unsearched,
        clipped: patches.clipped,
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

/// Extensions GitHub never diffs as text.
pub(super) const BINARY_EXTENSIONS: &[&str] = &[
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
    Some(if is_binary_name(name) {
        "binary"
    } else {
        "omitted"
    })
}

/// Whether a changed file's name has a binary extension (its basename's,
/// never a dotted directory's).
fn is_binary_name(path: &str) -> bool {
    BINARY_EXTENSIONS.contains(&octocode_engine::text::extension_of(path, true, "").as_str())
}

/// Inventory rows flag files GitHub sent without a patch. Only REST entries
/// (which carry a blob `sha`) say anything about patches; GraphQL file nodes
/// never include one.
pub(super) fn inventory_patch_flag(row: &mut Value, file: &Value) {
    if file.get("patch").is_none()
        && file.get("sha").is_some()
        && let Some(reason) = missing_patch_reason(file)
    {
        row["patchUnavailable"] = json!(reason);
    }
}
