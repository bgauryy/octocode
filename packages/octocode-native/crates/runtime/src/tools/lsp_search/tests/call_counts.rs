//! Call counts: builtin-lib callees, text-match partials, alias parsing
//! and the one read of listed sites.

use super::*;

#[test]
fn typescript_builtin_lib_declarations_are_recognized_only_under_typescript_lib() {
    let at = |uri: &str| serde_json::json!({"uri": uri});
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.es5.d.ts"
    )));
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.dom.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/@types/node/lib.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/src/lib.utils.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/typescript.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&serde_json::json!({})));
}

#[tokio::test(flavor = "current_thread")]
async fn builtin_lib_callees_are_listed_not_counted() {
    let mut graph = graph(&[("root", &["toUpperCase", "map", "map"])]);
    graph.uri = Some("file:///repo/node_modules/typescript/lib/lib.es5.d.ts".into());
    let walk = walk(&graph, 1).await;
    assert!(walk.edges.is_empty());
    // Each name once, in walk (position) order.
    let mut names = walk.builtin_lib.clone();
    names.sort();
    assert_eq!(names, vec!["map".to_owned(), "toUpperCase".to_owned()]);
    assert_eq!(walk.out_of_policy, 0, "built-ins are not policy failures");
    let q = query(serde_json::json!({
        "operation": "callees", "path": "file:///repo/a.ts",
        "symbolName": "foo", "line": 1
    }));
    let mut row = serde_json::json!({"status": "hasResults", "kind": "callees", "items": []});
    mark_truncation(&mut row, &q, &[(Expansion::OutgoingCalls, &walk)]);
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(row["builtinLib"], serde_json::json!(walk.builtin_lib));
    // A container (`detail`) qualifies the name.
    let mut node = node_at("toUpperCase", Some("file:///x/typescript/lib/lib.es5.d.ts"));
    node["detail"] = serde_json::json!("String");
    assert_eq!(builtin_name(&node), "String.toUpperCase");
}

#[test]
fn file_level_partial_clears_when_text_files_match() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.ts"), "export function foo() {}\n").expect("a");
    std::fs::write(root.join("b.ts"), "import { foo } from './a';\nfoo();\n").expect("b");
    let scope =
        super::super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("foo", &paths, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "foo", "line": 1
    }));
    let row = || serde_json::json!({"kind": "references"});
    // One file only the text scan sees: flagged, and the lead lists it.
    scope.answer([root.join("a.ts").to_string_lossy()]);
    let mut partial = row();
    flag_partial(
        &mut partial,
        &q,
        "importerScanCapped",
        Some("capped"),
        &scope,
    );
    assert_eq!(partial["isPartial"], true, "{partial}");
    // The importer cap is a window, not a terminal limit:
    // `next.nextImporterPage` verifies the unchecked candidates.
    assert!(partial.get("terminalLimit").is_none(), "{partial}");
    assert_eq!(partial["coverage"]["textOnlyFiles"], 1, "{partial}");
    let lead = &crate::tools::test_support::checks::related_call(&partial, "localSearch", |_| true)
        ["input"]["queries"];
    assert_eq!(lead.as_array().map(Vec::len), Some(1), "{partial}");
    assert_eq!(
        lead[0]["path"],
        root.join("b.ts").to_string_lossy().as_ref()
    );
    // Every file that spells the name is in the answer: the file-level
    // reason is moot.
    scope.answer([root.join("b.ts").to_string_lossy()]);
    let mut agreed = row();
    flag_partial(
        &mut agreed,
        &q,
        "importerScanCapped",
        Some("capped"),
        &scope,
    );
    assert!(agreed.get("isPartial").is_none(), "{agreed}");
    assert!(agreed.get("next").is_none(), "{agreed}");
    assert_eq!(agreed["coverage"]["textOnlyFiles"], 0, "{agreed}");
    assert!(agreed["coverage"].get("reason").is_none(), "{agreed}");
}

