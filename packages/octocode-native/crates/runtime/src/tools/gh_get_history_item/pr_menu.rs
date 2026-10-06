//! The pull-request read menu: the public query a continuation starts from,
//! the section menu, and the merge-commit read.
use super::util::{content_flag, merge};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, remove_nulls};
use serde_json::{Map, Value, json};

/// The caller's query as a working query for `operation`: the public fields,
/// with the pull-request/issue selection held as the internal `content`
/// selector (builders edit it); [`flatten_selectors`] turns it back into the
/// public `sections`/`include`/`patchRanges`/`includeBots` on emission.
pub(super) fn base_public_query(q: &HistoryItemRequest, operation: ItemOperation) -> Value {
    let mut v = serde_json::to_value(&q.query).unwrap_or_default();
    remove_nulls(&mut v);
    if let Some(m) = v.as_object_mut() {
        if matches!(operation, ItemOperation::PullRequest | ItemOperation::Issue) {
            for key in ["sections", "patchRanges", "includeBots"] {
                m.remove(key);
            }
            if let Some(content) = q.content_value() {
                m.insert("content".into(), content);
            }
        }
        m.insert(
            "operation".into(),
            json!(match operation {
                ItemOperation::PullRequest => "pullRequest",
                ItemOperation::Issue => "issue",
                ItemOperation::Commit => "commit",
                ItemOperation::Compare => "compare",
            }),
        );
        m.remove("debug");
        match operation {
            ItemOperation::PullRequest => {
                // pageSize has no contract default: each surface sizes its own
                // page, so an omitted pageSize stays omitted on replay.
                m.insert(
                    "minify".into(),
                    json!(q.minify().unwrap_or_else(|| "standard".to_owned())),
                );
                if let Some(Value::Object(content)) = m.get_mut("content")
                    && content.get("patches").is_some()
                {
                    content.insert("files".into(), json!(true));
                }
            }
            ItemOperation::Compare => {
                m.insert("page".into(), json!(q.page().unwrap_or(1)));
                m.insert("filePage".into(), json!(q.file_page().unwrap_or(1)));
                m.insert(
                    "pageSize".into(),
                    json!(q.page_size().unwrap_or(DEFAULT_PAGE_SIZE)),
                );
            }
            _ => {}
        }
    }
    v
}

/// An executable `ghGetHistoryItem` continuation in the public spelling.
pub(super) fn continuation(mut q: Value) -> Value {
    flatten_selectors(&mut q);
    Continuation::new(ToolId::GhGetHistoryItem, q)
        .confidence("exact")
        .build()
}

/// A pull-request read in the public spelling, without the default
/// (`standard`) `minify`.
pub(super) fn menu_read(mut q: Value) -> Value {
    if q.get("minify").and_then(Value::as_str) == Some("standard") {
        remove_key(&mut q, "minify");
    }
    continuation(q)
}

/// Pull-request sections in their published order.
pub(super) const PR_SECTION_ORDER: [&str; 8] = [
    "body",
    "files",
    "patches",
    "comments",
    "reviewComments",
    "reviews",
    "commits",
    "commitFiles",
];

