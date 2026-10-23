//! Dead code through re-export and alias resolution.
use super::*;

// ── Dead code through the re-export and alias resolution ─────────────────────

/// Unused exports of a fixture whose entry is `main.ts`, as `file#name`.
fn unused_exports(files: &[(&str, &str)]) -> Vec<String> {
    let temp = fixture(files);
    temp.write("package.json", r#"{"name":"app","main":"main.ts"}"#);
    let root = ingested(&temp);
    let out = ask_with(root, "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(0);
    });
    let mut unused = out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            let file = row["subject"].as_str().unwrap_or_default().to_owned();
            row["evidence"]["exports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str())
                .map(move |name| format!("{file}#{name}"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    unused.sort();
    unused
}

/// `export { foo as bar }`: importing `bar` uses `foo`; importing only
/// `kept` leaves the renamed export unused.
#[test]
fn renamed_exports_are_used_through_their_public_alias() {
    let module = "export function kept() { return 1 }\nfunction foo() { return 2 }\nfunction bar() { return 3 }\nexport { foo as bar }\n";
    assert_eq!(
        unused_exports(&[
            ("mod.ts", module),
            ("main.ts", "import { kept } from './mod'\nkept()\n"),
        ]),
        ["mod.ts#foo"]
    );
    assert_eq!(
        unused_exports(&[
            ("mod.ts", module),
            ("main.ts", "import { bar } from './mod'\nbar()\n"),
        ]),
        ["mod.ts#kept"]
    );
}

/// A default import uses the default export; a namespace import and a
/// re-export chain use what they reach.
#[test]
fn default_namespace_and_reexported_imports_use_their_exports() {
    let unused = unused_exports(&[
        (
            "def.ts",
            "export default function foo() { return 1 }\nexport function other() { return 2 }\n",
        ),
        ("ns.ts", "export const one = 1;\nexport const two = 2;\n"),
        (
            "inner.ts",
            "export const passed = 1;\nexport const idle = 2;\n",
        ),
        ("barrel.ts", "export { passed, idle } from './inner';\n"),
        (
            "main.ts",
            "import foo from './def'\nimport * as ns from './ns'\nimport { passed } from './barrel'\nfoo(); ns.one; passed;\n",
        ),
    ]);
    assert_eq!(unused, ["def.ts#other", "inner.ts#idle"]);
}

/// The `unlinkedPathImports` rows of a `dependents` answer.
fn unlinked_path_imports(out: &Value) -> Vec<(String, u64)> {
    out["summary"]["unlinkedPathImports"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["file"].as_str().unwrap_or_default().to_owned(),
                row["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

/// A bare import that names a scanned file by its path (`vs/base/common/x`
/// for `src/vs/base/common/x.ts`, `src/dir` for `src/dir/index.ts`) but that
/// no tsconfig `paths`/`baseUrl` or workspace package links is no dependent
/// edge; `dependents` of that file lists it, so the answer never silently
/// lacks a file a text search finds. Package imports stay external.
#[test]
fn dependents_list_bare_path_imports_no_config_links() {
    let temp = fixture(&[
        ("src/vs/base/common/x.ts", "export const x = 1;\n"),
        ("src/dir/index.ts", "export const d = 1;\n"),
        (
            "src/app.ts",
            "import React from 'react';\nimport { x } from 'vs/base/common/x';\nconsole.log(React, x);\n",
        ),
        (
            "src/main.ts",
            "import { x } from './vs/base/common/x';\nimport { d } from 'src/dir';\nimport fp from 'lodash/fp';\nconsole.log(x, d, fp);\n",
        ),
    ]);
    let root = ingested(&temp);
    let x = ask(root, "dependents", Some("src/vs/base/common/x.ts"));
    assert_eq!(files(&x), ["src/main.ts"], "{x}");
    assert_eq!(
        unlinked_path_imports(&x),
        [("src/app.ts".to_owned(), 2)],
        "{x}"
    );
    let row = &x["summary"]["unlinkedPathImports"][0];
    assert!(
        row["message"]
            .as_str()
            .is_some_and(|m| m.contains("\"vs/base/common/x\"")),
        "{row}"
    );
    let dir = ask(root, "dependents", Some("src/dir/index.ts"));
    assert_eq!(files(&dir), Vec::<&str>::new(), "{dir}");
    assert_eq!(
        unlinked_path_imports(&dir),
        [("src/main.ts".to_owned(), 2)],
        "{dir}"
    );
    // A dependents answer with none states none.
    let app = ask(root, "dependents", Some("src/app.ts"));
    assert_eq!(app["summary"], Value::Null, "{app}");
    // Recorded at ingest as a typed diagnostic that names its target; the
    // import still counts as external.
    let diagnostics = ask(root, "diagnostics", Some("unlinked-path-import"));
    let rows = diagnostics["results"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{diagnostics}");
    assert_eq!(rows[0]["file"], "src/app.ts", "{diagnostics}");
    assert_eq!(
        rows[0]["target"], "src/vs/base/common/x.ts",
        "{diagnostics}"
    );
    assert_eq!(rows[1]["target"], "src/dir/index.ts", "{diagnostics}");
    assert_eq!(ask(root, "stats", None)["imports"]["external"], 4);
}

/// The same specifier linked by a tsconfig `baseUrl` is a dependent edge
/// and no diagnostic.
#[test]
fn a_bare_path_a_tsconfig_links_is_a_dependent_not_a_diagnostic() {
    let temp = fixture(&[
        ("tsconfig.json", r#"{"compilerOptions":{"baseUrl":"src"}}"#),
        ("src/vs/base/common/x.ts", "export const x = 1;\n"),
        (
            "src/app.ts",
            "import { x } from 'vs/base/common/x';\nconsole.log(x);\n",
        ),
    ]);
    let root = ingested(&temp);
    let x = ask(root, "dependents", Some("src/vs/base/common/x.ts"));
    assert_eq!(files(&x), ["src/app.ts"], "{x}");
    assert_eq!(x["summary"], Value::Null, "{x}");
    let diagnostics = ask(root, "diagnostics", Some("unlinked-path-import"));
    assert_eq!(diagnostics["total"], 0, "{diagnostics}");
}
