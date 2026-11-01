//! Call rows: edges compacted per file, caller labels and the member or
//! callback that holds a call site.

use super::*;

#[test]
fn call_sites_that_differ_only_in_end_column_publish_once() {
    let edge = HierarchyEdge {
        node: node("callee"),
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 22}}),
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 17}}),
            serde_json::json!({"start": {"line": 871, "character": 2}, "end": {"line": 871, "character": 5}}),
        ],
    };
    let public = public_edge(Expansion::OutgoingCalls, &edge, None);
    assert_eq!(
        public["fromRanges"],
        serde_json::json!([
            {"startLine": 871, "startCharacter": 11, "endLine": 871},
            {"startLine": 872, "startCharacter": 3, "endLine": 872}
        ])
    );
}

#[test]
fn call_edges_compact_to_per_file_call_rows() {
    let caller = |path: &str,
                  name: &str,
                  kind: &str,
                  start: u64,
                  end: u64,
                  sites: &[(u64, u64)]| {
        serde_json::json!({
            "from": {"symbolName": name, "kind": kind, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": end}},
            "fromRanges": sites.iter().map(|(line, column)| serde_json::json!({"startLine": line, "startCharacter": column, "endLine": line})).collect::<Vec<_>>(),
            "level": 1
        })
    };
    let mut method = caller(
        "/repo/App.tsx",
        "renderEmbeddables",
        "method",
        1833,
        2120,
        &[(1981, 32)],
    );
    method["from"]["detail"] = serde_json::json!("App");
    let mut recovered = caller("/repo/lib.ts", "load", "function", 718, 718, &[(729, 24)]);
    recovered["source"] = serde_json::json!("recoveredFromReferences");
    // A detail that only restates the name (`fn other()`) is dropped; a
    // signature is kept only to tell apart listed nodes sharing a name; a
    // container name (`App`) is kept.
    let mut render = caller(
        "/repo/svg.ts",
        "render",
        "function",
        97,
        848,
        &[(359, 38), (402, 5)],
    );
    render["from"]["detail"] = serde_json::json!("pub fn render(\n    ctx: &Ctx,\n) -> Svg");
    let mut other = caller("/repo/svg.ts", "other", "function", 900, 950, &[(910, 3)]);
    other["from"]["detail"] = serde_json::json!("pub(crate) async fn other()");
    let mut row = serde_json::json!({"operation": "callers", "matches": [
        render,
        method,
        recovered,
        other,
    ]});
    compact_calls(&mut row, &[], &repo_policy());
    assert!(row.get("matches").is_none(), "{row}");
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/svg.ts", "matches": [
                {"symbolName": "render", "kind": "function", "line": 97, "endLine": 848,
                    "sites": [{"line": 359, "column": 38}, {"line": 402, "column": 5}]},
                {"symbolName": "other", "kind": "function", "line": 900, "endLine": 950,
                    "sites": [{"line": 910, "column": 3}]}
            ]},
            {"path": "/repo/App.tsx", "matches": [
                {"symbolName": "renderEmbeddables", "kind": "method", "line": 1833, "endLine": 2120,
                    "sites": [{"line": 1981, "column": 32}], "detail": "App"}
            ]},
            {"path": "/repo/lib.ts", "matches": [
                {"symbolName": "load", "kind": "function", "line": 718,
                    "sites": [{"line": 729, "column": 24}], "source": "recoveredFromReferences"}
            ]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("compact callers satisfy the output contract");
    // A module caller is top-level code named by its file; that name
    // anchors no lspSearch call, so its row has no symbolName.
    let mut module = serde_json::json!({"operation": "callers", "matches": [
        caller("/repo/public.test.ts", "public.test.ts", "module", 1, 40, &[(12, 3)]),
    ]});
    compact_calls(&mut module, &[], &repo_policy());
    let module_row = &module["files"][0]["matches"][0];
    assert_eq!(module_row["kind"], "module", "{module}");
    assert!(module_row.get("symbolName").is_none(), "{module}");
    assert_eq!(module_row["sites"][0]["line"], 12, "{module}");
    // Deeper and outgoing edges use the same rows: `to` names a callee (its
    // call sites are in the caller), and `via <name>@<line>` names the parent
    // node an edge below level 1 connects to.
    let via = |name: &str, path: &str, line: u64| serde_json::json!({"symbolName": name, "uri": format!("file://{path}"), "line": line, "character": 7});
    let mut level2 = caller("/repo/b.ts", "b", "function", 5, 9, &[(6, 3)]);
    level2["level"] = serde_json::json!(2);
    level2["via"] = via("a", "/repo/a.ts", 1);
    let mut callee = caller("/repo/c.ts", "c", "function", 10, 12, &[(2, 5)]);
    callee["to"] = callee["from"].take();
    callee.as_object_mut().expect("item").remove("from");
    // Two listed nodes share a name and line: the via names its file,
    // relative to the workspace like every row path.
    let mut ambiguous = caller("/repo/sub/z.ts", "z", "function", 40, 44, &[(41, 2)]);
    ambiguous["level"] = serde_json::json!(2);
    ambiguous["via"] = via("walk", "/repo/y.ts", 281);
    let items = serde_json::json!([
        caller("/repo/a.ts", "a", "function", 1, 3, &[(2, 1)]),
        caller("/repo/x.ts", "walk", "function", 281, 290, &[(285, 1)]),
        caller("/repo/y.ts", "walk", "function", 281, 299, &[(290, 1)]),
        level2,
        ambiguous,
        callee
    ]);
    let mut deep = serde_json::json!({"operation": "callers", "matches": items.clone()});
    compact_calls(&mut deep, items.as_array().expect("items"), &repo_policy());
    assert_eq!(
        deep["files"],
        serde_json::json!([
            {"path": "/repo/a.ts", "matches": [{"symbolName": "a", "kind": "function", "line": 1,
                "endLine": 3, "sites": [{"line": 2, "column": 1}]}]},
            {"path": "/repo/x.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 290, "sites": [{"line": 285, "column": 1}]}]},
            {"path": "/repo/y.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 299, "sites": [{"line": 290, "column": 1}]}]},
            {"path": "/repo/b.ts", "matches": [{"symbolName": "b", "kind": "function", "line": 5,
                "endLine": 9, "sites": [{"line": 6, "column": 3}], "via": {"symbolName": "a", "line": 1}}]},
            {"path": "/repo/sub/z.ts", "matches": [{"symbolName": "z", "kind": "function", "line": 40,
                "endLine": 44, "sites": [{"line": 41, "column": 2}],
                "via": {"symbolName": "walk", "line": 281, "path": "y.ts"}}]},
            {"path": "/repo/c.ts", "matches": [{"symbolName": "c", "kind": "function", "line": 10,
                "endLine": 12, "sites": [{"line": 2, "column": 5}]}]}
        ]),
        "{deep}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&deep))
        .expect("compact hierarchy rows satisfy the output contract");
    // Type-hierarchy items are nodes, not calls: they stay items.
    let mut types = serde_json::json!({"operation": "supertypes", "matches": [
        {"name": "Base", "kind": "class", "uri": "file:///repo/a.ts", "level": 1}
    ]});
    let before = types.clone();
    compact_calls(&mut types, &[], &repo_policy());
    assert_eq!(types, before);
}

/// An outgoing call site lies in the caller's file, so a callee row is filed
/// under the caller and names the callee's file beside its declaration range
/// when it differs. Incoming and outgoing rows never share a file entry.
#[test]
fn outgoing_call_sites_are_filed_under_the_caller() {
    let root = node_at("main", Some("file:///repo/index.ts"));
    let mut callee = node_at("helper", Some("file:///repo/types.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let outgoing = HierarchyEdge {
        node: callee,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 380, "character": 2}, "end": {"line": 380, "character": 8}}),
        ],
    };
    let incoming = HierarchyEdge {
        node: node_at("boot", Some("file:///repo/index.ts")),
        parent: Some(root),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::IncomingCalls, &incoming, None),
        public_edge(Expansion::OutgoingCalls, &outgoing, None),
    ];
    assert!(
        items.iter().all(|item| item.get("via").is_none()),
        "level-1 edges name no parent: {items:?}"
    );
    let mut row = serde_json::json!({"operation": "callers", "matches": items});
    compact_calls(&mut row, &[], &repo_policy());
    let helper_line = name_line("helper") + 1;
    let boot_line = name_line("boot") + 1;
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/index.ts", "matches": [{"symbolName": "boot", "kind": "function",
                "line": boot_line, "endLine": boot_line + 3, "sites": [{"line": 4, "column": 5}]}]},
            {"path": "/repo/index.ts", "matches": [{"symbolName": "helper", "kind": "function",
                "line": helper_line, "endLine": helper_line + 3, "path": "types.d.ts",
                "sites": [{"line": 381, "column": 3}]}]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("caller-filed callee rows satisfy the output contract");
}