/// Rewrite a working pull-request or issue query's internal `content` into
/// the public `sections`, `include` (selected patch files), `patchRanges`
/// and `includeBots`. Lossless: every internal selector has one public
/// spelling.
pub(super) fn flatten_selectors(query: &mut Value) {
    let Some(fields) = query.as_object_mut() else {
        return;
    };
    if !matches!(
        fields.get("operation").and_then(Value::as_str),
        Some("pullRequest" | "issue")
    ) {
        return;
    }
    let Some(content) = fields.remove("content") else {
        return;
    };
    for key in ["sections", "patchRanges", "includeBots"] {
        fields.remove(key);
    }
    let flag = |key: &str| content.get(key).and_then(Value::as_bool) == Some(true);
    let mut sections = Vec::new();
    if flag("body") {
        sections.push("body");
    }
    if flag("files") {
        sections.push("files");
    }
    if let Some(patches) = content.get("patches").and_then(Value::as_object)
        && patches.get("mode").and_then(Value::as_str) != Some("none")
    {
        sections.push("patches");
        if patches.get("mode").and_then(Value::as_str) == Some("selected") {
            if let Some(files) = patches
                .get("files")
                .and_then(Value::as_array)
                .filter(|files| !files.is_empty())
            {
                fields.insert("include".into(), Value::Array(files.clone()));
            }
            if let Some(ranges) = patches
                .get("ranges")
                .and_then(Value::as_array)
                .filter(|ranges| !ranges.is_empty())
            {
                fields.insert("patchRanges".into(), Value::Array(ranges.clone()));
            }
        }
    }
    if let Some(comments) = content.get("comments").and_then(Value::as_object) {
        let set = |key: &str| comments.get(key).and_then(Value::as_bool) == Some(true);
        if set("discussion") {
            sections.push("comments");
        }
        if set("reviewInline") {
            sections.push("reviewComments");
        }
        if set("includeBots") {
            fields.insert("includeBots".into(), json!(true));
        }
    }
    if flag("reviews") {
        sections.push("reviews");
    }
    if let Some(commits) = content.get("commits").and_then(Value::as_object) {
        sections.push(
            if commits.get("includeFiles").and_then(Value::as_bool) == Some(true) {
                "commitFiles"
            } else {
                "commits"
            },
        );
    }
    sections.sort_by_key(|section| {
        PR_SECTION_ORDER
            .iter()
            .position(|known| known == section)
            .unwrap_or(PR_SECTION_ORDER.len())
    });
    if !sections.is_empty() {
        fields.insert("sections".into(), json!(sections));
    }
}

/// A diff at most this many changed lines reads in one all-patches call, so a
/// separate file-list-only fetch would only repeat its file list.
pub(super) const SMALL_DIFF_LINES: u64 = 100;

/// Unfiltered inventories up to this many files keep `readPatches` beside
/// the selected-patch pick: every patch is still a bounded read.
pub(super) const INVENTORY_ALL_PATCHES_FILES: u64 = 30;

/// A first-page fetch of `query`'s PR: the base public query without its
/// content selection, filters, or per-surface cursors.
pub(super) fn fresh_pr_query(query: &HistoryItemRequest) -> Value {
    let mut target = base_public_query(query, ItemOperation::PullRequest);
    if let Some(object) = target.as_object_mut() {
        for key in [
            "content",
            "offset",
            "length",
            "commentPage",
            "commitPage",
            "reviewPage",
            "filePage",
            "page",
            "matchString",
        ] {
            object.remove(key);
        }
    }
    target
}

/// Whether a pull request's whole diff fits one patch read: at most
/// `SMALL_DIFF_LINES` changed lines over at most
/// [`INVENTORY_ALL_PATCHES_FILES`] files (an unknown file count passes). An
/// unknown line count is never small.
pub(super) fn is_small_pr(
    additions: Option<u64>,
    deletions: Option<u64>,
    changed_files: Option<u64>,
) -> bool {
    matches!(
        (additions, deletions),
        (Some(additions), Some(deletions)) if additions + deletions <= SMALL_DIFF_LINES
    ) && changed_files.is_none_or(|files| files <= INVENTORY_ALL_PATCHES_FILES)
}

/// Leads one menu offers at most; other surfaces stay reachable through
/// `include`.
pub(super) const MENU_CAP: usize = 2;

