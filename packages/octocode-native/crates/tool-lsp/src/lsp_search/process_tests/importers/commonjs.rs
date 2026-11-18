//! A CommonJS anchor in a small project inside a larger repository: its
//! importers name its module, so the files elsewhere that only spell the
//! name are set aside unopened, and the first answer is whole.

use super::*;

/// A JavaScript server whose program holds only the opened files and does
/// not follow a CommonJS export back to a destructured `require` (tsserver's
/// two gaps): `b.cjs` resolves to the anchor, any other file to `lib/other.ts`.
const ANSWERS: &str = r#"
  const anchor = file('fx/a.cjs');
  const importer = file('fx/b.cjs');
  const decl = { uri: anchor, range: at(0, 9, 5) };
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, documentSymbolProvider: true } };
    case 'textDocument/definition': {
      const uri = p.textDocument.uri;
      if (uri === anchor || uri === importer) return [decl];
      return [{ uri: file('lib/other.ts'), range: at(0, 13, 5) }];
    }
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === anchor) return [decl, { uri: anchor, range: at(3, 19, 5) }];
      if (uri === importer) return [{ uri: importer, range: at(0, 8, 5) }, { uri: importer, range: at(2, 9, 5) }];
      return [];
    }
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
"#;

/// Files under `lib/` that bind `stage` from `lib/other.ts`: more than one
/// importer window.
const UNRELATED: usize = 30;

/// `references` at the export line of `fx/a.cjs` (a project of its own, by
/// its `package.json`, in a repository), with no `workspaceRoot`: one call
/// lists `a.cjs` and its `require` importer `b.cjs`, complete. No file that
/// binds the name from another module is opened in the server, and neither
/// a page that only spells the name nor a gitignored file that imports it
/// from another module makes the answer partial.
#[test]
fn a_commonjs_anchor_lists_its_require_importer_on_the_first_call() {
    if !require_node() {
        return;
    }
    let unrelated = (0..UNRELATED)
        .map(|index| format!("lib/n{index:03}.ts"))
        .collect::<Vec<_>>();
    let mut files = vec![
        (".gitignore", "cache/\n"),
        ("fx/package.json", "{}\n"),
        (
            "fx/a.cjs",
            "function stage(a) {\n  return a;\n}\nmodule.exports = { stage };\n",
        ),
        (
            "fx/b.cjs",
            "const { stage } = require('./a.cjs');\nfunction run() {\n  return stage(1);\n}\nmodule.exports = { run };\n",
        ),
        ("lib/other.ts", "export const stage = 1;\n"),
        (
            "lib/page.html",
            "<section id=\"stage\"></section>\n<script>\nconst stage = document.getElementById('stage');\n</script>\n",
        ),
        ("cache/z.js", "export function stage() {}\n"),
        ("cache/y.js", "import { stage } from './z.js';\nstage();\n"),
    ];
    for name in &unrelated {
        files.push((
            name.as_str(),
            "import { stage } from './other';\nexport const copy = stage;\n",
        ));
    }
    let (_dir, root, opened, execution) =
        repository_fixture(&files, ANSWERS, (&[".cjs", ".js", ".ts"], "javascript"));
    let row = run_once(
        &root,
        &execution,
        serde_json::json!({
            "operation": "references", "path": root.join("fx/a.cjs").to_string_lossy(),
            "symbolName": "stage", "line": 4
        }),
    );
    assert!(lists_path(&row, "fx/a.cjs"), "{row}");
    assert!(lists_path(&row, "fx/b.cjs"), "the require importer: {row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.pointer("/next/nextImporterWindow").is_none(), "{row}");
    let opened = std::fs::read_to_string(&opened).unwrap_or_default();
    assert!(
        !opened.contains("/lib/"),
        "files that bind the name from another module stay unopened: {opened}"
    );
}
