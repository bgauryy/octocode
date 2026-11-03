use super::*;
use crate::tools::gh_get_history_item::ItemOperation;
use crate::tools::gh_get_history_item::patch_hop::attach_diff_continuations;

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
        page: crate::response::pages::PageFacts::open(1, Some(count), false)
            .with_total(count)
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

/// The read lead names a file the patch window shows, bounded to the
/// hunk's lines: the filter keeps more files than one window packs, and the
/// most-changed file past the window is no lead of this page.
#[test]
fn side_read_leads_name_a_shown_file_over_its_hunk_window() {
    let mut files = source_files();
    let lines = (1..=60)
        .map(|line| format!("+gamma {line}: unseen on page one\n"))
        .collect::<String>();
    files.push(
        json!({"filename":"src/gamma.ts","status":"modified","additions":60,
        "deletions":0,"patch":format!("@@ -4,0 +5,60 @@\n{lines}")}),
    );
    let input =
        json!({"operation":"commit","owner":"o","repo":"r","ref":"main","sections":["patches"]});
    let mut query = HistoryItemRequest::from_row(input).expect("query");
    query.auto_page_chars = Some(300);
    let (out, page) = shaped_files(&query, files, Some("head"));
    let shown = out["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(!shown.contains(&"src/gamma.ts"), "window cuts gamma: {out}");
    let [(name, read)] = page.side_reads.as_slice() else {
        panic!("one lead: {:?}", page.side_reads);
    };
    assert_eq!(*name, "readAtCommit");
    let row = &read["input"]["queries"][0];
    assert!(
        shown.contains(&row["path"].as_str().expect("path")),
        "{read}"
    );
    assert_eq!(
        row["ranges"],
        json!([{"startLine": 1, "endLine": 20}]),
        "{read}"
    );
}

#[test]
fn commit_and_compare_patch_windows_keep_complete_stream_facts() {
    for (operation, fields) in [
        (
            ItemOperation::Commit,
            json!({"operation":"commit","ref":"main"}),
        ),
        (
            ItemOperation::Compare,
            json!({"operation":"compare","baseRef":"base","headRef":"head"}),
        ),
    ] {
        let input = super::super::util::merge(
            json!({"owner":"o","repo":"r","sections":["patches"]}),
            fields,
        );
        let mut query = HistoryItemRequest::from_row(input).expect("query");
        query.auto_page_chars = Some(300);
        let mut offset = 0;
        let mut windows = 0;
        loop {
            let (mut out, page) = shaped(&query);
            attach_diff_continuations(&mut out, &query, operation, None, page.cursors);
            let facts = &out["contentPagination"]["patches"];
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
                assert!(facts.get("next").is_none(), "{out}");
                break;
            };
            assert_eq!(next_offset, offset + length, "{out}");
            assert!(next_offset > offset, "every window advances: {out}");
            let next = &facts["next"]["nextPage"]["input"]["queries"][0];
            assert_eq!(next["offset"], next_offset, "{out}");
            query = HistoryItemRequest::from_row(next.clone()).expect("continuation query");
            query.auto_page_chars = Some(300);
            offset = next_offset;
        }
        assert!(windows > 1, "fixture exercises multiple windows");
    }
}
