//! Symbol outlines, locations and declaration content: compact rows,
//! one-based coordinates and capped bodies.

use super::*;

#[test]
fn document_symbols_are_compact_outline_rows() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs"
    }))
    .expect("document symbols query");
    let range = |start: u64, start_char: u64, end: u64| {
        serde_json::json!({
            "start": {"line": start, "character": start_char},
            "end": {"line": end, "character": 1}
        })
    };
    let symbol = |name: &str, kind: u64, full: (u64, u64), anchor: (u64, u64)| {
        serde_json::json!({
            "name": name,
            "kind": kind,
            "range": range(full.0, 0, full.1),
            "selectionRange": range(anchor.0, anchor.1, anchor.0)
        })
    };
    let mut greeter = symbol("Greeter", 5, (2, 8), (2, 7));
    greeter["children"] = serde_json::json!([symbol("greet", 6, (3, 5), (3, 5))]);
    // Rust `impl` blocks are `object` symbols; their members are listed.
    let mut implementation = symbol("impl Greeter", 19, (16, 20), (16, 5));
    implementation["children"] = serde_json::json!([symbol("new", 12, (17, 19), (17, 7))]);
    // A function's locals are listed under it like any member.
    let mut outer = symbol("outer", 12, (22, 30), (22, 3));
    outer["children"] = serde_json::json!([symbol("x", 13, (23, 23), (23, 8))]);
    let raw = serde_json::json!([
        greeter,
        // Doc comment / attribute lines are part of `range`; the row's line
        // is the name line (`selectionRange`), a usable line.
        symbol("documented", 12, (10, 14), (12, 7)),
        implementation,
        outer,
        // Two symbols on one line carry their 1-based column.
        symbol("a", 14, (32, 32), (32, 6)),
        symbol("b", 14, (32, 32), (32, 13)),
    ]);

    let envelope = items_payload(&query, "documentSymbols", raw.clone(), TEST_BUDGET);
    assert_eq!(envelope["lsp"]["source"], "lsp");
    assert!(envelope.get("summary").is_none(), "{envelope}");
    // Outline rows: members nest under their container, a multi-line
    // declaration states its range, and two symbols on one line carry
    // their 1-based column.
    assert_eq!(
        serde_json::json!({"kind": envelope["kind"], "symbols": envelope["symbols"]}),
        serde_json::json!({
            "kind": "documentSymbols",
            "symbols": [
                {"symbolName":"Greeter","kind":"class","line":3,"endLine":9,
                 "members":[{"symbolName":"greet","kind":"method","line":4,"endLine":6}]},
                {"symbolName":"documented","kind":"function","line":13,"endLine":15},
                {"symbolName":"impl Greeter","kind":"object","line":17,"endLine":21,
                 "members":[{"symbolName":"new","kind":"function","line":18,"endLine":20}]},
                {"symbolName":"outer","kind":"function","line":23,"endLine":31,
                 "members":[{"symbolName":"x","kind":"variable","line":24}]},
                {"symbolName":"a","kind":"constant","line":33,"column":7},
                {"symbolName":"b","kind":"constant","line":33,"column":14}
            ]
        }),
        "{envelope}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&envelope))
        .expect("outline rows satisfy the output contract");
    // Every row is an object, and pages count the six top-level rows: a
    // one-row page keeps a container's members with it.
    assert!(
        envelope["symbols"]
            .as_array()
            .expect("rows")
            .iter()
            .all(|row| row.is_object()
                && row["members"]
                    .as_array()
                    .is_none_or(|members| members.iter().all(serde_json::Value::is_object))),
        "{envelope}"
    );
    assert_eq!(envelope["pagination"]["totalItems"], 6, "{envelope}");
    let paged: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs", "pageSize": 1
    }))
    .expect("paged query");
    let first = items_payload(&paged, "documentSymbols", raw.clone(), TEST_BUDGET);
    assert_eq!(first["pagination"]["totalItems"], 6, "{first}");
    assert_eq!(first["pagination"]["totalPages"], 6, "{first}");
    assert_eq!(
        first["symbols"],
        serde_json::json!([envelope["symbols"][0].clone()]),
        "{first}"
    );
    // CHAIN: an outline leads to localFetch: the first top-level
    // multi-line declaration, name line to last line.
    let read = &envelope["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{envelope}");
    assert_eq!(
        read["query"]["queries"][0],
        serde_json::json!({"path": "file:///repo/src/lib.rs", "ranges": ["3-9"]}),
        "{envelope}"
    );
}

