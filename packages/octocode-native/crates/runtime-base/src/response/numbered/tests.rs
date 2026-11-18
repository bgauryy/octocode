use super::*;

/// Whether `content` is in numbered form: `N\t` lines in ascending
/// order, consecutive except across a [`GAP`] line, which stands for the
/// lines between its neighbours' numbers.
fn is_numbered(content: &str) -> bool {
    let mut expected: Option<u64> = None;
    let mut gap = false;
    let mut numbered = 0usize;
    for record in content.split_inclusive('\n') {
        if record.trim_end_matches(['\n', '\r']) == GAP.trim_end() {
            if gap {
                return false;
            }
            gap = true;
            continue;
        }
        let Some((number, _)) = record.split_once(SEPARATOR) else {
            return false;
        };
        let Ok(line) = number.parse::<u64>() else {
            return false;
        };
        if line == 0 || number.starts_with('0') {
            return false;
        }
        match expected {
            Some(next) if gap && line <= next => return false,
            Some(next) if !gap && line != next => return false,
            _ => {}
        }
        gap = false;
        expected = Some(line + 1);
        numbered += 1;
    }
    numbered > 0
}
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn outlines_use_one_tab_gutter() {
    assert_eq!(
        tab_gutter("3 import x\n4\tfn a() {\n12}\nplain\n"),
        "3\timport x\n4\tfn a() {\n12\t}\nplain\n"
    );
}

#[test]
fn numbers_each_line_with_a_tab_gutter_and_passes_markers_through() {
    assert_eq!(
        number_lines("a\nb\n", &[(4, 5)]).as_deref(),
        Some("4\ta\n5\tb\n")
    );
    assert_eq!(
        number_lines(
            "a\n... [lines 3-8 not requested] ...\nb\n",
            &[(2, 2), (9, 9)]
        )
        .as_deref(),
        Some("2\ta\n--\n9\tb\n")
    );
    // Last line without a trailing newline keeps that shape.
    assert_eq!(
        number_lines("x\ny", &[(10, 11)]).as_deref(),
        Some("10\tx\n11\ty")
    );
}

#[test]
fn rejects_content_that_does_not_map_onto_the_ranges() {
    assert_eq!(number_lines("a\nb\n", &[(4, 4)]), None);
    assert_eq!(number_lines("a\n", &[(0, 1)]), None);
    assert_eq!(number_lines("a\nb\nc\n", &[(1, 5)]), None);
    assert_eq!(number_lines("", &[(1, 1)]), None);
    assert_eq!(number_lines("a\n", &[]), None);
    // Byte windows are not source lines.
    assert_eq!(
        number_lines("xx\n... [3000 bytes omitted] ...\n", &[(1, 2)]),
        None
    );
}

#[test]
fn recognizes_numbered_content_only() {
    assert!(is_numbered("4\ta\n5\tb\n"));
    assert!(is_numbered("2\ta\n--\n9\tb\n"));
    assert!(is_numbered("7\t"));
    assert!(!is_numbered("4\ta\n6\tb\n"));
    assert!(!is_numbered("a\tb\n"));
    assert!(!is_numbered("0\ta\n"));
    assert!(!is_numbered(""));
    // A gap must skip at least one line, and gaps never stack.
    assert!(!is_numbered("2\ta\n--\n3\tb\n"));
    // A page cut at a gap still numbers: its edge markers drop.
    assert_eq!(
            number_lines(
                "... [lines 1-9 not requested] ...\nten\n... [lines 11-19 not requested] ...\ntwenty\n... [lines 21-29 not requested] ...\n",
                &[(10, 10), (20, 20)]
            )
            .as_deref(),
            Some("10\tten\n--\n20\ttwenty\n")
        );
    // The compact view skips the lines it drops: still a source gutter.
    let compact = |content: &str| gutter_numbered(&serde_json::json!({"content": content}));
    assert!(compact("4\ta\n9\tb\n--\n20\tc\n"));
    assert!(!compact("4\ta\n3\tb\n"));
    assert!(!compact("plain\n"));
    assert!(!is_numbered("2\ta\n--\n--\n9\tb\n"));
}

