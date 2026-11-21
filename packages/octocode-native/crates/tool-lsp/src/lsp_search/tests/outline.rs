//! Symbol outlines, locations and declaration content: compact rows,
//! one-based coordinates and capped bodies.

use super::*;
use octocode_test_support::json_fixture;

#[test]
fn document_symbols_are_compact_outline_rows() {
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "documentSymbols",
            "path": "file:///repo/src/lib.rs"
        }"#,
    ))
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
    assert!(envelope["lsp"].get("source").is_none(), "{envelope}");
    assert!(envelope.get("summary").is_none(), "{envelope}");
    // Outline rows: members nest under their container, a multi-line
    // declaration states its range, and two symbols on one line carry
    // their 1-based column.
    assert_eq!(
        serde_json::json!({"operation": envelope["operation"], "symbols": envelope["symbols"]}),
        json_fixture(
            r#"{
                "operation": "documentSymbols",
                "symbols": [
                    {
                        "symbolName": "Greeter",
                        "kind": "class",
                        "line": 3,
                        "endLine": 9,
                        "members": [
                            {
                                "symbolName": "greet",
                                "kind": "method",
                                "line": 4,
                                "endLine": 6
                            }
                        ]
                    },
                    {
                        "symbolName": "documented",
                        "kind": "function",
                        "line": 13,
                        "endLine": 15
                    },
                    {
                        "symbolName": "impl Greeter",
                        "kind": "object",
                        "line": 17,
                        "endLine": 21,
                        "members": [
                            {
                                "symbolName": "new",
                                "kind": "function",
                                "line": 18,
                                "endLine": 20
                            }
                        ]
                    },
                    {
                        "symbolName": "outer",
                        "kind": "function",
                        "line": 23,
                        "endLine": 31,
                        "members": [{"symbolName": "x", "kind": "variable", "line": 24}]
                    },
                    {
                        "symbolName": "a",
                        "kind": "constant",
                        "line": 33,
                        "column": 7
                    },
                    {
                        "symbolName": "b",
                        "kind": "constant",
                        "line": 33,
                        "column": 14
                    }
                ]
            }"#
        ),
        "{envelope}"
    );
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&envelope))
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
    // The total counts the rows served, each member under its container
    // included: what a walk of the outline delivers.
    fn served(rows: &serde_json::Value) -> u64 {
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| 1 + row.get("members").map_or(0, served))
            .sum()
    }
    assert_eq!(served(&envelope["symbols"]), 9, "{envelope}");
    assert_eq!(
        envelope["pagination"]["total"],
        served(&envelope["symbols"]),
        "{envelope}"
    );
    assert_eq!(
        envelope["pagination"]["length"],
        served(&envelope["symbols"]),
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
    let location = compact_location(json_fixture(
        r#"{
            "uri": "file:///repo/src/lib.rs",
            "range": {
                "start": {"line": 5, "character": 3},
                "end": {"line": 5, "character": 8}
            },
            "value": "pub fn greet() {}\n",
            "symbol_kind": "function"
        }"#,
    ));
    assert_eq!(location["uri"], "file:///repo/src/lib.rs");
    assert_eq!(location["range"]["start"]["line"], 5);
    assert_eq!(
        location["displayRange"],
        json_fixture(r#"{"startLine": 6, "endLine": 6}"#)
    );
    assert!(location.get("symbolKind").is_none());
    assert!(location.get("path").is_none());
    assert!(location.get("line").is_none());

    let public = public_location(location);
    assert!(public.get("range").is_none(), "one coordinate form only");
    assert_eq!(
        (&public["line"], &public["column"], public.get("endLine")),
        (&serde_json::json!(6), &serde_json::json!(4), None),
        "{public}"
    );
    assert!(public.get("contentStartLine").is_none());
    // One line at the row's own line carries no number prefix.
    assert_eq!(public["value"], "pub fn greet() {}", "{public}");
}

#[test]
fn widened_content_reports_its_first_line_and_single_file_pages_hoist_the_path() {
    let widened = public_location(json_fixture(
        r#"{
            "uri": "file:///repo/src/lib.rs",
            "range": {
                "start": {"line": 5, "character": 3},
                "end": {"line": 5, "character": 8}
            },
            "value": "a\nb\nc\n",
            "displayRange": {"startLine": 5, "endLine": 7}
        }"#,
    ));
    assert_eq!(widened["line"], 6);
    // The gutter states where widened content begins.
    assert_eq!(widened["value"], "5\ta\n6\tb\n7\tc", "{widened}");
    assert!(widened.get("contentStartLine").is_none(), "{widened}");
}