#[test]
fn type_aliases_are_named_from_their_declaring_line() {
    let range = |line: u64| serde_json::json!({"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}});
    let symbol = |name: &str, line: u64| serde_json::json!({"name": name, "kind": 13, "range": range(line), "selectionRange": range(line)});
    let mut outer = symbol("holder", 3);
    outer["children"] = serde_json::json!([symbol("Inner", 4)]);
    let mut symbols = serde_json::json!([
        symbol("Alias", 0),
        symbol("Exported", 1),
        symbol("value", 2),
        outer,
        symbol("Typed", 5),
    ]);
    let content = "type Alias = string;\nexport declare type Exported<T> = T;\nconst value = 1;\nconst holder = () => {\n  type Inner = number;\ntype TypedLonger = 1;\n";
    super::super::render::name_type_aliases(&mut symbols, content);
    let kinds: Vec<_> = [
        &symbols[0],
        &symbols[1],
        &symbols[2],
        &symbols[3],
        &symbols[3]["children"][0],
        &symbols[4],
    ]
    .iter()
    .map(|symbol| symbol["kind"].clone())
    .collect();
    assert_eq!(
        kinds,
        [
            serde_json::json!("type"),
            serde_json::json!("type"),
            serde_json::json!(13),
            serde_json::json!(13),
            serde_json::json!("type"),
            serde_json::json!(13)
        ]
    );
}

#[test]
fn locations_are_compact_camel_case_and_one_based() {
    let location = compact_location(serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {
            "start": {"line": 5, "character": 3},
            "end": {"line": 5, "character": 8}
        },
        "content": "pub fn greet() {}\n",
        "symbol_kind": "function"
    }));
    assert_eq!(location["uri"], "file:///repo/src/lib.rs");
    assert_eq!(location["range"]["start"]["line"], 5);
    assert_eq!(
        location["displayRange"],
        serde_json::json!({
            "startLine": 6,
            "endLine": 6
        })
    );
    assert!(location.get("symbolKind").is_none());
    assert!(location.get("path").is_none());
    assert!(location.get("line").is_none());

    let public = public_location(location);
    assert!(public.get("range").is_none(), "one coordinate form only");
    assert_eq!(
        public["displayRange"],
        serde_json::json!({"startLine": 6, "startCharacter": 4, "endLine": 6})
    );
    assert!(public.get("contentStartLine").is_none());
    assert_eq!(public["content"], "6\tpub fn greet() {}\n", "{public}");
}

#[test]
fn widened_content_reports_its_first_line_and_single_file_pages_hoist_the_path() {
    let widened = public_location(serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {
            "start": {"line": 5, "character": 3},
            "end": {"line": 5, "character": 8}
        },
        "content": "a\nb\nc\n",
        "displayRange": {"startLine": 5, "endLine": 7}
    }));
    assert_eq!(widened["displayRange"]["startLine"], 6);
    // The gutter states where widened content begins.
    assert_eq!(widened["content"], "5\ta\n6\tb\n7\tc\n", "{widened}");
    assert!(widened.get("contentStartLine").is_none(), "{widened}");
}

