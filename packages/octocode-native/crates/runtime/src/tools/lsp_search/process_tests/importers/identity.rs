//! Importer identity: a candidate site is a reference only when the name
//! there binds the anchor's declaration alone. A file's own same-named
//! binding shadows it; a site that also binds a declaration elsewhere is
//! undecided.

use super::*;

/// [`IMPORTER_SERVER`] answering `definition` and `references` per file:
/// - `m*.ts` declares its own `const target` (line 0) and hands it to a
///   module mock as a shorthand property (line 3): there the server names
///   both that binding and, through the factory's contextual type, the
///   anchor (as tsserver does); every other site names the binding alone,
///   and its references are the binding's own; every importer's references
///   list that shorthand too, as a property of the anchor's module type;
/// - `r*.ts` imports through the barrel `b.ts`, whose export specifier the
///   chain follows to the anchor;
/// - `u*.ts` names the anchor and a declaration in `other.ts` at once;
/// - every other file names the anchor alone.
fn identity_server() -> String {
    let cases = r"    case 'textDocument/definition': {
      const uri = p.textDocument.uri;
      const line = p.position.line;
      const other = 'file://' + root + '/other.ts';
      const local = { uri, range: at(0, 6, 6) };
      if (/\/m\d+\.ts$/.test(uri)) return line === 3 ? [local, decl] : [local];
      if (/\/r\d+\.ts$/.test(uri)) return [{ uri: 'file://' + root + '/b.ts', range: at(0, 9, 6) }];
      if (/\/u\d+\.ts$/.test(uri)) return [decl, { uri: other, range: at(0, 16, 5) }];
      if (uri === other) return [{ uri, range: at(0, 16, 5) }];
      return [decl];
    }
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === declUri) return [decl];
      if (/\/m\d+\.ts$/.test(uri)) return [at(0, 6, 6), at(3, 2, 6), at(5, 0, 6)].map((range) => ({ uri, range }));
      if (/\/b\.ts$/.test(uri)) return [{ uri, range: at(0, 9, 6) }, decl];
      const mock = root + '/m000.ts';
      const property = require('fs').existsSync(mock) ? [{ uri: 'file://' + mock, range: at(3, 2, 6) }] : [];
      return [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }, decl].concat(property);
    }
";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/definition': {")
        .expect("definition case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/prepareCallHierarchy':")
            .expect("next case");
    format!(
        "{}{cases}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    )
}

/// A test file mocking the anchor's module with its own same-named binding
/// (the shape of a vitest `vi.mock` factory).
const MOCKING: &str = "const target = vi.fn();\nvi.mock(import('./a'), async (importOriginal) => ({\n  ...(await importOriginal()),\n  target,\n}));\ntarget.mockReturnValue(1);\n";

/// `references` for `target` without its declaration, over a workspace of
/// `a.ts`, one plain importer `f000.ts` and the `extra` files.
fn references(extra: &[(&str, &str)]) -> (Fixture, serde_json::Value) {
    let (dir, root, execution) = importer_fixture_serving(&[("f", 1)], &identity_server());
    for (name, content) in extra {
        std::fs::write(root.join(name), content).expect("candidate");
    }
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let query_row = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "includeDeclaration": false, "workspaceRoot": root.to_string_lossy()
    });
    let query = serde_json::from_value(query_row.clone()).expect("query");
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    (dir, row)
}

/// A file declaring its own `target` and naming the anchor's module only
/// to mock it is no importer: none of its sites are references, and the
/// row stays complete. An aliased import and an import through a re-export
/// barrel still resolve to the anchor.
#[test]
fn a_same_named_local_binding_shadows_the_anchor_while_aliases_and_reexports_resolve() {
    if !require_node() {
        return;
    }
    let (_dir, row) = references(&[
        ("m000.ts", MOCKING),
        ("s000.ts", "import { target as run } from './a';\nrun();\n"),
        ("b.ts", "export { target } from './a';\n"),
        ("r000.ts", "import { target } from './b';\ntarget();\n"),
    ]);
    let mut rows = reference_rows(&row);
    rows.sort();
    let sites = |file: &str| [(file.to_owned(), 1, 10), (file.to_owned(), 2, 1)];
    let mut expected = vec![("b.ts".to_owned(), 1, 10)];
    for file in ["f000.ts", "r000.ts", "s000.ts"] {
        expected.extend(sites(file));
    }
    assert_eq!(rows, expected, "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert_eq!(row["coverage"]["importerScan"], "complete", "{row}");
    assert_eq!(row["coverage"]["state"], "complete", "{row}");
}

/// A site whose name binds the anchor and another declaration at once is
/// not a confirmed reference: its file is an unresolved candidate, left out
/// of the answer and named by the text lead, and the row is partial.
#[test]
fn a_site_binding_another_declaration_too_stays_unresolved_and_partial() {
    if !require_node() {
        return;
    }
    let (_dir, row) = references(&[("u000.ts", "import { target } from './a';\ntarget();\n")]);
    let sites = reference_rows(&row);
    assert_eq!(
        sites,
        [("f000.ts".to_owned(), 1, 10), ("f000.ts".to_owned(), 2, 1)],
        "{row}"
    );
    assert_eq!(row["coverage"]["importerScan"], "failed", "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importerScanFailed"))),
        "{row}"
    );
    // The unresolved candidate is named by the row's text lead.
    let lead = crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)["input"]["queries"].to_string();
    assert!(lead.contains("u000.ts"), "{row}");
}
