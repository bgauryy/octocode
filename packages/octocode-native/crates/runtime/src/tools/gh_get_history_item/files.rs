//! Changed files: selection filters, path scopes, per-file shaping and the
//! shared patch char window.
use super::GhGetHistoryItemQuery;
use super::util::{minified_view, needle, paginate_text, str_at, string, usize_at};
use super::window::WindowState;
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Changed-file selection shared by the provider scan and output shaping.
pub(super) struct FileFilter<'a> {
    pub(super) selected: &'a [String],
    pub(super) needle: Option<&'a str>,
}

impl FileFilter<'_> {
    pub(super) fn is_trivial(&self) -> bool {
        self.selected.is_empty() && self.needle.is_none()
    }
    pub(super) fn matches(&self, file: &Value) -> bool {
        let path = str_at(file, "/filename").unwrap_or("");
        (self.selected.is_empty() || self.selected.iter().any(|selected| selected == path))
            && self.needle.is_none_or(|n| {
                path.to_lowercase().contains(n)
                    || str_at(file, "/patch").is_some_and(|v| v.to_lowercase().contains(n))
            })
    }
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

/// Shape a pull request's changed-file page into `row`. Returns true when a
/// selected path matched no changed file and the provider has no more files.
pub(super) fn shape_pr_files(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    files: Vec<Value>,
    state: WindowState,
    query: &GhGetHistoryItemQuery,
    selector: Option<&Map<String, Value>>,
    patch_mode: &str,
) -> bool {
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
    };
    let filtered = files
        .into_iter()
        .filter(|file| filter.matches(file))
        .collect::<Vec<_>>();
    let (slice, page) = state.paginate(filtered, query.file_page, query.page_size);
    let files_on_page = slice.len();
    let shaped = slice
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
            let mut shaped = shape_file(&file, patch_mode != "none", query, files_on_page);
            if let Some(name) = shaped.as_object_mut().and_then(|v| v.remove("filename")) {
                shaped["path"] = name;
            }
            if let Some(shaped) = shaped.as_object_mut() {
                shaped.remove("previousFilename");
            }
            shaped
        })
        .collect::<Vec<_>>();
    if !shaped.is_empty() {
        row["changedFiles"] = Value::Array(shaped);
    }
    // Every file on the page shares one char window, so one continuation
    // covers them all; list each unfinished file, not just the first.
    if patch_mode != "none" {
        let unfinished = row
            .get("changedFiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|v| v.pointer("/patchPagination/hasMore") == Some(&json!(true)))
            .collect::<Vec<_>>();
        if let Some(first) = unfinished.first() {
            let mut patch_page = first["patchPagination"].clone();
            patch_page["files"] = json!(
                unfinished
                    .iter()
                    .filter_map(|v| str_at(v, "/path"))
                    .collect::<Vec<_>>()
            );
            pagination.insert("patches".into(), patch_page);
        }
    }
    pagination.insert("changedFiles".into(), page);
    selection_requested && !selected_path_matched && state.exhausted
}