#[test]
fn server_controlled_item_uris_are_authorized_before_emission() {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    // A policy with no configured roots authorizes no real path, so any
    // server-supplied file URI must be rejected.
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");

    let workspace_symbol =
        serde_json::json!({ "name": "Foo", "location": { "uri": "file:///etc/passwd" } });
    assert!(!item_uri_is_authorized(&workspace_symbol, &paths));

    let call = serde_json::json!({ "from": { "uri": "file:///etc/hosts" } });
    assert!(!item_uri_is_authorized(&call, &paths));

    let type_item = serde_json::json!({ "name": "Base", "uri": "file:///etc/group" });
    assert!(!item_uri_is_authorized(&type_item, &paths));

    // An item that embeds no file URI carries no path to leak and is kept.
    let no_uri = serde_json::json!({ "name": "Local" });
    assert!(item_uri_is_authorized(&no_uri, &paths));

    // filter_authorized_items removes only the unauthorized entries and leaves
    // non-array values untouched for downstream `as_array` handling.
    let filtered = filter_authorized_items(serde_json::json!([workspace_symbol, no_uri]), &paths);
    assert_eq!(filtered.as_array().map(Vec::len), Some(1));
    assert_eq!(filtered[0]["name"], "Local");
    assert_eq!(
        filter_authorized_items(serde_json::Value::Null, &paths),
        serde_json::Value::Null
    );
}

#[test]
fn long_declaration_content_is_capped_with_a_marker_naming_the_omitted_lines() {
    let body = (0..200)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut location = serde_json::json!({
        "uri": "file:///repo/a.ts",
        "content": body,
        "displayRange": {"startLine": 426, "endLine": 625}
    });
    let rest = cap_declaration_content(&mut location).expect("a read of the omitted lines");
    let content = location["content"].as_str().expect("content");
    let lines = content.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), MAX_DECLARATION_CONTENT_LINES + 1);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[MAX_DECLARATION_CONTENT_LINES - 1], "line 59");
    assert_eq!(
        lines[MAX_DECLARATION_CONTENT_LINES],
        "… 140 more lines omitted (source lines 486-625)."
    );
    // The omitted lines are one executable read, each line once.
    assert_eq!(rest["tool"], "localFetch");
    assert_eq!(
        rest["query"]["queries"][0],
        serde_json::json!({"path": "/repo/a.ts", "ranges": ["486-625"]})
    );
    let short = "a\nb\nc";
    let mut small =
        serde_json::json!({"content": short, "displayRange": {"startLine": 1, "endLine": 3}});
    assert!(cap_declaration_content(&mut small).is_none());
    assert_eq!(small["content"], short, "short bodies are untouched");
}

#[test]
fn hover_range_is_published_as_a_one_based_display_range() {
    let hover = public_hover(serde_json::json!({
        "contents": {"kind": "markdown", "value": "```ts\nconst a: number\n```"},
        "range": {"start": {"line": 3, "character": 8}, "end": {"line": 4, "character": 2}}
    }));
    assert!(hover.get("range").is_none(), "{hover}");
    assert_eq!(
        hover["displayRange"],
        serde_json::json!({"startLine": 4, "startCharacter": 9, "endLine": 5}),
        "{hover}"
    );
    assert_eq!(hover["contents"]["kind"], "markdown", "{hover}");
    let bare = public_hover(serde_json::json!({"contents": "text"}));
    assert_eq!(bare, serde_json::json!({"contents": "text"}));
    crate::contracts::validate_output(
        "lspSearch",
        &public_row(&serde_json::json!({
            "lsp": {"serverAvailable": true, "provider": "hoverProvider"},
            "kind": "hover", "hover": hover
        })),
    )
    .expect("published hover satisfies the internal output contract");
}

/// A capped declaration body numbers its source lines; the trailing
/// omission marker is not a source line and stays unnumbered.
#[test]
fn capped_declaration_content_numbers_lines_but_not_the_marker() {
    let body = (1..=70).map(|n| format!("line {n}\n")).collect::<String>();
    let mut location = serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {"start": {"line": 9, "character": 0}, "end": {"line": 78, "character": 1}},
        "content": body,
        "displayRange": {"startLine": 10, "endLine": 79}
    });
    super::super::locations::cap_declaration_content(&mut location).expect("read of the rest");
    let public = public_location(location);
    let content = public["content"].as_str().expect("content");
    assert!(content.starts_with("10\tline 1\n"), "{content}");
    assert!(content.contains("\n69\tline 60\n… "), "{content}");
    assert!(
        content
            .lines()
            .last()
            .is_some_and(|marker| marker.starts_with("… ")),
        "{content}"
    );
}