/// Per-row menu of first-page fetches for content the call did not request:
/// at most [`MENU_CAP`] entries, in priority order (files or patches, then
/// the body, then the discussion). A merged row already shows
/// `mergeCommitSha`, so no merge-commit read is offered.
///
/// `raw` is the provider PR object; `review` is the inventory's
/// [`super::inventory::review_selection`] (empty before files were read). An
/// entry is emitted only when it can return something the row does not
/// already hold: a non-empty body (the summary row carries none) rides
/// `readFiles`, else the patch read or `readBody`; a small diff reads every patch (`readPatches`)
/// instead of a separate file list; `readDiscussion` reads comments and
/// reviews together, without comments when the provider counts none.
pub(super) fn pr_next_menu(
    query: &HistoryItemRequest,
    content: Option<&Map<String, Value>>,
    patch_mode: &str,
    review: &[String],
    raw: &Value,
) -> Value {
    let count = |key: &str| raw.get(key).and_then(Value::as_u64);
    let has_body = raw
        .get("body")
        .and_then(Value::as_str)
        .is_some_and(|body| !body.trim().is_empty());
    let want_body = !content_flag(content, "body") && has_body;
    let changed_files = count("changed_files");
    let has_files = changed_files != Some(0);
    let small_pr = is_small_pr(count("additions"), count("deletions"), changed_files);
    let no_comments = count("comments") == Some(0) && count("review_comments") == Some(0);
    // Each menu entry is a fresh first-page fetch: the base public query
    // without the current content selection or any per-surface cursor.
    let target = fresh_pr_query(query);
    let mut next = Map::new();
    let call = |content: Value| menu_read(merge(target.clone(), json!({"content":content})));
    let with_body = |mut content: Value| {
        if want_body {
            content["body"] = json!(true);
        }
        content
    };
    let files_read = content_flag(content, "files") || patch_mode != "none";
    let mut body_offered = false;
    if !files_read && has_files && !small_pr {
        next.insert("readFiles".into(), call(with_body(json!({"files":true}))));
        body_offered = true;
    }
    if patch_mode == "none" && has_files {
        let covers_all = changed_files.is_some_and(|files| review.len() as u64 >= files);
        if !review.is_empty() && !covers_all {
            // The query is exact; which files answer the question is a
            // ranking guess (source files by churn, packed to one budget).
            let mut selected = call(json!({"patches":{"mode":"selected","files":review}}));
            selected["confidence"] = json!("high");
            next.insert("readSelectedPatches".into(), selected);
            if changed_files.is_none_or(|files| files <= INVENTORY_ALL_PATCHES_FILES) {
                next.insert(
                    "readPatches".into(),
                    call(json!({"patches":{"mode":"all"}})),
                );
            }
        } else if small_pr || covers_all {
            let patches = json!({"patches":{"mode":"all"}});
            let patches = if body_offered {
                patches
            } else {
                with_body(patches)
            };
            body_offered = true;
            next.insert("readPatches".into(), call(patches));
        }
    }
    if want_body && !body_offered && !files_read {
        next.insert("readBody".into(), call(json!({"body":true})));
    }
    let mut discussion = Map::new();
    if content.and_then(|v| v.get("comments")).is_none() && !no_comments {
        discussion.insert(
            "comments".into(),
            json!({"discussion":true,"reviewInline":true}),
        );
    }
    if !content_flag(content, "reviews") {
        discussion.insert("reviews".into(), json!(true));
    }
    if !discussion.is_empty() {
        next.insert("readDiscussion".into(), call(Value::Object(discussion)));
    }
    Value::Object(next.into_iter().take(MENU_CAP).collect())
}

/// Lines `next.readAtMerge` reads around each hunk's new-side span, so the
/// merged window shows the enclosing code beyond the diff context.
pub(super) const MERGE_WINDOW_PAD: usize = 10;

/// Ranges one `ghGetFileContent` read takes (the contract's `maxItems`).
pub(super) const MERGE_WINDOW_RANGES: usize = 10;

