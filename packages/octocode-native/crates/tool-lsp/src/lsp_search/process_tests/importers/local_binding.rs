//! A JS/TS local binding has no importer: the files elsewhere that spell its
//! name are never candidates, so no window opens and no lead continues.

use super::*;

/// `shadow.js` of the tool-ratings fixture: a parameter and a nested function
/// spelled like an exported function of other files.
const SHADOW: &str = "export function shadow(core) { return core(7); }\nexport function unrelatedCaller() { function core(x) { return x * 2; } return core(3); }\n";

/// The server answers the nested `core` with its two sites on line 2.
const ANSWERS: &str = r#"
  const anchor = file('src/shadow.js');
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, documentSymbolProvider: true } };
    case 'textDocument/definition':
      if (p.textDocument.uri === anchor) return [{ uri: anchor, range: at(1, 45, 4) }];
      return [{ uri: p.textDocument.uri, range: at(0, 16, 4) }];
    case 'textDocument/references':
      if (p.textDocument.uri === anchor) return [{ uri: anchor, range: at(1, 45, 4) }, { uri: anchor, range: at(1, 78, 4) }];
      return [];
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
"#;

/// Files under `lib/` that declare and call their own `core`: more than one
/// importer window.
const SAME_SPELLED: usize = 30;

/// `references` on the nested `core` of `shadow.js`, with no `workspaceRoot`:
/// the two sites, complete, with no importer window and no file opened but
/// the anchor; neither the gitignored importer of another `core` nor a page
/// that spells it makes the answer partial.
#[test]
fn a_local_binding_opens_no_importer_window() {
    if !require_node() {
        return;
    }
    let same_spelled = (0..SAME_SPELLED)
        .map(|index| format!("lib/n{index:03}.js"))
        .collect::<Vec<_>>();
    let mut files = vec![
        (".gitignore", "cache/\n"),
        ("src/package.json", "{}\n"),
        ("src/shadow.js", SHADOW),
        ("lib/page.html", "<script>\ncore();\n</script>\n"),
        ("cache/z.js", "export function core() {}\n"),
        ("cache/y.js", "import { core } from './z.js';\ncore();\n"),
    ];
    for name in &same_spelled {
        files.push((name.as_str(), "export function core() {}\ncore();\n"));
    }
    let (_dir, root, opened, execution) =
        repository_fixture(&files, ANSWERS, (&[".js"], "javascript"));
    let row = run_once(
        &root,
        &execution,
        serde_json::json!({
            "operation": "references", "path": root.join("src/shadow.js").to_string_lossy(),
            "symbolName": "core", "line": 2
        }),
    );
    let sites = location_rows(&row)
        .iter()
        .map(|site| site["line"].as_u64().expect("line"))
        .collect::<Vec<_>>();
    assert_eq!(sites, vec![2, 2], "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.pointer("/next/nextImporterWindow").is_none(), "{row}");
    let opened = std::fs::read_to_string(&opened).unwrap_or_default();
    assert!(
        !opened.contains("/lib/") && !opened.contains("/cache/"),
        "only the anchor is opened: {opened}"
    );
}