#[test]
fn server_controlled_item_uris_are_authorized_before_emission() {
    use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
    // A policy with no configured roots authorizes no real path, so any
    // server-supplied file URI must be rejected.
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");

    let workspace_symbol =
        json_fixture(r#"{"name": "Foo", "location": {"uri": "file:///etc/passwd"}}"#);
    assert!(!item_uri_is_authorized(&workspace_symbol, &paths));

    let call = json_fixture(r#"{"from": {"uri": "file:///etc/hosts"}}"#);
    assert!(!item_uri_is_authorized(&call, &paths));

    let type_item = json_fixture(r#"{"name": "Base", "uri": "file:///etc/group"}"#);
    assert!(!item_uri_is_authorized(&type_item, &paths));

    // An item that embeds no file URI carries no path to leak and is kept.
    let no_uri = json_fixture(r#"{"name": "Local"}"#);
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
fn long_declaration_content_is_cut_to_the_page_and_the_rest_is_one_read() {
    let body = (0..200)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut location = serde_json::json!({
        "uri": "file:///repo/a.ts",
        "value": body,
        "displayRange": {"startLine": 426, "endLine": 625}
    });
    let rest = cap_declaration_content(&mut location, 600).expect("a read of the lines left out");
    let content = location["value"].as_str().expect("value");
    let kept = content.lines().collect::<Vec<_>>();
    assert!(kept.len() < 200 && !kept.is_empty(), "{content}");
    assert_eq!(kept[0], "line 0");
    assert!(
        kept.iter().all(|line| line.starts_with("line ")),
        "the content holds source lines only: {content}"
    );
    // The lines left out are one executable read, each line once.
    let first_left = 426 + kept.len();
    let rest = rest.build();
    assert_eq!(rest["tool"], "localFetch");
    assert_eq!(
        rest["input"]["queries"][0],
        serde_json::json!({"path": "/repo/a.ts", "ranges": [{"startLine":first_left,"endLine":625}]})
    );
    assert_eq!(location["displayRange"]["endLine"], first_left - 1);
    assert_eq!(
        octocode_tools_shared::test_support::read_view(
            octocode_runtime_base::id::ToolId::LocalFetch,
            &rest["input"]["queries"][0]
        ),
        octocode_contract_types::ReadView::Content
    );
    let short = "a\nb\nc";
    let mut small =
        serde_json::json!({"value": short, "displayRange": {"startLine": 1, "endLine": 3}});
    assert!(cap_declaration_content(&mut small, 600).is_none());
    assert_eq!(small["value"], short, "short bodies are untouched");
}

#[test]
fn hover_range_is_published_as_a_one_based_display_range() {
    let hover = public_hover(json_fixture(
        r#"{
            "contents": {"kind": "markdown", "value": "```ts\nconst a: number\n```"},
            "range": {
                "start": {"line": 3, "character": 8},
                "end": {"line": 4, "character": 2}
            }
        }"#,
    ));
    assert!(hover.get("range").is_none(), "{hover}");
    assert_eq!(
        (&hover["line"], &hover["column"], &hover["endLine"]),
        (
            &serde_json::json!(4),
            &serde_json::json!(9),
            &serde_json::json!(5)
        ),
        "{hover}"
    );
    assert_eq!(hover["contents"]["kind"], "markdown", "{hover}");
    let bare = public_hover(json_fixture(r#"{"contents": "text"}"#));
    assert_eq!(bare, json_fixture(r#"{"contents": "text"}"#));
    octocode_runtime_base::contracts::validate_output(
        "lspSearch",
        &public_row(&serde_json::json!({
            "lsp": {"serverAvailable": true, "provider": "hoverProvider"},
            "operation": "hover", "hover": hover
        })),
    )
    .expect("published hover satisfies the internal output contract");
}

/// A capped declaration body numbers its source lines and carries no marker.
#[test]
fn capped_declaration_content_numbers_every_line() {
    let body = (1..=70).map(|n| format!("line {n}\n")).collect::<String>();
    let mut location = serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {"start": {"line": 9, "character": 0}, "end": {"line": 78, "character": 1}},
        "value": body,
        "displayRange": {"startLine": 10, "endLine": 79}
    });
    let _read = super::super::locations::cap_declaration_content(&mut location, 500)
        .expect("read of the rest");
    let public = public_location(location);
    let content = public["value"].as_str().expect("value");
    assert!(content.starts_with("10\tline 1\n"), "{content}");
    assert!(
        content.lines().all(|line| line
            .split_once('\t')
            .is_some_and(|(number, _)| number.parse::<u32>().is_ok())),
        "{content}"
    );
}

/// Every rung names kinds in the LSP `SymbolKind` vocabulary: a syntax kind
/// the vocabulary has no name for maps onto one that has, and every mapped
/// name is an LSP name or the documented `typeAlias` extension.
#[test]
fn syntax_kinds_speak_the_lsp_symbol_kind_vocabulary() {
    use crate::lsp_search::render::{symbol_kind_name, wire_kind};
    let lsp_names: Vec<&str> = (1..=26)
        .filter_map(octocode_engine::symbol_kind::name)
        .collect();
    for syntax in ["trait", "union", "type", "class", "enumMember", "impl"] {
        let named = symbol_kind_name(Some(&serde_json::json!(syntax)));
        assert_eq!(named, wire_kind(syntax));
        assert!(
            lsp_names.contains(&named.as_str()) || matches!(named.as_str(), "typeAlias" | "impl"),
            "{syntax} -> {named}"
        );
    }
    assert_eq!(wire_kind("trait"), "interface");
}