/// A grep map of single lines separates its windows with one `--` line
/// each; the numbers state every skipped span, so the returned lines and
/// ranges round-trip exactly.
#[test]
fn single_line_windows_use_one_gap_line_and_round_trip() {
    let ranges = [(3, 3), (10, 10), (20, 23), (40, 40), (50, 50)];
    let content = "c\n... [lines 4-9 not requested] ...\nj\n... [lines 11-19 not requested] ...\nt\nu\nv\nw\n... [lines 24-39 not requested] ...\nN\n... [lines 41-49 not requested] ...\nX\n";
    let numbered = number_lines(content, &ranges).expect("numbered");
    assert_eq!(
        numbered,
        "3\tc\n--\n10\tj\n--\n20\tt\n21\tu\n22\tv\n23\tw\n--\n40\tN\n--\n50\tX\n"
    );
    assert!(is_numbered(&numbered));
    let mut restored: Vec<(u64, u64)> = Vec::new();
    for record in numbered.lines() {
        let Some((number, _)) = record.split_once(SEPARATOR) else {
            continue;
        };
        let line: u64 = number.parse().expect("number");
        match restored.last_mut() {
            Some((_, end)) if *end + 1 == line => *end = line,
            _ => restored.push((line, line)),
        }
    }
    assert_eq!(restored, ranges);
}

/// A localFetch grep map (single-line matchString windows) gets one
/// `--` line per gap between windows, and its numbers restore every line.
#[test]
fn local_fetch_single_line_windows_share_gap_markers_and_round_trip() {
    let mut local = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "path": "a.rs",
                        "content": "c\n... [lines 4-9 not requested] ...\nj\n... [lines 11-19 not requested] ...\nt\n",
                        "sourceLineRanges": [
                            {"line": 3, "endLine": 3},
                            {"line": 10, "endLine": 10},
                            {"line": 20, "endLine": 20}
                        ],
                        "matchedLines": [3, 10, 20]
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut local);
    let content = local["results"][0]["data"]["content"]
        .as_str()
        .expect("content");
    assert_eq!(content, "3\tc\n--\n10\tj\n--\n20\tt\n");
    assert!(is_numbered(content));
    let lines = content
        .lines()
        .filter_map(|record| record.split_once(SEPARATOR))
        .map(|(number, _)| number.parse::<u64>().expect("number"))
        .collect::<Vec<_>>();
    assert_eq!(lines, [3, 10, 20]);
}

#[test]
fn numbered_file_pages_preserve_match_identity() {
    for tool in [ToolId::LocalFetch, ToolId::GhGetFileContent] {
        for matched in [json!([]), json_fixture(r#"[7]"#), json_fixture(r#"[7, 8]"#)] {
            let mut page = json!({"results":[{"index":0,"data":{
                "content":"same\nsame\n", "sourceLineRanges":[{"line":7,"endLine":8}],
                "matchedLines":matched, "selectedMatchCount":2}}]});
            number_read_rows(tool, &mut page);
            let data = &page["results"][0]["data"];
            assert_eq!(data["matchedLines"], matched, "{tool:?}: {page}");
            assert_eq!(data["content"], "7\tsame\n8\tsame\n");
        }
    }
}

#[test]
fn numbers_local_and_github_rows_and_drops_redundant_anchors() {
    let mut local = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "path": "a.rs",
                        "content": "x\ny\n",
                        "sourceLineRanges": [{"line": 7, "endLine": 8}],
                        "matchedLines": [7, 8]
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut local);
    assert_eq!(
        local["results"][0]["data"],
        json_fixture(
            r#"{
                "path": "a.rs",
                "content": "7\tx\n8\ty\n",
                "matchedLines": [7, 8]
            }"#
        )
    );
    let mut window = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "content": "x\ny\n",
                        "sourceLineRanges": [{"line": 7, "endLine": 8}],
                        "matchedLines": [8]
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut window);
    assert_eq!(
        window["results"][0]["data"]["matchedLines"],
        json_fixture(r#"[8]"#)
    );
    // Two windows: the gutter states the first, last and returned lines.
    let mut windows = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "content": "x\n... [lines 8-9 not requested] ...\ny\n",
                        "sourceLineRanges": [{"line": 7, "endLine": 7}, {"line": 10, "endLine": 10}],
                        "startLine": 7,
                        "endLine": 10,
                        "returnedLines": 3,
                        "totalLines": 12
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut windows);
    assert_eq!(
        windows["results"][0]["data"],
        json_fixture(r#"{"content": "7\tx\n--\n10\ty\n", "totalLines": 12}"#)
    );
    let mut remote = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "owner": "o",
                        "repo": "r",
                        "path": "a.py",
                        "content": "x\n",
                        "sourceLineRanges": [{"line": 3, "endLine": 3}],
                        "startLine": 3,
                        "endLine": 3,
                        "returnedLines": 1
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "path": "b.py",
                        "content": "def a\n",
                        "contentView": "symbols"
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::GhGetFileContent, &mut remote);
    assert_eq!(
        remote["results"][0]["data"],
        json_fixture(
            r#"{
                "owner": "o",
                "repo": "r",
                "path": "a.py",
                "content": "3\tx\n"
            }"#
        ),
        "the gutter states the window"
    );
    assert_eq!(
        remote["results"][1]["data"]["content"], "def a\n",
        "transformed views stay verbatim"
    );
    // Byte pages stay verbatim: their offsets count the returned text.
    let mut bytes = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "content": "x\n",
                        "sourceLineRanges": [{"line": 1, "endLine": 1}],
                        "pagination": {"unit": "bytes", "offset": 0, "length": 2, "hasMore": true}
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut bytes);
    assert_eq!(bytes["results"][0]["data"]["content"], "x\n");
    // Other tools are untouched.
    let mut other = local.clone();
    number_read_rows(ToolId::LocalSearch, &mut other);
    assert_eq!(other, local);
}