#[test]
fn site_level_partial_survives_matching_counts() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.py"), "def get():\n    pass\nget()\n").expect("a");
    let scope =
        super::super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.py".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("get", &paths, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    scope.answer([root.join("a.py").to_string_lossy()]);
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.py").to_string_lossy(),
        "symbolName": "get", "line": 1
    }));
    let mut row = serde_json::json!({"kind": "references"});
    flag_partial(&mut row, &q, "cfgGatedFiles", Some("gated"), &scope);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["cfgGatedFiles"]),
        "{row}"
    );
    // Dynamic dispatch is judged by files: every file that spells the name
    // is answered, so the Python row is complete.
    let mut python = serde_json::json!({"kind": "references"});
    flag_partial(&mut python, &q, "dynamicDispatch", Some("dynamic"), &scope);
    assert!(python.get("isPartial").is_none(), "{python}");
    assert!(row["coverage"].get("reason").is_none(), "{row}");
    // No file to list: the lead searches the whole scope.
    assert_eq!(
        crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)["input"]["queries"]
            [0]["path"],
        root.to_string_lossy().as_ref()
    );
}

#[test]
fn builtin_lib_and_text_only_counts_satisfy_the_output_contract() {
    let mut row = serde_json::json!({
        "lsp": {"serverAvailable": true},

            "kind": "callees",
            "files": [{"path": "/repo/a.ts", "matches": [{"symbolName": "f", "kind": "function", "line": 10, "endLine": 12, "sites": [{"line": 2, "column": 3}]}]}],
            "builtinLib": ["String.toUpperCase", "map"],
            "coverage": {"textOnlyFiles": 3, "reason": "importerScanCapped"}

    });
    stamp_coverage(&mut row);
    assert_eq!(row["coverage"]["state"], "partial", "{row}");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("builtinLib and textOnlyFiles are contract-valid");
}

/// Only a file that spells a rename of the symbol (`symbol as x`, or a
/// destructured `symbol: x`) is parsed for aliasing imports.
#[test]
fn only_a_spelled_rename_is_parsed_for_aliases() {
    for renamed in [
        "import { foo as bar } from './a';",
        "import {\n  foo\n    as bar,\n} from './a';",
        "const { foo: bar } = require('./a');",
        "use crate::a::foo as bar;",
        "from a import (foo as bar)",
    ] {
        assert!(
            super::super::recovery::may_rename(renamed, "foo"),
            "{renamed}"
        );
    }
    for plain in [
        "import { foo } from './a';",
        "foo(1); foobar as x; x.foo::y;",
        "const food = { foo };",
    ] {
        assert!(!super::super::recovery::may_rename(plain, "foo"), "{plain}");
    }
}

/// A callers or references success offers one read of the listed sites in
/// context: one localFetch row per file (first files first), merged ±6-line
/// windows, at most the batch limit of rows.
#[test]
fn site_rows_offer_one_read_of_the_sites_in_context() {
    let mut row = serde_json::json!({"kind": "callers", "files": [
        {"path": "/repo/a.ts", "matches": [{"symbolName": "render", "kind": "function", "line": 97,
            "endLine": 848, "sites": [{"line": 359, "column": 38}, {"line": 362, "column": 5}]}]},
        {"path": "/repo/b.ts", "matches": [
            {"symbolName": "load", "kind": "function", "line": 1, "endLine": 9, "sites": [{"line": 4, "column": 2}]},
            {"symbolName": "go", "kind": "function", "line": 30, "endLine": 50, "sites": [{"line": 40, "column": 1}]}
        ]}
    ]});
    attach_read_lead(&mut row);
    let read = &crate::tools::test_support::checks::related_call(&row, "localFetch", |_| true);
    assert_eq!(read["tool"], "localFetch", "{row}");
    assert_eq!(
        read["input"]["queries"],
        serde_json::json!([
            {"path": "/repo/a.ts", "ranges": [{"startLine":353,"endLine":368}]},
            {"path": "/repo/b.ts", "ranges": [{"startLine":1,"endLine":10},{"startLine":34,"endLine":46}]}
        ]),
        "{row}"
    );
    let files = (0..7)
        .map(|n| serde_json::json!({"path": format!("/repo/{n}.ts"), "matches": [{"line": n + 10, "column": 1, "value": "x"}]}))
        .collect::<Vec<_>>();
    let mut wide = serde_json::json!({"kind": "references", "files": files});
    attach_read_lead(&mut wide);
    let reads = wide["related"]
        .as_array()
        .expect("read batches")
        .iter()
        .flat_map(|call| call["input"]["queries"].as_array().expect("read rows"));
    assert_eq!(
        reads
            .map(|query| query["path"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        (0..7)
            .map(|index| format!("/repo/{index}.ts"))
            .collect::<Vec<_>>()
    );
    let mut empty = serde_json::json!({"status": "empty", "kind": "empty"});
    attach_read_lead(&mut empty);
    assert!(empty.get("next").is_none());
}