/// `next.readAtMerge`: a merged pull request whose patches were read offers
/// its most-changed code file (not a test) at the merge commit, as one
/// numbered read of every hunk's new-side lines (padded, merged, at most
/// [`MERGE_WINDOW_RANGES`] ranges), so the fix is checked in the code that
/// shipped. `files` is every loaded provider file (`filename`, `additions`,
/// `deletions`, `patch`), not only the patch window shown. `None` for
/// unmerged pull requests, for a `matchString` read (a targeted answer
/// already), and when no such file adds a line. Only files the read
/// selected (`include`, `status`, `minChanges`) are candidates: `selected`.
pub(super) fn read_at_merge(
    query: &HistoryItemRequest,
    raw: &Value,
    files: &[Value],
    selected: impl Fn(&Value) -> bool,
) -> Option<Value> {
    use crate::content::{FileType, classify_file_type, is_test_path};
    if query.match_string().is_some() {
        return None;
    }
    raw.get("merged_at").filter(|merged| !merged.is_null())?;
    let sha = raw
        .get("merge_commit_sha")
        .and_then(Value::as_str)
        .filter(|sha| !sha.is_empty())?;
    let (path, ranges) = files
        .iter()
        .filter(|file| selected(file))
        .filter_map(|file| {
            let path = file.get("filename")?.as_str()?;
            let patch = file.get("patch")?.as_str()?;
            (classify_file_type(path) == Some(FileType::Code)
                && !is_test_path(path)
                && adds_lines(patch))
            .then_some(())?;
            let ranges = merge_windows(patch);
            (!ranges.is_empty()).then(|| (churn(file), path, ranges))
        })
        // The most-changed file; the first of equals.
        .rev()
        .max_by_key(|(churn, _, _)| *churn)
        .map(|(_, path, ranges)| (path, ranges))?;
    Some(
        Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": query.owner(),
                "repo": query.repo(),
                "ref": sha,
                "path": path,
                "ranges": ranges,
            }),
        )
        .confidence("high")
        .build(),
    )
}

/// Whether a patch adds a line with something to read (not a lone brace or
/// a redaction placeholder).
pub(super) fn adds_lines(patch: &str) -> bool {
    patch
        .lines()
        .filter(|line| !line.starts_with("@@") && !line.starts_with("+++"))
        .filter_map(|line| line.strip_prefix('+'))
        .any(|text| {
            text.chars().filter(|c| c.is_alphanumeric()).count() >= 3 && !text.contains("[REDACTED")
        })
}

/// Added plus removed lines of a provider file.
pub(super) fn churn(file: &Value) -> u64 {
    ["additions", "deletions"]
        .iter()
        .filter_map(|key| file.get(*key).and_then(Value::as_u64))
        .sum()
}

/// The new-side span of every hunk header (`@@ -a,b +c,d @@`), padded by
/// [`MERGE_WINDOW_PAD`] lines and merged where they touch; past
/// [`MERGE_WINDOW_RANGES`] spans, the closest neighbours merge. Pure
/// deletions (`+c,0`) have no new-side lines. A whole hunk that opens with
/// the diff's context but ends with less reaches the end of the file, so its
/// span is not padded past it.
pub(super) fn merge_windows(patch: &str) -> Vec<String> {
    hunk_windows(patch, Side::New)
}

/// One side of a unified diff: `-a,b` (the parent) or `+c,d` (the change).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Old,
    New,
}

/// [`merge_windows`] on either side: each hunk's span on `side` (old-side
/// lines are context and `-` lines, new-side lines context and `+` lines),
/// so the ranges read the file as that side of the diff numbers it.
pub(super) fn hunk_windows(patch: &str, side: Side) -> Vec<String> {
    let mark = match side {
        Side::Old => '-',
        Side::New => '+',
    };
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut lines = patch.lines().peekable();
    while let Some(header) = lines.next() {
        if !header.starts_with("@@") {
            continue;
        }
        let (mut side_lines, mut leading, mut trailing) = (0usize, None, 0usize);
        while let Some(line) = lines.next_if(|line| !line.starts_with("@@")) {
            if line.starts_with(' ') {
                side_lines += 1;
                trailing += 1;
            } else if line.starts_with('+') || line.starts_with('-') {
                side_lines += usize::from(line.starts_with(mark));
                leading.get_or_insert(trailing);
                trailing = 0;
            }
        }
        let Some((start, count)) = hunk_side(header, mark) else {
            continue;
        };
        if count == 0 {
            continue;
        }
        // Only a whole hunk with the diff's context says where the file ends.
        let context = leading == Some(DIFF_CONTEXT_LINES) || start == 1;
        let at_end = side_lines == count && context && trailing < DIFF_CONTEXT_LINES;
        let span = (
            start.saturating_sub(MERGE_WINDOW_PAD).max(1),
            start + count - 1 + if at_end { 0 } else { MERGE_WINDOW_PAD },
        );
        match spans.last_mut() {
            Some(last) if span.0 <= last.1 + 1 => last.1 = last.1.max(span.1),
            _ => spans.push(span),
        }
    }
    while spans.len() > MERGE_WINDOW_RANGES {
        let closest = (1..spans.len())
            .min_by_key(|&i| spans[i].0 - spans[i - 1].1)
            .unwrap_or(1);
        spans[closest - 1].1 = spans[closest].1;
        spans.remove(closest);
    }
    spans
        .into_iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect()
}

