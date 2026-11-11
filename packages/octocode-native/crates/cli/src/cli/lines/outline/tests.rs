use super::super::render;
use octocode_native::id::ToolId;
use serde_json::{Value, json};

/// Every string and number leaf of `value`.
fn leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => fields.values().for_each(|value| leaves(value, out)),
        Value::Array(items) => items.iter().for_each(|item| leaves(item, out)),
        Value::String(text) => out.extend(text.split('\n').map(str::to_owned)),
        Value::Number(number) => out.push(number.to_string()),
        _ => {}
    }
}

#[test]
fn count_lines_name_each_terms_count() {
    let counts = json!({"results":[{"index":0,"data":{"matchCount":11,"files":[
        {"path":"a.mjs","matchCount":10,"termCounts":[0,10]},
        {"path":"b.mjs","matchCount":1,"termCounts":[1,0]}]}}]});
    let asked = json!({"queries":[{"path":".","matchString":["planChunks","readGraph"],"view":"countMatches"}]});
    let lines = render(ToolId::LocalSearch, &counts, &asked, None).expect("lines");
    // Every count is a named field, the terms' in `matchString` order as
    // the result carries them: `path:N` is only ever a line.
    assert_eq!(
        lines.out,
        "a.mjs  [matchCount 10 termCounts 0,10]\nb.mjs  [matchCount 1 termCounts 1,0]"
    );
}

#[test]
fn a_tool_without_a_line_format_prints_every_field_as_text() {
    let fetched = json!({"results":[{"index":0,"data":{
        "path":"README.md","content":"1\t# Title\n2\t\n3\tbody, with: colons","totalLines":120,
        "pagination":{"unit":"lines","offset":0,"length":3,"hasMore":true},
        "matchedLines":[1,3],"hints":["Read on with next.nextPage."],"empty":"",
        "symbols":[{"name":"Title","line":1,"doc":"two words"}],
        "next":{"nextPage":{"kind":"continue","tool":"localFetch","input":{"cursor":"issued-page"}}}}}]});
    let lines = render(ToolId::LocalFetch, &fetched, &json!({}), None).expect("text");
    assert_eq!(
        lines.out,
        [
            "path: README.md",
            "content: |",
            "  1\t# Title",
            "  2\t",
            "  3\tbody, with: colons",
            "totalLines: 120",
            "matchedLines: 1, 3",
            "empty: \"\"",
            "symbols[]: name line doc",
            "  Title 1 two words",
            "# pagination: unit lines offset 0 length 3 hasMore",
            "# hints: Read on with next.nextPage.",
            "# next.nextPage: octocode localFetch '{\"cursor\":\"issued-page\"}'",
        ]
        .join("\n")
    );
    assert_eq!(lines.notes, "");
}

#[test]
fn every_tool_prints_text_and_loses_no_leaf() {
    let deep = json!({"results":[{"index":0,"data":{"repositories":[{"owner":"o","repo":"r",
        "topics":["a b","c"],"stars":3,"nested":{"list":[[1,2],["x\ny"]],"deep":{"k":"two words"}}}],
        "pagination":{"page":1,"totalPages":2,"totalItems":4,"hasMore":true}}}]});
    let mut expected = Vec::new();
    leaves(&deep["results"][0]["data"], &mut expected);
    for tool in ToolId::ALL {
        let Some(lines) = render(tool, &deep, &json!({}), None) else {
            continue;
        };
        for leaf in &expected {
            assert!(
                lines.out.contains(leaf.as_str()),
                "{tool:?} lost {leaf}: {}",
                lines.out
            );
        }
    }
}

