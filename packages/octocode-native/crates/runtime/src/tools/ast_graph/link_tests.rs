//! Linking across languages, asked the way a caller asks: `graph ingest`
//! a fixture, then `graph query` its edges, cycles and import tallies.
use super::test_support::*;
use serde_json::{Value, json};
use std::path::Path;

/// Ingests `root` (as its own workspace) and returns it.
fn ingested(temp: &Fixture) -> &Path {
    let root = temp.0.as_path();
    let out = ingest(root, root);
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    root
}

/// Files (not packages or symbols) of a traversal answer.
fn files(out: &Value) -> Vec<&str> {
    result_ids(out)
        .into_iter()
        .filter(|id| !id.starts_with("pkg:") && !id.contains('#'))
        .collect()
}

fn unresolved_internal(root: &Path) -> Value {
    ask(root, "stats", None)["imports"]["unresolvedInternal"].clone()
}

/// The cap counts distinct file edges: a file naming its one dependency
/// many times is one edge, so a cap of one edge holds it whole.
#[test]
fn repeated_references_to_one_target_count_as_one_edge() {
    struct EdgeCap;
    impl Drop for EdgeCap {
        fn drop(&mut self) {
            super::graph::EDGE_CAP.with(|edges| edges.set(super::graph::MAX_GRAPH_EDGES));
        }
    }
    super::graph::EDGE_CAP.with(|edges| edges.set(1));
    let _restore = EdgeCap;
    let temp = fixture(&[
        (
            "a.ts",
            "import { b } from './b'\nimport { c } from './b'\nexport const a = b + c\n",
        ),
        ("b.ts", "export const b = 1\nexport const c = 2\n"),
    ]);
    let out = ingest(temp.0.as_path(), temp.0.as_path());
    assert_eq!(out.value["scan"]["edgesCapped"], false, "{}", out.value);
}

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

// ── Rust ─────────────────────────────────────────────────────────────────────

/// A `mod` tree loops by construction (the parent declares the child, the
/// child uses its parent): no cycle. Two modules using each other are one.
#[test]
fn rust_module_tree_loops_are_not_cycles_but_use_cycles_are() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod tools;\npub mod a;\npub mod b;\npub fn root() {}\n",
        ),
        ("src/tools/mod.rs", "pub mod inner;\npub fn shared() {}\n"),
        (
            "src/tools/inner.rs",
            "use super::shared;\npub fn run() { shared(); }\n",
        ),
        ("src/a.rs", "use crate::b::bee;\npub fn ay() { bee(); }\n"),
        ("src/b.rs", "use crate::a::ay;\npub fn bee() { ay(); }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        cycle_members(&ask(root, "cycles", None)),
        [json!(["src/a.rs", "src/b.rs"])]
    );
}

/// `include_str!` embeds a file as data: a dependency its dependents see,
/// but never a loop in the code graph.
#[test]
fn rust_data_includes_link_files_but_close_no_cycle() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        (
            "src/a.rs",
            "pub const SAMPLE: &str = include_str!(\"b.rs\");\npub fn ay() {}\n",
        ),
        ("src/b.rs", "use crate::a::ay;\npub fn bee() { ay(); }\n"),
    ]);
    let root = ingested(&temp);
    assert!(cycle_members(&ask(root, "cycles", None)).is_empty());
    let dependents = ask(root, "dependents", Some("src/b.rs"));
    assert_eq!(
        result_row(&dependents, "src/a.rs")["via"],
        "rust-include-data",
        "{dependents}"
    );
}

#[test]
fn rust_qualified_path_calls_are_dependencies_that_close_cycles() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        // `a` names `b` only through a path in a body; `b` imports `a`.
        ("src/a.rs", "pub fn ay() { crate::b::bee(); }\n"),
        (
            "src/b.rs",
            "use crate::a::ay;\npub fn bee() {}\npub fn other() { ay(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        cycle_members(&ask(root, "cycles", None)),
        [json!(["src/a.rs", "src/b.rs"])]
    );
    // The dependency cites the path's line as its evidence.
    let deps = ask(root, "deps", Some("src/a.rs"));
    assert_eq!(result_row(&deps, "src/b.rs")["edgeLine"], 1, "{deps}");
}

