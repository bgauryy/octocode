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
    assert_eq!(
        lines.out,
        "a.mjs:10  [planChunks 0 readGraph 10]\nb.mjs:1  [planChunks 1 readGraph 0]"
    );
    // A continuation does not name the terms: the counts stay, in order.
    let lines = render(
        ToolId::LocalSearch,
        &counts,
        &json!({"queries":[{"path":"."}]}),
        None,
    )
    .expect("lines");
    assert!(
        lines.out.starts_with("a.mjs:10  [termCounts 0,10]"),
        "{}",
        lines.out
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
            "pagination: unit lines offset 0 length 3 hasMore",
            "matchedLines: 1, 3",
            "hints:",
            "  - Read on with next.nextPage.",
            "empty: \"\"",
            "symbols:",
            "  - name: Title",
            "    line: 1",
            "    doc: two words",
        ]
        .join("\n")
    );
    assert_eq!(
        lines.notes,
        "# next.nextPage: octocode localFetch '{\"cursor\":\"issued-page\"}'"
    );
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
        "## results[0]\na.rs:3:fn a()\n## results[1]\nerrorCode: pathNotFound\nerror: No such path"
    );
    assert!(
        lines.notes.contains("# results[1] status: error"),
        "{}",
        lines.notes
    );
    assert!(
        lines.notes.contains("# responsePagination: hasMore"),
        "{}",
        lines.notes
    );
    for cursor in ["cursor-row", "cursor-envelope"] {
        assert_eq!(lines.notes.matches(cursor).count(), 1, "{}", lines.notes);
    }
    // A continuation that renumbered its rows names each row's own index.
    let resumed = json!({"results":[{"index":2,"data":{"files":[{"path":"b"}]}},{"index":4,"data":{"files":[]}}]});
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
