//! A heritage-scan row beside the server's `implementation` answer must be
//! able to name the anchor's type: another project's same-named type is no
//! implementation of it.

use super::*;

/// The server answers `implementation` of `fx/types.ts`'s `Shape` with
/// `Circle` only: the files outside its project are not in its program.
const ANSWERS: &str = r#"
  const types = file('fx/types.ts');
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, implementationProvider: true, documentSymbolProvider: true } };
    case 'textDocument/implementation':
      return [{ uri: types, range: at(1, 13, 6) }];
    case 'textDocument/definition':
      return [{ uri: types, range: at(0, 17, 5) }];
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
"#;

/// `implementation` of `Shape` with no `workspaceRoot`: the server's
/// `Circle`, and `Tri` (whose file imports the anchor's `Shape`) from the
/// scan; never `Square`, which implements the `Shape` its own file declares.
#[test]
fn a_scan_row_must_reach_the_anchors_type() {
    if !require_node() {
        return;
    }
    let (_dir, root, _opened, execution) = repository_fixture(
        &[
            ("fx/tsconfig.json", "{}\n"),
            (
                "fx/types.ts",
                "export interface Shape { area(): number; }\nexport class Circle implements Shape { area() { return 1; } }\n",
            ),
            (
                "other/model.ts",
                "export interface Shape {\n  area(): number;\n}\n\nexport class Square implements Shape {\n  area(): number { return 2; }\n}\n",
            ),
            (
                "user/tri.ts",
                "import { Shape } from '../fx/types';\nexport class Tri implements Shape { area() { return 3; } }\n",
            ),
        ],
        ANSWERS,
        (&[".ts"], "typescript"),
    );
    let row = run_once(
        &root,
        &execution,
        serde_json::json!({
            "operation": "implementation", "path": root.join("fx/types.ts").to_string_lossy(),
            "symbolName": "Shape", "line": 1
        }),
    );
    let found = located(&row);
    assert!(
        found
            .iter()
            .any(|(file, line, _)| file == "types.ts" && *line == 2),
        "the server's Circle: {row}"
    );
    assert!(
        found.iter().any(|(file, _, source)| file == "tri.ts"
            && source.as_deref() == Some(crate::lsp_search::ops::SCAN_SOURCE)),
        "a scan row that imports the anchor's type: {row}"
    );
    assert!(
        !found.iter().any(|(file, _, _)| file == "model.ts"),
        "another project's same-named type: {row}"
    );
}

/// The same answer from a use of `Shape` (`user/tri.ts`'s `implements`
/// clause): the scan reads the reach of the declaration the use resolves
/// to, so `Square`, which implements another file's `Shape`, stays out.
#[test]
fn a_scan_from_a_use_reaches_what_its_declaration_reaches() {
    if !require_node() {
        return;
    }
    let (_dir, root, _opened, execution) = repository_fixture(
        &[
            ("fx/tsconfig.json", "{}\n"),
            (
                "fx/types.ts",
                "export interface Shape { area(): number; }\nexport class Circle implements Shape { area() { return 1; } }\n",
            ),
            (
                "other/model.ts",
                "export interface Shape {\n  area(): number;\n}\n\nexport class Square implements Shape {\n  area(): number { return 2; }\n}\n",
            ),
            (
                "user/tri.ts",
                "import { Shape } from '../fx/types';\nexport class Tri implements Shape { area() { return 3; } }\n",
            ),
        ],
        ANSWERS,
        (&[".ts"], "typescript"),
    );
    let ask = |file: &str, line: u32| {
        run_once(
            &root,
            &execution,
            serde_json::json!({
                "operation": "implementation", "path": root.join(file).to_string_lossy(),
                "symbolName": "Shape", "line": line
            }),
        )
    };
    let at_declaration = ask("fx/types.ts", 1);
    let at_use = ask("user/tri.ts", 2);
    assert!(
        !located(&at_use)
            .iter()
            .any(|(file, _, _)| file == "model.ts"),
        "another project's same-named type: {at_use}"
    );
    assert_eq!(located(&at_use), located(&at_declaration), "{at_use}");
}
