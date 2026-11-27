//! Import tallies, coverage gaps, and JS/TS path and package resolution.
use super::*;
use octocode_test_support::json_fixture;

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
        ["packages/agents/tests/native.test.mjs"],
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
    assert_eq!(diagnostics["pagination"]["total"], 2, "{diagnostics}");
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
        [json_fixture(r#"["a.ts", "b.ts"]"#)]
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
    assert_eq!(diagnostics["pagination"]["total"], 0, "{diagnostics}");
}

/// A file the scan could not read stays a node marked unparsed: an import
/// of it is an `unparsed-target` edge, never a resolved import, and every
/// answer names the skip.
#[test]
fn skipped_import_target_is_an_unparsed_node_not_a_resolved_import() {
    let temp = fixture(&[("entry.ts", "import './unread';\n")]);
    temp.write("unread.ts", [0x00]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("entry.ts"));
    let unread = result_row(&deps, "unread.ts");
    assert_eq!(unread["via"], "unparsed-target", "{deps}");
    assert_eq!(unread["roles"], json_fixture(r#"["unparsed"]"#), "{deps}");
    assert_eq!(
        deps["graph"]["partialReasons"],
        json_fixture(r#"["filesSkipped"]"#),
        "{deps}"
    );
    let imports = &ask(root, "stats", None)["imports"];
    // The relink decides once: the import is linked, so it is neither
    // counted nor diagnosed as unresolved.
    assert_eq!(imports["resolved"], 1, "{imports}");
    assert_eq!(imports["unresolvedInternal"], 0, "{imports}");
    let diagnostics = ask(root, "diagnostics", Some("unresolved-internal"));
    assert_eq!(diagnostics["pagination"]["total"], 0, "{diagnostics}");
}

/// The tally and the diagnostics are one fact: a missing file stays
/// unresolved in both, an unparsed one in neither.
#[test]
fn unresolved_internal_count_matches_its_diagnostics_beside_an_unparsed_target() {
    let temp = fixture(&[("entry.ts", "import './unread';\nimport './missing';\n")]);
    temp.write("unread.ts", [0xff]);
    let root = ingested(&temp);
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["unresolvedInternal"], 1, "{imports}");
    assert_eq!(imports["resolved"], 1, "{imports}");
    let diagnostics = ask(root, "diagnostics", Some("unresolved-internal"));
    assert_eq!(diagnostics["pagination"]["total"], 1, "{diagnostics}");
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
    assert_eq!(files(&dependents), ["pkg/src/main.ts"], "{dependents}");
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

/// Node resolves a package's own name from inside it (`require('lodash')` in
/// lodash) to itself only through self-reference, which needs `exports`
/// in its `package.json`, or through the workspace link of a workspace
/// member; otherwise the name is the installed copy in `node_modules`.
#[test]
fn a_package_names_itself_only_with_exports_or_as_a_workspace_member() {
    let temp = fixture(&[
        (
            "package.json",
            r#"{"name":"lodash","main":"lodash.js","devDependencies":{"lodash":"4.17.20"}}"#,
        ),
        ("lodash.js", "module.exports = function isObject() {};\n"),
        (
            "lib/minify.js",
            "const _ = require('lodash');\nmodule.exports = _;\n",
        ),
        (
            "exported/package.json",
            r#"{"name":"exported","exports":"./index.js"}"#,
        ),
        ("exported/index.js", "module.exports = 1;\n"),
        (
            "exported/lib/self.js",
            "const self = require('exported');\nmodule.exports = self;\n",
        ),
        (
            "mono/package.json",
            r#"{"name":"mono","workspaces":["packages/*"]}"#,
        ),
        (
            "mono/packages/member/package.json",
            r#"{"name":"member","main":"index.js"}"#,
        ),
        ("mono/packages/member/index.js", "module.exports = 1;\n"),
        (
            "mono/packages/member/lib/self.js",
            "const self = require('member');\nmodule.exports = self;\n",
        ),
    ]);
    let root = ingested(&temp);
    let deps = ask(root, "deps", Some("lib/minify.js"));
    assert!(files(&deps).is_empty(), "{deps}");
    let deps = ask(root, "deps", Some("exported/lib/self.js"));
    assert_eq!(files(&deps), ["exported/index.js"], "{deps}");
    let deps = ask(root, "deps", Some("mono/packages/member/lib/self.js"));
    assert_eq!(files(&deps), ["mono/packages/member/index.js"], "{deps}");
    // The installed copy is a third-party package, not a linking gap.
    assert_eq!(unresolved_internal(root), 0);
}

const MEMBER_WORKSPACE: &[(&str, &str)] = &[
    (
        "mono/package.json",
        r#"{"name":"mono","workspaces":["packages/*"]}"#,
    ),
    (
        "mono/packages/member/package.json",
        r#"{"name":"member","main":"index.js"}"#,
    ),
    ("mono/packages/member/index.js", "module.exports = 1;\n"),
    (
        "mono/packages/member/lib/self.js",
        "const self = require('member');\nmodule.exports = self;\n",
    ),
    (
        "mono/packages/other/package.json",
        r#"{"name":"other","main":"index.js"}"#,
    ),
    ("mono/packages/other/index.js", "module.exports = 1;\n"),
];

/// A scan rooted at a workspace member reaches the member by its own name as
/// Node does: through the workspace above it, or, when the path policy
/// keeps that manifest unread, through the `node_modules` link the install
/// made. With neither, membership is unknown: the import is an internal gap,
/// never a third-party package.
#[test]
fn a_workspace_member_scanned_alone_names_itself() {
    let temp = fixture(MEMBER_WORKSPACE);
    let root = temp.0.as_path();
    let member = root.join("mono/packages/member");
    // The workspace above the scan is readable.
    let out = ingest(root, &member);
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    let deps = ask(root, "deps", Some("lib/self.js"));
    assert_eq!(files(&deps), ["mono/packages/member/index.js"], "{deps}");
    // Only the member is readable, and nothing is installed.
    let out = ingest(&member, &member);
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    let deps = ask(&member, "deps", Some("lib/self.js"));
    assert!(files(&deps).is_empty(), "{deps}");
    assert_eq!(unresolved_internal(&member), 1);
    // Installed: the workspace links the member into `node_modules`.
    #[cfg(unix)]
    {
        std::fs::create_dir_all(root.join("mono/node_modules")).expect("node_modules");
        std::os::unix::fs::symlink("../packages/member", root.join("mono/node_modules/member"))
            .expect("workspace link");
        let out = ingest(&member, &member);
        assert_eq!(out.value.get("error"), None, "{}", out.value);
        let deps = ask(&member, "deps", Some("lib/self.js"));
        assert_eq!(files(&deps), ["index.js"], "{deps}");
        assert_eq!(unresolved_internal(&member), 0);
    }
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

/// The agents-communication shape: `bin/cli.mjs` imports its build output
/// `../dist/cli.js` (pruned from the scan) and `database.ts` imports
/// `../../assets/schema.sql` (a loader asset no grammar parses). Both files
/// exist on disk under the root, so neither is a linking gap nor an
/// `unresolved-import` finding; an import of a missing file still is.
#[test]
fn files_on_disk_the_scan_does_not_hold_are_no_linking_gap() {
    let temp = fixture(&[
        ("bin/cli.mjs", "import '../dist/cli.js';\n"),
        ("dist/cli.js", "export const built = 1;\n"),
        (
            "src/runtime/database.ts",
            "import schemaSql from '../../assets/schema.sql';\nimport { gone } from './missing';\nexport const schema = [schemaSql, gone];\n",
        ),
        ("assets/schema.sql", "CREATE TABLE t (id INTEGER);\n"),
    ]);
    let root = ingested(&temp);
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["unresolvedInternal"], 1, "{imports}");
    assert_eq!(imports["unscanned"], 2, "{imports}");
    let issues = ask_with(root, "issues", None, |o| {
        o.detectors = vec!["unresolved-import".into()];
        o.min_tier = Some(0);
    });
    let rows = issues["rows"].as_array().cloned().unwrap_or_default();
    let subjects = rows
        .iter()
        .filter_map(|row| row["subject"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, ["src/runtime/database.ts"], "{issues}");
    assert!(
        rows.iter()
            .all(|row| !row.to_string().contains("schema.sql")),
        "{issues}"
    );
}

/// A file that imports one missing module at several lines is one finding
/// whose evidence names the unresolved import once with every line, so a page
/// of findings costs by distinct imports, not by repeated message text, and no
/// site is dropped.
#[test]
fn unresolved_import_evidence_groups_sites_by_message_and_keeps_every_line() {
    let temp = fixture(&[(
        "src/a.ts",
        "import { one } from './gone';\nimport { two } from './gone';\nimport { three } from './other';\nexport const all = [one, two, three];\n",
    )]);
    let root = ingested(&temp);
    let issues = ask_with(root, "issues", None, |o| {
        o.detectors = vec!["unresolved-import".into()];
        o.min_tier = Some(0);
    });
    let evidence = &issues["rows"][0]["evidence"];
    assert_eq!(evidence["count"], 3, "{issues}");
    let sites = evidence["sites"].as_array().expect("sites");
    assert_eq!(sites.len(), 2, "{issues}");
    let lines = |index: usize| {
        sites[index]["lines"]
            .as_array()
            .map(|lines| lines.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
    };
    assert_eq!(lines(0), Some(vec![1, 2]), "{issues}");
    assert_eq!(lines(1), Some(vec![3]), "{issues}");
}

/// A path rooted at `::` names an extern crate (`::std::vec::Vec`,
/// `::serde_json::Value`), never the current one: it is no unresolved
/// internal import, in a `use`, a type position or an expression
/// (`crate::` gaps: `rust_unlinkable_crate_paths_count_as_unresolved_internal`).
#[test]
fn rooted_paths_name_extern_crates_not_unresolved_internal() {
    let temp = fixture(&[
        (
            "src/lib.rs",
            "use ::std::fmt::Display;\npub struct S { pub v: ::std::vec::Vec<u8> }\npub fn f() -> usize { ::core::mem::size_of::<u8>() + ::alloc::vec::Vec::<u8>::new().len() }\npub fn g<T: Display>(_: T) {}\n",
        ),
        (
            "src/extern_user.rs",
            "pub struct T { pub r: ::regress::Regex, pub v: ::serde_json::Value }\npub fn h() -> Option<::serde::de::IgnoredAny> { None }\n",
        ),
    ]);
    let root = ingested(&temp);
    let diagnostics = ask(root, "diagnostics", Some("unresolved-internal"));
    let messages = diagnostics["rows"].to_string();
    for extern_crate in ["std", "core", "alloc", "serde", "regress", "serde_json"] {
        assert!(
            !messages.contains(&format!("::{extern_crate}::")),
            "{extern_crate}: {diagnostics}"
        );
    }
    assert_eq!(diagnostics["rows"], json!([]), "{diagnostics}");
    let imports = &ask(root, "stats", None)["imports"];
    assert_eq!(imports["unresolvedInternal"], 0, "{imports}");
}

/// `(line, message, importKind)` of the `code` diagnostics of the snapshot
/// at `scope` (the workspace's when `None`), in line order.
fn import_kinds(root: &Path, scope: Option<&str>, code: &str) -> Vec<(u64, String, Value)> {
    let out = ask_with(root, "diagnostics", Some(code), |o| {
        o.path = scope.map(str::to_owned);
    });
    let mut rows = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["line"].as_u64().unwrap_or_default(),
                row["message"].as_str().unwrap_or_default().to_owned(),
                row.get("importKind").cloned().unwrap_or(Value::Null),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.0);
    rows
}

/// An import that leaves the snapshot root keeps its edge kind: each
/// `outside-root` row names it as `importKind`, the same string the edge of
/// that import carries in a snapshot that holds its target, so stitching
/// chunks keeps an `import type` apart from a load. A statement mixing kinds
/// (`import { type W, w }`) is one row with its strongest kind, as its file
/// edge is.
#[test]
fn outside_root_diagnostics_carry_the_import_kind_of_their_edge() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        ("outside/a.ts", "export interface X { x: number }\n"),
        ("outside/b.ts", "export const y = 1;\n"),
        ("outside/c.ts", "export const z = 1;\n"),
        (
            "outside/d.ts",
            "export interface W { w: number }\nexport const w = 1;\n",
        ),
        (
            "app/main.ts",
            "import type { X } from '../outside/a';\nimport { y } from '../outside/b';\nimport { type W, w } from '../outside/d';\nexport async function run(x: X, v: W) {\n  const m = await import('../outside/c');\n  return [x, y, m, v, w];\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    // The whole tree holds every target: each import is a linked edge.
    let whole = ingest(root, root);
    assert_eq!(whole.value.get("error"), None, "{}", whole.value);
    let deps = ask(root, "deps", Some("app/main.ts"));
    let mut linked = deps["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (text("id"), text("via"))
        })
        .collect::<Vec<_>>();
    linked.sort();
    // The chunk `app` holds none of them.
    let chunk = ingest(root, &root.join("app"));
    assert_eq!(chunk.value.get("error"), None, "{}", chunk.value);
    let rows = import_kinds(root, Some("app"), "outside-root");
    let kinds = rows
        .iter()
        .map(|(line, _, kind)| (*line, kind.as_str().unwrap_or_default()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            (1, "type-import"),
            (2, "static-import"),
            (3, "static-import"),
            (5, "dynamic-import"),
        ],
        "{rows:?}"
    );
    // Each row's kind is one the linked import's edge carries.
    for (_, message, kind) in &rows {
        let target = format!("{}.ts", message.trim_matches('"').trim_start_matches("../"));
        let kind = kind.as_str().unwrap_or_default();
        assert!(
            linked.contains(&(target.clone(), kind.to_owned())),
            "{target} {kind}: {linked:?}"
        );
    }
}

/// A broken import, Python or Rust, names its edge kind too: a Python
/// import inside a function is lazy, a Rust `use` and a qualified path keep
/// their own kinds. A diagnostic about no import names none.
#[test]
fn unresolved_internal_diagnostics_carry_the_import_kind() {
    let temp = fixture(&[
        ("py/__init__.py", ""),
        (
            "py/main.py",
            "from .missing import a\n\n\ndef f():\n    from .gone import b\n    return b\n",
        ),
        (
            "src/user.rs",
            "use crate::gone::Thing;\npub fn run() -> u8 { let _ = crate::missing::LIMIT; 1 }\n",
        ),
    ]);
    let root = ingested(&temp);
    // `(line, from the Python file, importKind)`.
    let kinds = import_kinds(root, None, "unresolved-internal")
        .into_iter()
        .map(|(line, message, kind)| {
            let python = message.contains("\".missing\"") || message.contains("\".gone\"");
            (line, python, kind)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            (1, true, json!("python-import")),
            (1, false, json!("rust-use")),
            (2, false, json!("rust-path")),
            (5, true, json!("python-lazy-import")),
        ],
        "{kinds:?}"
    );
    let all = ask(root, "diagnostics", None);
    for row in all["rows"].as_array().into_iter().flatten() {
        let import = matches!(
            row["code"].as_str(),
            Some("outside-root" | "unresolved-internal")
        );
        assert_eq!(row.get("importKind").is_some(), import, "{row}");
    }
}

/// `(to, line, detail, importKind)` of the `imports` witnesses out of `file`
/// whose target is a package, sorted.
fn package_witnesses(root: &Path, file: &str) -> Vec<(String, u64, String, Value)> {
    let out = ask_with(root, "witnesses", Some(file), |o| {
        o.edges = vec!["imports".into()];
    });
    assert_eq!(out.get("error"), None, "{out}");
    let mut rows = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["to"].as_str().is_some_and(|to| to.starts_with("pkg:")))
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (
                text("to"),
                row["line"].as_u64().unwrap_or_default(),
                text("detail"),
                row.get("importKind").cloned().unwrap_or(Value::Null),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    rows
}

/// An import of a package states its own kind on its witness
/// (`importKind`, the string a file import of the same form carries) while
/// `detail` keeps how the package is declared: an `import type` of a
/// package loads nothing, and a file that both type-imports and
/// value-imports one yields a witness of each kind.
#[test]
fn package_import_witnesses_carry_the_import_kind() {
    let temp = fixture(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"big-lib":"1.0.0","@scope/ai":"1.0.0"}}"#,
        ),
        (
            "src/main.ts",
            "import type { X } from '@scope/ai';\nimport { y } from '@scope/ai';\nimport big from 'big-lib';\nexport function run(x: X) { return [x, y, big]; }\n",
        ),
    ]);
    let root = ingested(&temp);
    let row =
        |to: &str, line: u64, kind: &str| (to.to_owned(), line, "external".to_owned(), json!(kind));
    assert_eq!(
        package_witnesses(root, "src/main.ts"),
        [
            row("pkg:@scope/ai", 1, "type-import"),
            row("pkg:@scope/ai", 2, "static-import"),
            row("pkg:big-lib", 3, "static-import"),
        ]
    );
}