/// Called module paths, and qualified paths outside a call (a constant, a
/// function value, a path in a macro body, a type), are edges to their
/// module.
#[test]
fn rust_qualified_paths_link_to_their_module() {
    let temp = fixture(&[
        CARGO_APP,
        (
            "src/lib.rs",
            "pub mod tools;\npub mod portable;\npub mod rules;\npub mod user;\npub mod api;\n",
        ),
        ("src/tools/mod.rs", "pub mod inner;\n"),
        ("src/tools/inner.rs", "pub fn used() {}\n"),
        ("src/portable.rs", "pub fn sanitize() {}\n"),
        (
            "src/api.rs",
            "pub fn handle() {\n    crate::portable::sanitize();\n    crate::tools::inner::used();\n}\n",
        ),
        (
            "src/rules.rs",
            "pub const HINT: &str = \"h\";\npub fn globs(x: u8) -> u8 { x }\npub fn recovery() -> u8 { 1 }\npub struct Out;\n",
        ),
        (
            "src/user.rs",
            "pub fn run() -> Option<crate::rules::Out> {\n    let _ = crate::rules::HINT;\n    let _ = [1u8].map(crate::rules::globs);\n    let _ = format!(\"{}\", crate::rules::recovery());\n    None\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("src/api.rs"));
    assert_eq!(
        files(&deps),
        ["src/portable.rs", "src/tools/inner.rs"],
        "{deps}"
    );
    let deps = ask(root, "deps", Some("src/user.rs"));
    assert_eq!(files(&deps), ["src/rules.rs"], "{deps}");
    assert_eq!(unresolved_internal(root), 0);
}

/// A `crate::` path that cannot be linked is an internal gap, never a
/// complete graph.
#[test]
fn rust_unlinkable_crate_paths_count_as_unresolved_internal() {
    let temp = fixture(&[(
        "src/user.rs",
        "pub fn run() -> u8 { let _ = crate::missing::LIMIT; 1 }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(unresolved_internal(root), 1);
}

/// A scan of a directory inside a crate (its `src/lib.rs` above the scan)
/// links `crate::` paths into the scanned modules from the scan root; a
/// path that leaves the scanned modules stays an unresolved gap.
#[test]
fn crate_paths_link_inside_a_subdirectory_scan() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod tools;\n"),
        (
            "src/tools/mod.rs",
            "pub mod a;\npub mod b;\npub const TOP: u8 = 1;\n",
        ),
        (
            "src/tools/a.rs",
            "use crate::tools::b::helper;\npub fn run() -> u8 { helper() + crate::tools::TOP + crate::config::LIMIT }\n",
        ),
        ("src/tools/b.rs", "pub fn helper() -> u8 { 1 }\n"),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("src/tools"));
    let deps = ask(root, "deps", Some("a.rs"));
    assert_eq!(files(&deps), ["b.rs", "mod.rs"], "{deps}");
    assert_eq!(unresolved_internal(root), 1);
}

#[test]
fn rust_super_past_an_inline_module_climbs_from_the_enclosing_module() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod x;\npub mod z;\n"),
        ("src/z.rs", "pub fn top() {}\n"),
        ("src/x/mod.rs", "pub mod y;\npub mod z;\n"),
        ("src/x/z.rs", "pub fn near() {}\n"),
        // Inside `mod tests` (one inline level), `super::super` is the
        // parent of the file's module: `crate::x`, so `z` is `x/z.rs`.
        (
            "src/x/y.rs",
            "pub fn y() {}\n#[cfg(test)]\nmod tests {\n    use super::super::z::near;\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("src"));
    let deps = ask(root, "deps", Some("x/y.rs"));
    assert_eq!(files(&deps), ["x/z.rs"], "{deps}");
}

/// A file's dependents include the modules that use its items through a
/// `pub use` re-export, and only those naming a re-exported item.
#[test]
fn dependents_follow_rust_pub_use_reexports() {
    let temp = fixture(&[
        CARGO_APP,
        ("src/lib.rs", "pub mod sync;\npub mod task;\n"),
        (
            "src/sync/mod.rs",
            "mod notify;\npub use notify::Notify;\nmod broadcast;\nmod other;\npub struct Other;\n",
        ),
        ("src/sync/notify.rs", "pub struct Notify;\n"),
        (
            "src/sync/broadcast.rs",
            "use super::Notify;\npub fn b(_: Notify) {}\n",
        ),
        (
            "src/sync/other.rs",
            "use super::Other;\npub fn o(_: Other) {}\n",
        ),
        ("src/task/mod.rs", "pub mod local;\n"),
        (
            "src/task/local.rs",
            "use crate::sync::Notify;\npub fn l(_: Notify) {}\n",
        ),
    ]);
    let root = ingested(&temp);
    let dependents = ask(root, "dependents", Some("src/sync/notify.rs"));
    let mut found = files(&dependents);
    found.sort_unstable();
    assert_eq!(
        found,
        [
            "src/sync/broadcast.rs",
            "src/sync/mod.rs",
            "src/task/local.rs"
        ],
        "{dependents}"
    );
    assert_eq!(
        result_row(&dependents, "src/task/local.rs")["via"],
        "reexport",
        "{dependents}"
    );
}

// ── Go, Python, Java ─────────────────────────────────────────────────────────

#[test]
fn go_package_imports_link_every_package_file_with_its_import_line() {
    let temp = fixture(&[
        ("go.mod", "module example.com/app\n"),
        (
            "main.go",
            "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/app/tsdb\"\n)\n\nfunc main() { fmt.Println(tsdb.Open()) }\n",
        ),
        (
            "tsdb/db.go",
            "package tsdb\n\nfunc Open() int { return 1 }\n",
        ),
        ("tsdb/head.go", "package tsdb\n\ntype Head struct{}\n"),
        ("tsdb/db_test.go", "package tsdb\n"),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("main.go"));
    assert_eq!(files(&deps), ["tsdb/db.go", "tsdb/head.go"], "{deps}");
    for file in ["tsdb/db.go", "tsdb/head.go"] {
        assert_eq!(result_row(&deps, file)["edgeLine"], 5, "{deps}");
    }
    let dependents = ask(root, "dependents", Some("tsdb/head.go"));
    assert_eq!(files(&dependents), ["main.go"], "{dependents}");
}

/// A scan rooted below the module root still reads the enclosing `go.mod`.
#[test]
fn go_subdirectory_roots_resolve_through_the_enclosing_go_mod() {
    let temp = fixture(&[
        ("go.mod", "module example.com/app\n"),
        (
            "tsdb/chunkenc/chunk.go",
            "package chunkenc\n\nfunc New() int { return 1 }\n",
        ),
        (
            "tsdb/db.go",
            "package tsdb\n\nimport \"example.com/app/tsdb/chunkenc\"\n\nfunc Open() int { return chunkenc.New() }\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("tsdb"));
    let dependents = ask(root, "dependents", Some("chunkenc/chunk.go"));
    assert_eq!(files(&dependents), ["db.go"], "{dependents}");
}

/// A scan rooted at a Python package resolves absolute imports that name
/// that package (`from pkg.utils.text import x` under root `pkg/`).
#[test]
fn python_package_roots_resolve_imports_through_their_package_name() {
    let temp = fixture(&[
        ("pkg/__init__.py", ""),
        ("pkg/utils/__init__.py", ""),
        ("pkg/utils/text.py", "def slug(x):\n    return x\n"),
        (
            "pkg/admin.py",
            "from pkg.utils.text import slug\n\nprint(slug('a'))\n",
        ),
    ]);
    let root = temp.0.as_path();
    ingest(root, &root.join("pkg"));
    let dependents = ask(root, "dependents", Some("utils/text.py"));
    assert_eq!(files(&dependents), ["admin.py"], "{dependents}");
}

/// Java classes of one package use each other without an import; the
/// edge cites the first use, an imported class the import.
#[test]
fn java_same_package_and_imported_classes_link_to_their_files() {
    let temp = fixture(&[
        (
            "src/com/acme/Lists.java",
            "package com.acme;\n\npublic final class Lists {\n  public static int transform(int x) { return x; }\n}\n",
        ),
        (
            "src/com/acme/util/Strings.java",
            "package com.acme.util;\n\npublic class Strings {}\n",
        ),
        (
            "src/com/acme/Multimaps.java",
            "package com.acme;\n\nimport com.acme.util.Strings;\nimport java.util.List;\n\nfinal class Multimaps {\n  int run() { return Lists.transform(1); }\n}\n",
        ),
        (
            "src/com/acme/Other.java",
            "package com.acme;\n\nfinal class Other {\n  int run() { return 2; }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let dependents = ask(root, "dependents", Some("src/com/acme/Lists.java"));
    assert_eq!(
        files(&dependents),
        ["src/com/acme/Multimaps.java"],
        "{dependents}"
    );
    let deps = ask(root, "deps", Some("src/com/acme/Multimaps.java"));
    let same = result_row(&deps, "src/com/acme/Lists.java");
    assert_eq!(same["via"], "java-same-package", "{deps}");
    assert_eq!(same["edgeLine"], 7, "{deps}");
    let imported = result_row(&deps, "src/com/acme/util/Strings.java");
    assert_eq!(imported["via"], "java-import", "{deps}");
    assert_eq!(imported["edgeLine"], 3, "{deps}");
}

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