/// The two refs a change's numbered gutters read at: `new_ref` for ` `/`+`
/// lines (the commit, the PR head, the comparison head), `old_ref` for `-`
/// lines (the first parent, the PR base, the comparison merge base).
pub(super) struct ChangeSides<'a> {
    pub(super) new_ref: Option<&'a str>,
    pub(super) old_ref: Option<&'a str>,
    /// `high` when `old_ref` is the diff's exact old side; `medium` for a
    /// PR base, which may have moved past the diff's merge base.
    pub(super) old_confidence: &'static str,
}

/// `readAtCommit` and `readParent`: the change's most-changed code file (a
/// test, doc or other file only when no code file has a patch), read whole
/// at the new side and, over its hunks' old-side windows, at the old side
/// (its pre-rename path). An added file has no parent read, a removed one
/// no new-side read. Only files the read selected are candidates.
pub(super) fn change_reads(
    owner: &str,
    repo: &str,
    files: &[Value],
    sides: &ChangeSides<'_>,
    selected: impl Fn(&Value) -> bool,
) -> Vec<(&'static str, Value)> {
    use crate::content::{FileType, classify_file_type, is_test_path};
    let rank = |file: &Value| {
        let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
        let code = classify_file_type(path) == Some(FileType::Code) && !is_test_path(path);
        (code, churn(file))
    };
    let Some(file) = files
        .iter()
        .filter(|file| selected(file) && file.get("patch").and_then(Value::as_str).is_some())
        .rev()
        .max_by_key(|file| rank(file))
    else {
        return Vec::new();
    };
    let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
    let status = file.get("status").and_then(Value::as_str).unwrap_or("");
    let patch = file.get("patch").and_then(Value::as_str).unwrap_or("");
    let mut reads = Vec::new();
    if let Some(new_ref) = sides.new_ref.filter(|_| status != "removed") {
        reads.push((
            "readAtCommit",
            Continuation::new(
                ToolId::GhGetFileContent,
                json!({"owner": owner, "repo": repo, "path": path, "ref": new_ref}),
            )
            .confidence("high")
            .build(),
        ));
    }
    if let Some(old_ref) = sides.old_ref.filter(|_| status != "added") {
        let old_path = file
            .get("previous_filename")
            .and_then(Value::as_str)
            .unwrap_or(path);
        let mut row = json!({"owner": owner, "repo": repo, "path": old_path, "ref": old_ref});
        let ranges = hunk_windows(patch, Side::Old);
        if !ranges.is_empty() {
            row["ranges"] = json!(ranges);
        }
        reads.push((
            "readParent",
            Continuation::new(ToolId::GhGetFileContent, row)
                .confidence(sides.old_confidence)
                .build(),
        ));
    }
    reads
}

/// Context lines a unified diff keeps around each change.
pub(super) const DIFF_CONTEXT_LINES: usize = 3;

/// The `(start, count)` of a hunk header's `mark` side (`-` or `+`).
fn hunk_side(header: &str, mark: char) -> Option<(usize, usize)> {
    let side = header.split(' ').find_map(|part| part.strip_prefix(mark))?;
    let mut numbers = side.split(',').map(str::parse::<usize>);
    let start = numbers.next()?.ok()?;
    let count = match numbers.next() {
        Some(count) => count.ok()?,
        None => 1,
    };
    Some((start, count))
}

pub(super) fn remove_key(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}