#[test]
fn a_batch_prints_each_row_under_its_heading_in_its_line_format() {
    let two = json!({"results":[
        {"index":0,"data":{"files":[{"path":"a.rs","matches":[{"line":3,"value":"fn a()"}]}],
            "next":{"nextPage":{"kind":"continue","tool":"localSearch","input":{"cursor":"cursor-row"}}}}},
        {"index":1,"status":"error","data":{"errorCode":"pathNotFound","error":"No such path"}}],
        "responsePagination":{"hasMore":true,"next":{"kind":"continue","tool":"localSearch","input":{"cursor":"cursor-envelope"}}}});
    let asked =
        json!({"queries":[{"path":".","matchString":["a"]},{"path":"nope","matchString":["a"]}]});
    let lines = render(ToolId::LocalSearch, &two, &asked, None).expect("text");
    assert_eq!(
        lines.out,
        [
            "## results[0]",
            "a.rs:3:fn a()",
            "# next.nextPage: octocode localSearch '{\"cursor\":\"cursor-row\"}'",
            "## results[1]",
            "errorCode: pathNotFound",
            "error: No such path",
            "# status: error",
            "# responsePagination: hasMore",
            "# responsePagination.next: octocode localSearch '{\"cursor\":\"cursor-envelope\"}'",
        ]
        .join("\n")
    );
    assert_eq!(lines.notes, "");
    // A continuation that renumbered its rows names each row's own index.
    let resumed = json!({"results":[{"index":2,"data":{"files":[{"dir":"b","files":["c"]}]}},{"index":4,"data":{"files":[]}}]});
    let lines = render(
        ToolId::StructureSearch,
        &resumed,
        &json!({"cursor":"c"}),
        None,
    )
    .expect("text");
    assert!(
        lines.out.starts_with("## results[0] index 2\n"),
        "{}",
        lines.out
    );
}

/// A list of flat rows with at most one free-text field is a table: the
/// columns named once, identity first and the free text last, a field no
/// row lacks with one word on every row stated once, `-` where a row lacks
/// a field. Every row of the list uses the one layout.
#[test]
fn flat_rows_print_as_one_table() {
    let fetched = json!({"results":[{"index":0,"data":{"commits":[
        {"sha":"abc","date":"2026-01-01","author":"ada","state":"open"},
        {"sha":"def","messageHeadline":"fix: two words","date":"2026-01-02","state":"open"},
        {"sha":"fed","date":"2026-01-03","author":"bob","state":"open"}]}}]});
    let lines = render(ToolId::LocalFetch, &fetched, &json!({}), None).expect("text");
    assert_eq!(
        lines.out,
        [
            "commits[]: sha date author messageHeadline  [state open]",
            "  abc 2026-01-01 ada -",
            "  def 2026-01-02 - fix: two words",
            "  fed 2026-01-03 bob -",
        ]
        .join("\n")
    );
    // Rows of single words alone are a table too; one row states nothing
    // once.
    let words = json!({"results":[{"index":0,"data":{"rows":[{"id":"a.rs","fanIn":2}]}}]});
    let lines = render(ToolId::LocalFetch, &words, &json!({}), None).expect("text");
    assert_eq!(lines.out, "rows[]: id fanIn\n  a.rs 2");
}

/// Two free-text fields, a nested value, a multi-line value or a value that
/// reads as the absent mark keep the `key: value` layout, for every row.
#[test]
fn rows_a_table_cannot_hold_keep_their_fields() {
    for row in [
        json!({"number":1,"title":"two words","author":"Ada Lovelace"}),
        json!({"number":1,"labels":["bug"]}),
        json!({"number":1,"body":"line\nnext"}),
        json!({"number":1,"state":"-"}),
    ] {
        let fetched =
            json!({"results":[{"index":0,"data":{"items":[{"number":2,"state":"open"},row]}}]});
        let out = render(ToolId::LocalFetch, &fetched, &json!({}), None)
            .expect("text")
            .out;
        assert!(out.starts_with("items:\n  - "), "{out}");
        assert!(!out.contains("[]:"), "{out}");
    }
}

/// A count that is only the rows loaded so far prints as a floor; an exact
/// total keeps its name.
#[test]
fn a_loaded_count_prints_as_a_floor() {
    let page = |extra: Value| {
        let mut pagination = json!({"page":1,"hasMore":true,"totalItems":2});
        pagination
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let fetched = json!({"results":[{"index":0,"data":{"pagination":pagination}}]});
        render(ToolId::LocalFetch, &fetched, &json!({}), None)
            .expect("text")
            .out
    };
    let floor = page(json!({"countScope":"loaded"}));
    assert!(floor.contains("totalItemsAtLeast 2"), "{floor}");
    assert!(floor.contains("countScope loaded"), "{floor}");
    let exact = page(json!({}));
    assert!(
        exact.contains("totalItems 2") && !exact.contains("AtLeast"),
        "{exact}"
    );
}