/// Two listed callers share a name: their signatures tell them apart.
#[test]
fn signatures_are_kept_only_for_callers_that_share_a_name() {
    let caller = |path: &str, detail: &str, start: u64, site: u64| {
        serde_json::json!({
            "from": {"symbolName": "render", "kind": "function", "detail": detail, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": start + 5}},
            "fromRanges": [{"startLine": site, "startCharacter": 3, "endLine": site}],
            "level": 1
        })
    };
    let mut row = serde_json::json!({"operation": "callers", "matches": [
        caller("/repo/a.rs", "fn render(ctx: &Ctx) -> Svg", 10, 12),
        caller("/repo/b.rs", "fn render(page: &Page)", 20, 22)
    ]});
    compact_calls(&mut row, &[], &repo_policy());
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/a.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 10,
                "endLine": 15, "sites": [{"line": 12, "column": 3}], "detail": "fn render(ctx: &Ctx) -> Svg"}]},
            {"path": "/repo/b.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 20,
                "endLine": 25, "sites": [{"line": 22, "column": 3}], "detail": "fn render(page: &Page)"}]}
        ]),
        "{row}"
    );
}

/// tsserver reports a call in a constructor with the whole class as caller;
/// the row shows the constructor that holds the call site instead. The
/// class file is read once (policy-checked, bounded) for every class caller
/// it holds.
#[tokio::test(flavor = "current_thread")]
async fn class_callers_narrow_to_the_member_holding_the_call_site() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("canonical root");
    let file = root.join("editor.ts");
    std::fs::write(
        &file,
        "export class Editor {\n  value = 1;\n  constructor() {\n    mutate(this);\n  }\n  other() {}\n}\n",
    )
    .expect("source");
    let uri = format!("file://{}", file.display());
    let class = serde_json::json!({
        "name": "Editor", "kind": 5, "uri": uri,
        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 6, "character": 1}},
        "selectionRange": {"start": {"line": 0, "character": 13}, "end": {"line": 0, "character": 19}}
    });
    let edge = |site: serde_json::Value| super::super::walk::HierarchyEdge {
        node: class.clone(),
        parent: None,
        level: 1,
        sites: vec![site],
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    let mut sources = super::super::source::SourceCache::new(&policy);
    let mut facts = super::super::walk::ClassFacts::default();
    let site = serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 10}});
    let narrowed = facts
        .narrow(&edge(site), &mut sources)
        .await
        .expect("constructor holds the call");
    // The whole narrowed item: the constructor's name, ranges and the
    // class as its detail.
    assert_eq!(
        narrowed,
        serde_json::json!({
            "name": "constructor", "kind": 9, "detail": "Editor", "uri": uri,
            "range": {"start": {"line": 2, "character": 2}, "end": {"line": 4, "character": 3}},
            "selectionRange": {"start": {"line": 2, "character": 2}, "end": {"line": 2, "character": 13}}
        }),
        "{narrowed}"
    );
    // A site outside every member keeps the class.
    let field = serde_json::json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}});
    assert!(facts.narrow(&edge(field), &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "one read for both class callers");
    // A class file outside the read policy is never read.
    let outside = tempfile::tempdir().expect("outside");
    let foreign = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("x.ts");
    std::fs::write(
        &foreign,
        "export class X {\n  constructor() {\n    f();\n  }\n}\n",
    )
    .expect("foreign");
    let mut foreign_class = class.clone();
    foreign_class["uri"] = serde_json::json!(format!("file://{}", foreign.display()));
    let foreign_edge = super::super::walk::HierarchyEdge {
        node: foreign_class,
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 2, "character": 4}, "end": {"line": 2, "character": 5}}),
        ],
    };
    assert!(facts.narrow(&foreign_edge, &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "the refused file was not read");
}

