//! Import tallies, coverage gaps, and JS/TS path and package resolution.
use super::*;

#[test]
fn a_relative_path_that_leaves_and_reenters_the_scan_root_links_by_normalised_path() {
    let temp = fixture(&[
        ("packages/config/src/home.ts", "export const home = 1;\n"),
        (
            "packages/agents/tests/native.test.mjs",
            "import { home } from '../../../packages/config/src/home.ts';\nexport const t = home;\n",
        ),
        // Leaves the scan root and enters a directory that is not in it.
        (
            "packages/agents/tests/outside.test.mjs",
            "import { x } from '../../../other/x.ts';\nexport const o = x;\n",
        ),
        ("other/x.ts", "export const x = 1;\n"),
    ]);
    let root = temp.0.as_path();
    let out = ingest(root, &root.join("packages"));
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    let dependents = ask(root, "dependents", Some("config/src/home.ts"));
    assert_eq!(
        files(&dependents),
        ["agents/tests/native.test.mjs"],
        "re-entering importer is a dependent: {dependents}"
    );
    // Only the path that never re-enters the root stays an unresolved gap.
    assert_eq!(unresolved_internal(root), 1);
}

#[test]
fn declaration_siblings_resolve_like_the_typescript_extension_swap() {
    let temp = fixture(&[
        (
            "src/main.ts",
            "import { a } from './a.js';\nimport { b } from './b.mjs';\nimport { c } from './c.cjs';\nimport { d } from './d.js';\nimport { e } from './e';\nexport const all = [a, b, c, d, e];\n",
        ),
        ("src/a.d.ts", "export declare const a: number;\n"),
        ("src/b.d.mts", "export declare const b: number;\n"),
        ("src/c.d.cts", "export declare const c: number;\n"),
        ("src/d.tsx", "export const d = 1;\n"),
        ("src/e.d.ts", "export declare const e: number;\n"),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("src/main.ts"));
    assert_eq!(
        files(&deps),
        [
            "src/a.d.ts",
            "src/b.d.mts",
            "src/c.d.cts",
            "src/d.tsx",
            "src/e.d.ts"
        ],
        "{deps}"
    );
    assert_eq!(unresolved_internal(root), 0);
}

/// Import tallies count statements, as the diagnostics do: one statement
/// binding three names is one unresolved import.
#[test]
fn import_counters_count_statements_like_the_diagnostics() {
    let temp = fixture(&[
        (
            "src/main.ts",
            "import { a, b, c } from './missing';\nimport d from './also-missing';\nimport { x, y } from './ok';\nexport const all = [a, b, c, d, x, y];\n",
        ),
        ("src/ok.ts", "export const x = 1;\nexport const y = 2;\n"),
    ]);
    let root = ingested(&temp);
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["unresolvedInternal"], 2, "{imports}");
    assert_eq!(imports["resolved"], 1, "{imports}");
    let diagnostics = ask(root, "diagnostics", Some("unresolved-internal"));
    assert_eq!(diagnostics["total"], 2, "{diagnostics}");
}

/// Relative JSON, style and asset imports are no linking gap.
#[test]
fn non_code_imports_are_not_unresolved_coverage_gaps() {
    let temp = fixture(&[
        (
            "a.ts",
            "import pkg from './package.json';\nimport './a.css';\nimport logo from './logo.svg?url';\nimport { b } from './b';\nexport const a = b;\n",
        ),
        ("b.ts", "import { a } from './a';\nexport const b = 1;\n"),
    ]);
    let root = ingested(&temp);
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["unresolvedInternal"], 0, "{imports}");
    assert_eq!(imports["nonCode"], 3, "{imports}");
    assert_eq!(
        cycle_members(&ask(root, "cycles", None)),
        [json!(["a.ts", "b.ts"])]
    );
}

/// The syntax-only basis is stated once by the evidence, not as a
/// diagnostic of every file.
#[test]
fn syntax_basis_is_not_repeated_as_a_diagnostic_for_every_file() {
    let temp = fixture(&[
        ("one.py", "def one(): pass\n"),
        ("two.py", "def two(): pass\n"),
    ]);
    let root = ingested(&temp);
    let diagnostics = ask(root, "diagnostics", None);
    assert_eq!(diagnostics["total"], 0, "{diagnostics}");
}

/// A file the scan could not read stays a node marked unparsed: an import
/// of it is an `unparsed-target` edge, never a resolved import, and every
/// answer names the skip.
#[test]
fn skipped_import_target_is_an_unparsed_node_not_a_resolved_import() {
    let temp = fixture(&[("entry.ts", "import './unread';\n")]);
    temp.write("unread.ts", [0xff]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("entry.ts"));
    let unread = result_row(&deps, "unread.ts");
    assert_eq!(unread["via"], "unparsed-target", "{deps}");
    assert_eq!(unread["roles"], json!(["unparsed"]), "{deps}");
    assert_eq!(
        deps["graph"]["partialReasons"],
        json!(["filesSkipped"]),
        "{deps}"
    );
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["resolved"], 0, "{imports}");
    assert_eq!(imports["unresolvedInternal"], 1, "{imports}");
}

#[test]
fn nested_workspace_package_import_resolves_to_scanned_source() {
    let temp = fixture(&[
        (
            "packages/lib/package.json",
            r#"{"name":"@fixture/lib","exports":"./src/index.ts"}"#,
        ),
        ("packages/lib/src/index.ts", "export const value = 1;\n"),
        (
            "entry.ts",
            "import { value } from '@fixture/lib';\nconsole.log(value);\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("entry.ts"));
    assert_eq!(files(&deps), ["packages/lib/src/index.ts"], "{deps}");
}

#[test]
fn subtree_scan_resolves_package_imports_through_the_outer_manifest() {
    let temp = fixture(&[
        (
            "pkg/package.json",
            r##"{"name":"pkg","imports":{"#enums/*":"./src/enums/*.ts","#outside":"./lib/x.js"}}"##,
        ),
        ("pkg/src/enums/flags.ts", "export const F = 1;\n"),
        (
            "pkg/src/main.ts",
            "import { F } from '#enums/flags';\nexport const m = F;\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("pkg/src"));
    let dependents = ask(root, "dependents", Some("enums/flags.ts"));
    assert_eq!(files(&dependents), ["main.ts"], "{dependents}");
    assert_eq!(unresolved_internal(root), 0);
}

#[test]
fn outer_manifest_lookup_stops_at_the_repository_boundary() {
    let temp = fixture(&[
        (
            "package.json",
            r##"{"name":"outer","imports":{"#enums/*":"./repo/src/enums/*.ts"}}"##,
        ),
        ("repo/src/enums/flags.ts", "export const F = 1;\n"),
        (
            "repo/src/main.ts",
            "import { F } from '#enums/flags';\nexport const m = F;\n",
        ),
    ]);
    let root = temp.0.as_path();
    std::fs::create_dir_all(root.join("repo/.git")).expect("repository marker");
    ingest(root, &root.join("repo/src"));
    let dependents = ask(root, "dependents", Some("enums/flags.ts"));
    assert!(files(&dependents).is_empty(), "{dependents}");
    assert_eq!(unresolved_internal(root), 1);
}

#[test]
fn path_edges_carry_import_lines() {
    let temp = fixture(&[
        (
            "a.ts",
            "// head\nimport { b } from './b';\nexport const a = b;\n",
        ),
        ("b.ts", "export const b = 1;\n"),
    ]);
    let root = ingested(&temp);
    let path = ask_with(root, "path", Some("a.ts"), |o| o.to = Some("b.ts".into()));
    assert_eq!(path["path"][1]["edgeLine"], 2, "{path}");
}
