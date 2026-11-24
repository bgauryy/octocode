use super::*;
use crate::gh_get_history_item::ItemOperation;
use crate::gh_get_history_item::patch_hop::attach_diff_continuations;
use octocode_test_support::json_fixture;

fn source_files() -> Vec<Value> {
    ["alpha", "beta"]
        .into_iter()
        .map(|name| {
            let lines = (1..=20)
                .map(|line| format!("+{name} {line}: café 界😀\n"))
                .collect::<String>();
            json!({"filename":format!("src/{name}.ts"),"status":"added",
                "additions":20,"deletions":0,"patch":format!("@@ -0,0 +1,20 @@\n{lines}")})
        })
        .collect()
}

fn shaped(query: &HistoryItemRequest) -> (Value, FilePage) {
    shaped_files(query, source_files(), None)
}

fn shaped_files(
    query: &HistoryItemRequest,
    files: Vec<Value>,
    new: Option<&str>,
) -> (Value, FilePage) {
    let count = files.len();
    let filtered = FilteredPage {
        files,
        page: octocode_runtime_base::response::pages::PageFacts::counted(0, count, count)
            .to_value(),
        kept: count,
        unsearched: vec![],
        dropped: 0,
        in_scope: count,
        loaded: count,
    };
    let mut out = json!({});
    let page = shape_file_page(
        &mut out,
        filtered,
        query,
        DiffSides { new, old: None },
        || json!({}),
    );
    (out, page)
}

/// A file page whose files all carry their patch leads to no source read:
/// the patch is the evidence itself, whichever files the window shows.
#[test]
fn a_delivered_patch_leads_to_no_source_read() {
    let mut files = source_files();
    let lines = (1..=60)
        .map(|line| format!("+gamma {line}: unseen on page one\n"))
        .collect::<String>();
    files.push(
        json!({"filename":"src/gamma.ts","status":"modified","additions":60,
        "deletions":0,"patch":format!("@@ -4,0 +5,60 @@\n{lines}")}),
    );
    let input = json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "main",
            "sections": ["patches"]
        }"#,
    );
    let mut query = HistoryItemRequest::from_row(input).expect("query");
    query.auto_page_chars = Some(300);
    let (out, page) = shaped_files(&query, files, Some("head"));
    assert!(
        out["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        "{out}"
    );
    assert!(page.side_reads.is_empty(), "{:?}", page.side_reads);
}

#[test]
fn commit_and_compare_patch_windows_keep_complete_stream_facts() {
    for (operation, fields) in [
        (
            ItemOperation::Commit,
            json_fixture(r#"{"operation": "commit", "ref": "main"}"#),
        ),
        (
            ItemOperation::Compare,
            json_fixture(
                r#"{
                    "operation": "compare",
                    "baseRef": "base",
                    "headRef": "head"
                }"#,
            ),
        ),
    ] {
        let input = super::super::util::merge(
            json_fixture(r#"{"owner": "o", "repo": "r", "sections": ["patches"]}"#),
            fields,
        );
        let mut query = HistoryItemRequest::from_row(input).expect("query");
        query.auto_page_chars = Some(300);
        let mut offset = 0;
        let mut windows = 0;
        loop {
            let (mut out, page) = shaped(&query);
            attach_diff_continuations(&mut out, &query, operation, None, page.cursors);
            let facts = &out["sectionPagination"]["patches"];
            assert_eq!(facts["unit"], "chars", "{operation:?}: {out}");
            assert_eq!(facts["offset"], offset, "{out}");
            assert_eq!(
                facts["hasMore"],
                page.cursors.patch_offset.is_some(),
                "{out}"
            );
            let length = out["files"]
                .as_array()
                .expect("files")
                .iter()
                .filter_map(|file| file["patch"].as_str())
                .map(|patch| patch.chars().count())
                .sum::<usize>();
            assert_eq!(facts["length"], length, "{out}");
            windows += 1;
            let Some(next_offset) = page.cursors.patch_offset else {
                assert_eq!(facts["total"], offset + length, "{out}");
                assert!(out["next"].get("patches").is_none(), "{out}");
                break;
            };
            assert_eq!(next_offset, offset + length, "{out}");
            assert!(next_offset > offset, "every window advances: {out}");
            let next = &out["next"]["patches"]["input"]["queries"][0];
            assert_eq!(next["offset"], next_offset, "{out}");
            query = HistoryItemRequest::from_row(next.clone()).expect("continuation query");
            query.auto_page_chars = Some(300);
            offset = next_offset;
        }
        assert!(windows > 1, "fixture exercises multiple windows");
    }
}

/// A patch total counts one file page's stream, as the output reference of
/// `sectionPagination.patches` states once: no page restates it in a
/// notice, whether file pages split or not, on any window.
#[test]
fn a_file_page_restates_no_patch_total_scope() {
    let input = json_fixture(
        r#"{"operation": "compare", "owner": "o", "repo": "r", "baseRef": "a",
            "headRef": "b", "sections": ["patches"]}"#,
    );
    let query = HistoryItemRequest::from_row(input).expect("query");
    let notices = |query: &HistoryItemRequest, start: usize, total: usize| {
        let files = source_files();
        let count = files.len();
        let filtered = FilteredPage {
            files,
            page: octocode_runtime_base::response::pages::PageFacts::counted(start, count, total)
                .to_value(),
            kept: count,
            unsearched: vec![],
            dropped: 0,
            in_scope: count,
            loaded: count,
        };
        let mut out = json!({});
        shape_file_page(
            &mut out,
            filtered,
            query,
            DiffSides {
                new: None,
                old: None,
            },
            || json!({}),
        );
        out["notices"].as_array().cloned().unwrap_or_default()
    };
    for (start, total) in [(0, 270), (4, 6), (0, 2)] {
        assert!(notices(&query, start, total).is_empty(), "{start}/{total}");
    }
    let later = HistoryItemRequest::from_row(json_fixture(
        r#"{"operation": "compare", "owner": "o", "repo": "r", "baseRef": "a",
            "headRef": "b", "sections": ["patches"], "offset": 5}"#,
    ))
    .expect("query");
    assert!(notices(&later, 0, 270).is_empty(), "later window");
}