/// Every form that loads a package is an import of it: a dynamic
/// `import()`, a `require` with or without a binding, a `createRequire`
/// loader and a re-export (`export { a } from`, `export * from`,
/// `export type { T } from`) each give the file a `pkg:` edge whose witness
/// names its kind, so a package loaded only lazily or only through a
/// re-export is still a dependency.
#[test]
fn every_import_form_of_a_package_is_a_package_edge_with_its_kind() {
    let temp = fixture(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"lazy-lib":"1","re-lib":"1","star-lib":"1","types-lib":"1","cjs-lib":"1","made-lib":"1"}}"#,
        ),
        (
            "src/main.ts",
            "export { a } from 're-lib';\nexport * from 'star-lib';\nexport type { T } from 'types-lib';\nexport async function run() {\n  const lazy = await import('lazy-lib');\n  return lazy;\n}\n",
        ),
        ("src/cjs.js", "require('cjs-lib');\n"),
        (
            "src/esm.mjs",
            "import { createRequire } from 'node:module';\nconst load = createRequire(import.meta.url);\nexport const made = load('made-lib');\n",
        ),
    ]);
    let root = ingested(&temp);
    let row =
        |to: &str, line: u64, kind: &str| (to.to_owned(), line, "external".to_owned(), json!(kind));
    assert_eq!(
        package_witnesses(root, "src/main.ts"),
        [
            row("pkg:lazy-lib", 5, "dynamic-import"),
            row("pkg:re-lib", 1, "named-reexport"),
            row("pkg:star-lib", 2, "star-reexport"),
            row("pkg:types-lib", 3, "type-named-reexport"),
        ]
    );
    assert_eq!(
        package_witnesses(root, "src/cjs.js"),
        [row("pkg:cjs-lib", 1, "commonjs-require")]
    );
    let made = package_witnesses(root, "src/esm.mjs");
    assert!(
        made.contains(&row("pkg:made-lib", 3, "create-require")),
        "{made:?}"
    );
    // A traversal reaches the package loaded only lazily.
    let deps = ask(root, "deps", Some("src/main.ts"));
    assert!(files(&deps).is_empty(), "{deps}");
    assert!(result_ids(&deps).contains(&"pkg:lazy-lib"), "{deps}");
}