/// Every omission marker kind reads back as its span, so a classified
/// read whose selected chunks are not adjacent keeps its gutter.
#[test]
fn every_omission_marker_kind_keeps_the_gutter() {
    for why in [Omitted::NotRequested, Omitted::NotSelected] {
        assert_eq!(omission_span(&omission_marker(4, 4, why)), Some((4, 4)));
        assert_eq!(omission_span(&omission_marker(3, 8, why)), Some((3, 8)));
    }
    assert_eq!(omission_span("... [lines 3-8 not wanted] ...\n"), None);
    let content = format!(
        "x\n{}y\n",
        omission_marker(1153, 2538, Omitted::NotSelected)
    );
    let mut response = json!({"results":[{"data":{
        "content":content,
        "sourceLineRanges":[{"line":1152,"endLine":1152},{"line":2539,"endLine":2539}],
        "selection":{"status":"complete","selectedItems":2,"totalItems":3}
    }}]});
    number_read_rows(ToolId::LocalFetch, &mut response);
    assert_eq!(
        response["results"][0]["data"]["content"],
        "1152\tx\n--\n2539\ty\n"
    );
}

#[test]
fn ranked_read_numbers_backwards_source_windows_and_keeps_gap_markers() {
    let mut response = json_fixture(
        r#"{
            "results": [
                {
                    "data": {
                        "content": "late\n... [lines 8-9 not requested] ...\nearlier\n",
                        "sourceLineRanges": [{"line": 10, "endLine": 10}, {"line": 3, "endLine": 3}],
                        "selection": {"status": "complete", "selectedItems": 2, "totalItems": 2}
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut response);
    let data = &response["results"][0]["data"];
    assert_eq!(data["content"], "10\tlate\n--\n3\tearlier\n");
}

/// A symbols outline states each declaration's span in its gutter (`N-M`),
/// so every page of it drops the window fields the gutter repeats, as a page
/// of single-line rows does: `returnedLines` is the gutter's row count on
/// every page, never on some.
#[test]
fn a_symbols_outline_page_states_its_lines_in_the_gutter_on_every_page() {
    for content in [
        "10\tuse … (lines 10-14)\n18-25\tpub struct A {\n20\t    a: u8,\n27-41\timpl A {\n27-30\t    fn a() {}\n",
        "325\tmod tests;",
    ] {
        let mut page = json!({"results":[{"index":0,"data":{
                "path":"a.rs","view":"symbols","content":content,"totalLines":325,
                "returnedLines":content.lines().count(),
                "pagination":{"unit":"lines","offset":0,"length":content.lines().count(),"total":43,"hasMore":true}}}]});
        number_read_rows(ToolId::LocalFetch, &mut page);
        let data = &page["results"][0]["data"];
        assert!(data.get("returnedLines").is_none(), "{data}");
        assert_eq!(data["content"], content, "the outline stays verbatim");
    }
    // A span that ends before it starts is no gutter.
    let mut broken = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "view": "symbols",
                        "content": "9-3\tfn a() {}\n",
                        "returnedLines": 1
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut broken);
    assert_eq!(broken["results"][0]["data"]["returnedLines"], 1);
}

/// Only a span gutter (`N-M`) may start on the previous line: a repeated
/// plain number is a malformed view, not a numbered one.
#[test]
fn a_repeated_plain_line_number_is_no_gutter() {
    let mut repeated = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "view": "compact",
                        "content": "5\tfn a() {}\n5\tfn b() {}\n",
                        "returnedLines": 2
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut repeated);
    assert_eq!(repeated["results"][0]["data"]["returnedLines"], 2);
    // A plain number after a span that starts on its line is no gutter either.
    let mut after_span = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "view": "symbols",
                        "content": "5-9\tfn a() {}\n5\tfn b() {}\n",
                        "returnedLines": 2
                    }
                }
            ]
        }"#,
    );
    number_read_rows(ToolId::LocalFetch, &mut after_span);
    assert_eq!(after_span["results"][0]["data"]["returnedLines"], 2);
}