/// tsserver files calls made inside a top-level callback (`describe(() =>
/// …)`, `it(…)`) under the whole module; each site is named by the
/// innermost function symbol around it, as reference-derived callers are.
#[test]
fn module_callers_name_the_innermost_enclosing_callback() {
    let range = |start: u64, end: u64| serde_json::json!({"start": {"line": start, "character": 0}, "end": {"line": end, "character": 2}});
    let module = serde_json::json!({
        "name": "\"/repo/a.test\"", "kind": 2, "uri": "file:///repo/a.test.ts",
        "range": range(0, 40), "selectionRange": range(0, 40)
    });
    let site = |line: u64| serde_json::json!({"start": {"line": line, "character": 4}, "end": {"line": line, "character": 9}});
    let edge = HierarchyEdge {
        node: module,
        parent: None,
        level: 1,
        sites: vec![site(3), site(10), site(30)],
    };
    let symbols = serde_json::json!([{
        "name": "describe('x') callback", "kind": 12, "range": range(1, 12), "selectionRange": range(1, 1),
        "children": [{"name": "it('y')   callback", "kind": 12, "range": range(2, 5), "selectionRange": range(2, 2)}]
    }]);
    let split = split_module_callers(&edge, &symbols);
    let named = split
        .iter()
        .map(|edge| {
            (
                edge.node["name"].as_str().unwrap_or_default().to_owned(),
                edge.sites.len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        named,
        vec![
            ("it('y') callback".to_owned(), 1),
            ("describe('x') callback".to_owned(), 1),
            ("\"/repo/a.test\"".to_owned(), 1),
        ]
    );
    assert_eq!(split[0].node["uri"], "file:///repo/a.test.ts");
    // A function caller is kept as the server named it.
    let mut function = edge;
    function.node["kind"] = serde_json::json!(12);
    assert_eq!(split_module_callers(&function, &symbols).len(), 1);
}

/// `callers` on a class lists the symbols that use it; the row says so.
#[test]
fn callers_of_a_class_are_labelled_as_uses() {
    let class = serde_json::json!({"name": "Widget", "kind": 5});
    let function = serde_json::json!({"name": "run", "kind": 12});
    let note = class_uses_warning("callers", std::slice::from_ref(&class)).expect("class note");
    assert!(
        note.contains("uses") && note.contains("not calls"),
        "{note}"
    );
    assert_eq!(
        class_uses_warning("callers", std::slice::from_ref(&function)),
        None
    );
    assert_eq!(
        class_uses_warning("callees", std::slice::from_ref(&class)),
        None
    );
    assert_eq!(
        class_uses_warning("subtypes", std::slice::from_ref(&class)),
        None
    );
}