/// An undeclared package is an undeclared dependency whatever form loads
/// it, a dynamic `import()` or a re-export included; a type-only re-export
/// counts exactly as a type-only import does.
#[test]
fn a_lazily_loaded_or_re_exported_undeclared_package_is_an_undeclared_dependency() {
    let temp = fixture(&[
        ("package.json", r#"{"name":"app","dependencies":{}}"#),
        (
            "src/main.ts",
            "import type { H } from 'ghost-type-import';\nexport type { G } from 'ghost-type-reexport';\nexport * from 'ghost-star';\nexport async function run(h: H) {\n  return [h, await import('ghost-lazy')];\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let issues = ask_with(root, "issues", None, |o| {
        o.detectors = vec!["undeclared-dependency".into()];
        o.min_tier = Some(0);
    });
    let flagged = crate::ast_graph::test_support::issue_rows(&issues)
        .iter()
        .filter_map(|row| row["evidence"]["package"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    for package in ["ghost-lazy", "ghost-star"] {
        assert!(flagged.contains(package), "{package}: {issues}");
    }
    assert_eq!(
        flagged.contains("ghost-type-reexport"),
        flagged.contains("ghost-type-import"),
        "{issues}"
    );
}

/// Every import witness and every import diagnostic states when the import
/// takes effect (`load`), read from the engine's one kind table: a value
/// import loads eagerly, `import type` never, `import()` later, whether the
/// target is a file, a package or outside the snapshot.
#[test]
fn import_witnesses_and_diagnostics_state_their_load() {
    let temp = fixture(&[
        (".git/HEAD", "ref: refs/heads/main\n"),
        (
            "package.json",
            r#"{"name":"app","dependencies":{"big-lib":"1"}}"#,
        ),
        ("outside/o.ts", "export type O = 1;\n"),
        ("app/b.ts", "export type B = 1;\nexport const v = 1;\n"),
        (
            "app/a.ts",
            "import type { B } from './b';\nimport { v } from './b';\nimport type { O } from '../outside/o';\nexport async function run(b: B, o: O) {\n  return [b, o, v, await import('big-lib')];\n}\n",
        ),
    ]);
    let root = temp.0.as_path();
    let out = ingest(root, &root.join("app"));
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    let witnesses = ask_with(root, "witnesses", Some("app/a.ts"), |o| {
        o.path = Some("app".into());
        o.edges = vec!["imports".into()];
    });
    let mut loads = witnesses["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (
                row["line"].as_u64().unwrap_or_default(),
                text("to"),
                text("load"),
            )
        })
        .collect::<Vec<_>>();
    loads.sort();
    let row = |line: u64, to: &str, load: &str| (line, to.to_owned(), load.to_owned());
    assert_eq!(
        loads,
        [
            row(1, "app/b.ts", "type"),
            row(2, "app/b.ts", "eager"),
            row(5, "pkg:big-lib", "deferred"),
        ],
        "{witnesses}"
    );
    let diagnostics = ask_with(root, "diagnostics", Some("outside-root"), |o| {
        o.path = Some("app".into());
    });
    let rows = diagnostics["rows"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "{diagnostics}");
    assert_eq!(rows[0]["importKind"], "type-import", "{diagnostics}");
    assert_eq!(rows[0]["load"], "type", "{diagnostics}");
}

/// `(target, via, edgeLine)` of every file `file` imports.
fn file_deps(root: &Path, file: &str) -> Vec<(String, String, u64)> {
    let deps = ask(root, "deps", Some(file));
    let mut rows = deps["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["kind"] == "file")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (
                text("id"),
                text("via"),
                row["edgeLine"].as_u64().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

/// A C# `using N;` of a namespace the repository declares depends on the
/// files declaring the types of `N` the file uses (a call, a receiver type,
/// a base type), citing the `using` line, as a Java file depends on a class
/// of its package; a type of `N` the file never names adds no edge, nor does
/// a namespace outside the repository.
#[test]
fn a_csharp_using_of_a_repo_namespace_depends_on_the_types_it_uses() {
    let temp = fixture(&[
        (
            "src/Serialization/JsonContract.cs",
            "namespace Newtonsoft.Json.Serialization\n{\n    public class JsonContract { public static JsonContract Create() { return null; } }\n}\n",
        ),
        (
            "src/Serialization/Unused.cs",
            "namespace Newtonsoft.Json.Serialization\n{\n    public class Unused { }\n}\n",
        ),
        (
            "src/Linq/JToken.cs",
            "namespace Newtonsoft.Json.Linq;\npublic class JToken { }\n",
        ),
        (
            "src/JsonConvert.cs",
            "using System;\nusing Newtonsoft.Json.Serialization;\nusing Newtonsoft.Json.Linq;\nnamespace Newtonsoft.Json\n{\n    public class JsonConvert : JToken\n    {\n        public void Run() { JsonContract.Create(); }\n    }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        file_deps(root, "src/JsonConvert.cs"),
        [
            (
                "src/Linq/JToken.cs".to_owned(),
                "namespace-use".to_owned(),
                3
            ),
            (
                "src/Serialization/JsonContract.cs".to_owned(),
                "namespace-use".to_owned(),
                2
            ),
        ]
    );
}

/// A Scala wildcard import of a repository package (`import cats.data._`)
/// depends on the files of that package declaring what the file uses,
/// citing the import line; the package is the declaring file's directory, as
/// Scala call linking reads it.
#[test]
fn a_scala_wildcard_import_of_a_repo_package_depends_on_what_it_uses() {
    let temp = fixture(&[
        (
            "core/src/main/scala/cats/data/Chain.scala",
            "package cats\npackage data\n\nobject Chain { def one(): Int = 1 }\n",
        ),
        (
            "core/src/main/scala/cats/data/Other.scala",
            "package cats.data\n\nclass Other\n",
        ),
        (
            "core/src/main/scala/cats/Use.scala",
            "package cats\n\nimport cats.data._\n\nobject Use { def run(): Int = Chain.one() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        file_deps(root, "core/src/main/scala/cats/Use.scala"),
        [(
            "core/src/main/scala/cats/data/Chain.scala".to_owned(),
            "namespace-use".to_owned(),
            3
        )]
    );
}