fn history_patch_view(value: &str, query: &GhGetHistoryItemQuery) -> String {
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

/// Shape one changed file. `files_on_page` splits the per-page patch budget
/// so a page of patches fits one automatic response page; the shared
/// `charOffset` continuation keeps every file lossless.
pub(super) fn shape_file(
    file: &Value,
    include_patch: bool,
    query: &GhGetHistoryItemQuery,
    files_on_page: usize,
) -> Value {
    let mut out = json!({"filename":str_at(file,"/filename").unwrap_or(""),"status":string(file.get("status")),"additions":usize_at(file,"/additions"),"deletions":usize_at(file,"/deletions"),"previousFilename":file.get("previous_filename")});
    if include_patch {
        if let Some(patch) = file.get("patch").and_then(Value::as_str) {
            let patch = history_patch_view(patch, query);
            let (text, page) = paginate_text(
                &patch,
                query.char_offset,
                Some(patch_window(
                    query.char_length,
                    files_on_page,
                    query.auto_page_chars,
                )),
            );
            out["patch"] = json!(text);
            if query.char_offset.unwrap_or(0) > 0 || page["hasMore"] == true {
                out["patchPagination"] = page;
            }
        } else {
            out["isPartial"] = json!(true);
            out["terminalLimit"] = json!(true);
            out["patchUnavailable"] = json!({"reason":"providerOmittedPatch"});
        }
    }
    remove_nulls(&mut out);
    out
}

/// Shape a page of files with one shared patch window.
pub(super) fn shape_files(
    files: Vec<Value>,
    include_patch: bool,
    query: &GhGetHistoryItemQuery,
) -> Value {
    let count = files.len();
    Value::Array(
        files
            .into_iter()
            .map(|v| shape_file(&v, include_patch, query, count))
            .collect(),
    )
}

/// Patch characters one page carries across all its files by default, and the
/// ceiling for an explicit `charLength`, as shares of the effective automatic
/// response page (`output.pagination.defaultCharLength`, 1k–50k). A page of
/// patches plus row metadata then fits one response page, so
/// responsePagination rarely splits the row; when it does, the row's `next.*`
/// rides only its last `rowPart`.
const PATCH_DEFAULT_PAGE: usize = 8_000;
const PATCH_PAGE_BUDGET: usize = 12_000;
/// Page assumed when the runtime did not supply one (direct callers, tests).
const FALLBACK_AUTO_PAGE: usize = 20_000;

fn patch_window(
    char_length: Option<usize>,
    files_on_page: usize,
    auto_page: Option<usize>,
) -> usize {
    let files = files_on_page.max(1);
    let page = auto_page
        .filter(|page| *page > 0)
        .unwrap_or(FALLBACK_AUTO_PAGE);
    let default = PATCH_DEFAULT_PAGE.min(page * 2 / 5);
    let budget = PATCH_PAGE_BUDGET.min(page * 3 / 5);
    match char_length {
        Some(length) => length.min((budget / files).max(1)),
        None => (default / files).max(1),
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

    fn patch_query(offset: usize) -> GhGetHistoryItemQuery {
        serde_json::from_value(json!({
            "operation":"pullRequest",
            "owner":"a",
            "repo":"b",
            "number":1,
            "charOffset":offset,
            "charLength":2,
            "minify":"none"
        }))
        .expect("patch query fixture should be valid")
    }

    #[test]
    fn multi_file_patch_continuation_survives_a_completed_first_file() {
        let query = patch_query(2);
        let mut row = json!({});
        let mut pagination = Map::new();
        let files = vec![
            json!({"filename":"short.rs","status":"modified","patch":"ABCD"}),
            json!({"filename":"long.rs","status":"modified","patch":"abcdefgh"}),
        ];

        let no_match = shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            &query,
            None,
            "all",
        );

        assert!(!no_match);
        assert_eq!(row["changedFiles"][0]["patch"], "CD");
        assert_eq!(row["changedFiles"][0]["patchPagination"]["hasMore"], false);
        assert_eq!(row["changedFiles"][1]["patch"], "cd");
        assert_eq!(pagination["patches"]["hasMore"], true);
        assert_eq!(pagination["patches"]["nextCharOffset"], 4);

        let source = json!({"filename":"long.rs","status":"modified","patch":"abcdefgh"});
        let rebuilt = [0, 2, 4, 6]
            .into_iter()
            .filter_map(|offset| {
                shape_file(&source, true, &patch_query(offset), 1)["patch"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect::<String>();
        assert_eq!(rebuilt.as_bytes(), b"abcdefgh");
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
            );
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
        assert!(!shape_pr_files(
            &mut incomplete,
            &mut incomplete_pagination,
            files.clone(),
            incomplete_state,
            &query,
            json!({"files":["src/missing.rs"]}).as_object(),
            "selected",
        ));

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
            json!({"filename":"done.rs","status":"modified","patch":"x"}),
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
        );
        assert_eq!(pagination["patches"]["files"], json!(["a.rs", "b.rs"]));
        let request: GhGetHistoryItemQuery = serde_json::from_value(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":5,
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
            json!({"mode":"selected","files":["a.rs","b.rs"]}),
            "{out}"
        );
        assert_eq!(next["filePage"], 1);
        assert_eq!(next["charOffset"], 2);
        assert!(next.get("collectionPages").is_none());
    }

    #[test]
    fn patch_window_splits_the_page_budget_across_files() {
        assert_eq!(patch_window(None, 1, None), PATCH_DEFAULT_PAGE);
        assert_eq!(patch_window(None, 30, None), PATCH_DEFAULT_PAGE / 30);
        assert_eq!(patch_window(Some(50_000), 30, None), PATCH_PAGE_BUDGET / 30);
        assert_eq!(patch_window(Some(2), 30, None), 2);
        assert_eq!(patch_window(None, 1, Some(50_000)), PATCH_DEFAULT_PAGE);
    }

    /// H4: at `defaultCharLength` 1000 the patch window shrinks with the page,
    /// so a page of patches still fits one automatic response page.
    #[test]
    fn patch_window_derives_from_the_effective_auto_page() {
        assert_eq!(patch_window(None, 1, Some(1_000)), 400);
        assert_eq!(patch_window(None, 4, Some(1_000)), 100);
        assert_eq!(patch_window(Some(5_000), 1, Some(1_000)), 600);
        let mut query = patch_query(0);
        query.char_length = None;
        query.auto_page_chars = Some(1_000);
        let patch = "+x\n".repeat(2_000);
        let shaped = shape_file(
            &json!({"filename":"a.rs","status":"modified","patch":patch}),
            true,
            &query,
            1,
        );
        let text = shaped["patch"].as_str().expect("patch");
        assert!(text.encode_utf16().count() <= 400, "{}", text.len());
        assert_eq!(shaped["patchPagination"]["hasMore"], true);
    }
}
