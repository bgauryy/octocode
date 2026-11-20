//! Impact: symbol precision, test functions, barrels and dependents.
use super::*;

#[test]
fn impact_is_symbol_precise_for_symbols_and_transitive_for_files() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();

    let symbol = ask(root, "impact", Some("src/lib.ts#shout"), |_| {});
    assert_eq!(
        symbol.value["summary"]["precision"], "symbol",
        "{}",
        symbol.value
    );
    let files = impacted(&symbol);
    assert!(files.contains(&("app/src/lib.ts".into(), 0)), "{files:?}");
    assert!(
        files.iter().any(|(f, _)| f == "app/src/main.ts"),
        "greet calls shout, main calls greet: {files:?}"
    );
    assert!(
        !files.iter().any(|(f, _)| f == "app/src/fmt.ts"),
        "fmt never touches lib: {files:?}"
    );
    assert_eq!(symbol.value["summary"]["testsToRun"], 1);
    assert_eq!(
        tests_to_run(root, "src/lib.ts#shout", |_| {}),
        ["app/src/main.test.ts"]
    );

    // Box is exported but nobody binds it: symbol precision keeps main.ts out.
    let unused = ask(root, "impact", Some("src/lib.ts#Box"), |_| {});
    assert_eq!(
        impacted(&unused),
        vec![("app/src/lib.ts".to_owned(), 0)],
        "{}",
        unused.value
    );

    let file = ask(root, "impact", Some("src/lib.ts"), |_| {});
    assert_eq!(file.value["summary"]["precision"], "file");
    let files = impacted(&file);
    assert!(files.contains(&("app/src/main.ts".into(), 1)), "{files:?}");
    assert!(
        files.contains(&("app/src/main.test.ts".into(), 2)),
        "{files:?}"
    );
    assert_eq!(file.value["summary"]["affectedEntrypoints"], 1);
    assert_eq!(
        summary_list(root, Some("src/lib.ts"), "affectedEntrypoints", |_| {}),
        [json!("app/src/main.ts")]
    );
}

#[test]
fn impact_lists_inline_rust_test_functions() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    let krate = dir.0.as_path().join("app");
    std::fs::create_dir_all(krate.join("src")).expect("src");
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");
    std::fs::write(
        krate.join("src/lib.rs"),
        "pub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n\npub fn twice(a: u8) -> u8 {\n    add(a, a)\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn adds() {\n        let _ = add(1, 2);\n    }\n\n    #[test]\n    fn doubles() {\n        let _ = twice(2);\n    }\n}\n",
    )
    .expect("lib");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let node = ask(
        dir.0.as_path(),
        "node",
        Some("src/lib.rs#tests.adds"),
        |_| {},
    );
    assert_eq!(node.value["node"]["test"], true, "{}", node.value);
    let out = ask(dir.0.as_path(), "impact", Some("src/lib.rs#add"), |_| {});
    assert_eq!(out.value["summary"]["testFunctions"], 2, "{}", out.value);
    let tests = summary_list(
        dir.0.as_path(),
        Some("src/lib.rs#add"),
        "testsToRun",
        |_| {},
    );
    assert_eq!(tests.len(), 1, "{tests:?}");
    assert_eq!(tests[0]["path"], "app/src/lib.rs", "{tests:?}");
    let functions = tests[0]["functions"].as_array().expect("functions");
    assert!(functions.contains(&json!("tests.adds")), "{tests:?}");
    assert!(
        functions.contains(&json!("tests.doubles")),
        "transitively via twice: {tests:?}"
    );
}

#[test]
fn plan_5_impact_defaults_to_depth_three_and_passes_through_barrels() {
    let dir = git_app(&[
        ("src/a.ts", "export const a = () => 1;\n"),
        ("src/b.ts", "export const b = () => 2;\n"),
        (
            "src/index.ts",
            "export { a } from './a';\nexport { b } from './b';\n",
        ),
        (
            "src/useA.ts",
            "import { a } from './index';\nexport const ua = () => a();\n",
        ),
        (
            "src/useB.ts",
            "import { b } from './index';\nexport const ub = () => b();\n",
        ),
        (
            "src/c1.ts",
            "import { ua } from './useA';\nexport const c1 = () => ua();\n",
        ),
        (
            "src/c2.ts",
            "import { c1 } from './c1';\nexport const c2 = () => c1();\n",
        ),
        (
            "src/c3.ts",
            "import { c2 } from './c2';\nexport const c3 = () => c2();\n",
        ),
        (
            "src/c4.ts",
            "import { c3 } from './c3';\nexport const c4 = () => c3();\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "impact", Some("src/a.ts"), |_| {});
    let files = impacted(&out);
    assert!(
        files.iter().any(|(f, _)| f == "app/src/useA.ts"),
        "{files:?}"
    );
    assert!(
        !files.iter().any(|(f, _)| f == "app/src/useB.ts"),
        "barrel passes through: {files:?}"
    );
    assert_eq!(
        out.value["summary"]["maxDepth"], 3,
        "{}",
        out.value["summary"]
    );
    assert!(
        !files.iter().any(|(f, _)| f == "app/src/c4.ts"),
        "depth 3 by default: {files:?}"
    );
    assert!(
        out.value["summary"]["byDepth"].is_object(),
        "{}",
        out.value["summary"]
    );
    let deep = ask(dir.0.as_path(), "impact", Some("src/a.ts"), |o| {
        o.depth = Some(10);
    });
    assert!(impacted(&deep).iter().any(|(f, _)| f == "app/src/c4.ts"));
}

// ── Drift, explicit entries, re-export dependents, scan cuts, parsers ───────

/// A file's dependents include the files that use its exports through a
/// re-exporting module, and only the ones naming one of those exports: an
/// importer of another item of the barrel does not depend on it.
#[test]
fn dependents_include_importers_through_a_reexporting_module() {
    let dir = git_app(&[
        (
            "src/a.ts",
            "export const a = 1;\nexport const unused = 2;\n",
        ),
        ("src/b.ts", "export const b = 2;\n"),
        (
            "src/index.ts",
            "export { a } from './a';\nexport { b } from './b';\n",
        ),
        (
            "src/main.ts",
            "import { a } from './index';\nconsole.log(a);\n",
        ),
        (
            "src/other.ts",
            "import { b } from './index';\nconsole.log(b);\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(root, "dependents", Some("src/a.ts"), |_| {});
    assert_eq!(
        ids(&out),
        vec!["app/src/index.ts", "app/src/main.ts"],
        "{}",
        out.value
    );
    let through = &out.value["rows"][1];
    assert_eq!(through["edge"], "uses", "{through}");
    assert_eq!(through["via"], "reexport", "{through}");
    assert_eq!(through["depth"], 1, "{through}");
    assert_eq!(through["edgeLine"], 1, "{through}");
    // An explicit edge filter walks exactly the edges it names.
    let imports = ask(root, "dependents", Some("src/a.ts"), |o| {
        o.edges = vec!["imports".into()];
    });
    assert_eq!(ids(&imports), vec!["app/src/index.ts"], "{}", imports.value);
}

/// `impact` lists each affected file once, as a row with its depth and
/// risk; the summary (counts, entrypoints, tests) goes out with the first
/// page only.
#[test]
fn impact_summary_is_sent_once_and_does_not_repeat_the_rows() {
    let dir = fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let first = ask(root, "impact", Some("src/lib.ts"), |o| o.limit = Some(1));
    assert_eq!(
        first.value["pagination"]["hasMore"], true,
        "{}",
        first.value
    );
    let summary = &first.value["summary"];
    assert!(summary["byDepth"].is_object(), "{summary}");
    for repeated in [
        "willBreak",
        "likely",
        "shouldTest",
        "testCount",
        "testFunctionCount",
    ] {
        assert!(summary.get(repeated).is_none(), "{repeated}: {summary}");
    }
    let second = ask(root, "impact", Some("src/lib.ts"), |o| {
        o.limit = Some(1);
        o.offset = Some(1);
    });
    assert_eq!(second.class, ExitClass::Success, "{}", second.value);
    assert!(second.value.get("summary").is_none(), "{}", second.value);
    assert_eq!(second.value["rows"].as_array().map(Vec::len), Some(1));
}

/// Symbol precision lists only what binds the changed name: an importer
/// whose linked references into the module name other items is skipped, and
/// one with no linked name at all (a glob or bare module import) is a
/// module-level `candidate`, never a `direct` breakage, and is not expanded
/// as if its whole module changed.
#[test]
fn symbol_impact_keeps_import_only_rows_as_unexpanded_candidates() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod paging;\npub mod names_other;\npub mod binds;\npub mod glob;\npub mod above;\n",
        ),
        (
            "src/paging.rs",
            "pub trait RowFields {\n    fn fields(&self) -> usize;\n}\n\npub fn push_warnings() -> usize {\n    1\n}\n",
        ),
        (
            "src/names_other.rs",
            "use crate::paging;\n\npub fn other() -> usize {\n    paging::push_warnings()\n}\n",
        ),
        (
            "src/binds.rs",
            "use crate::paging::RowFields;\n\npub fn count(row: &dyn RowFields) -> usize {\n    row.fields()\n}\n",
        ),
        (
            "src/glob.rs",
            "#[allow(unused_imports)]\nuse crate::paging::*;\n",
        ),
        (
            "src/above.rs",
            "use crate::glob;\nuse crate::names_other;\n\npub fn up() -> usize {\n    names_other::other()\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "impact",
        Some("src/paging.rs#RowFields"),
        |_| {},
    );
    assert_eq!(out.value["summary"]["precision"], "symbol", "{}", out.value);
    let rows = out.value["rows"].as_array().expect("rows");
    let risk = |file: &str| {
        rows.iter()
            .find(|row| row["id"] == format!("app/{file}"))
            .map(|row| row["risk"].as_str().unwrap_or_default().to_owned())
    };
    assert_eq!(
        risk("src/binds.rs").as_deref(),
        Some("direct"),
        "{}",
        out.value
    );
    assert_eq!(
        risk("src/names_other.rs"),
        None,
        "names only push_warnings: {}",
        out.value
    );
    for row in rows.iter().filter(|row| {
        row["via"]
            .as_str()
            .is_some_and(|via| via.starts_with("imports"))
    }) {
        assert_eq!(row["risk"], "candidate", "import-only row: {}", out.value);
    }
    assert_eq!(
        risk("src/above.rs"),
        None,
        "nothing above a candidate or a skipped importer: {}",
        out.value
    );
}

/// A Rust module is declared by its parent (`mod x;`, a `contains` edge of
/// detail `module`): a change in it reaches the crate root through the
/// module chain, so a library's root is an affected entrypoint. The parent
/// is listed as an unexpanded candidate and walks on only up its own module
/// chain: a file that uses another item of the root stays out.
#[test]
fn a_rust_module_change_reaches_its_crate_root_through_the_module_tree() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod num;\npub mod nested;\npub mod other;\n\npub fn helper() -> u8 {\n    1\n}\n",
        ),
        (
            "src/num.rs",
            "pub fn saturating(v: u64) -> u32 {\n    v as u32\n}\n",
        ),
        ("src/nested.rs", "pub mod deep;\n"),
        ("src/nested/deep.rs", "pub fn d() -> u8 {\n    2\n}\n"),
        (
            "src/other.rs",
            "use crate::helper;\n\npub fn o() -> u8 {\n    helper()\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    for (target, depth) in [
        ("src/num.rs", 1),
        ("src/num.rs#saturating", 1),
        ("src/nested/deep.rs", 2),
    ] {
        let out = ask(root, "impact", Some(target), |o| o.depth = Some(depth));
        assert_eq!(
            summary_list(root, Some(target), "affectedEntrypoints", |o| {
                o.depth = Some(depth);
            }),
            [json!("app/src/lib.rs")],
            "{target}: {}",
            out.value
        );
        let rows = out.value["rows"].as_array().expect("rows");
        let lib = rows
            .iter()
            .find(|row| row["id"] == "app/src/lib.rs")
            .unwrap_or_else(|| panic!("{target}: {}", out.value));
        assert_eq!(lib["via"], "contains:module", "{target}: {lib}");
        assert_eq!(lib["risk"], "candidate", "{target}: {lib}");
        assert!(
            !rows.iter().any(|row| row["id"] == "app/src/other.rs"),
            "other.rs uses only helper: {target}: {}",
            out.value
        );
    }
}

/// A depth bound only cuts the rows: every file it lists has the depth,
/// risk and reaching edge the whole walk gives it. A caller through a path
/// call (`crate::num::saturating(..)`, no `use`) is a direct row at depth
/// 1 under every bound, reached by its call, not by its function's
/// membership in the file.
#[test]
fn impact_rows_within_a_bound_do_not_depend_on_the_bound() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod num;\npub mod path_call;\npub mod used;\npub mod top;\n",
        ),
        (
            "src/num.rs",
            "pub fn saturating(v: u64) -> u32 {\n    v as u32\n}\n",
        ),
        (
            "src/path_call.rs",
            "pub fn p() -> u32 {\n    crate::num::saturating(3)\n}\n",
        ),
        (
            "src/used.rs",
            "use crate::num::saturating;\n\npub fn u() -> u32 {\n    saturating(4)\n}\n",
        ),
        (
            "src/top.rs",
            "pub fn t() -> u32 {\n    crate::path_call::p() + crate::used::u()\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let rows_at = |depth: u32| {
        let out = ask(root, "impact", Some("src/num.rs#saturating"), |o| {
            o.depth = Some(depth);
            o.limit = Some(500);
        });
        let rows = out.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| json!([row["id"], row["depth"], row["risk"], row["via"]]))
            .collect::<Vec<_>>();
        (out.value, rows)
    };
    let (deepest, whole) = rows_at(10);
    let caller = whole
        .iter()
        .find(|row| row[0] == "app/src/path_call.rs")
        .unwrap_or_else(|| panic!("path-call caller: {deepest}"));
    assert_eq!(caller[1], 1, "{caller}");
    assert_eq!(caller[2], "direct", "{caller}");
    assert!(
        caller[3]
            .as_str()
            .is_some_and(|via| via.starts_with("calls")),
        "{caller}"
    );
    assert!(
        whole
            .iter()
            .any(|row| row[0] == "app/src/top.rs" && row[1] == 2),
        "{deepest}"
    );
    for depth in 1..=3 {
        let (value, rows) = rows_at(depth);
        let within = whole
            .iter()
            .filter(|row| row[1].as_u64().is_some_and(|d| d <= u64::from(depth)))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(rows, within, "depth {depth}: {value}");
        assert_eq!(
            value["summary"]["byDepth"]["1"], deepest["summary"]["byDepth"]["1"],
            "depth {depth}: {value}"
        );
    }
}

/// `testsToRun` lists every test the change reaches, however far: the
/// default depth bounds the listed rows, never the tests. A test five hops
/// from the changed symbol is named, while its row stays past `maxDepth`.
#[test]
fn tests_to_run_reach_past_the_depth_bound() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/c3.ts"}"#),
        (
            "src/store.ts",
            "export function S() { return 1; }\nexport function T() { return 2; }\n",
        ),
        (
            "src/a.ts",
            "import { S } from './store';\nexport function a() { return S(); }\n",
        ),
        (
            "src/c.ts",
            "import { a } from './a';\nexport function c() { return a(); }\n",
        ),
        (
            "src/c2.ts",
            "import { c } from './c';\nexport function c2() { return c(); }\n",
        ),
        (
            "src/c3.ts",
            "import { c2 } from './c2';\nexport function c3() { return c2(); }\n",
        ),
        (
            "tests/deep.test.ts",
            "import { c3 } from '../src/c3';\nc3();\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let deep = ask(root, "impact", Some("src/store.ts#S"), |o| {
        o.depth = Some(10);
    });
    let deep_tests = tests_to_run(root, "src/store.ts#S", |o| o.depth = Some(10));
    assert_eq!(
        deep_tests,
        ["app/tests/deep.test.ts"],
        "the deep walk reaches the test: {}",
        deep.value
    );
    let bounded = ask(root, "impact", Some("src/store.ts#S"), |_| {});
    let summary = &bounded.value["summary"];
    assert_eq!(summary["depthLimited"], true, "{summary}");
    assert!(
        !impacted(&bounded)
            .iter()
            .any(|(file, _)| file == "app/tests/deep.test.ts"),
        "the test's row lies past maxDepth: {}",
        bounded.value
    );
    assert_eq!(
        summary["testsToRun"], deep.value["summary"]["testsToRun"],
        "{summary}"
    );
    assert_eq!(tests_to_run(root, "src/store.ts#S", |_| {}), deep_tests);
}

/// A Go importer binds its package, never one file of it: an importer whose
/// linked references name only other items of the changed type's package
/// is no candidate, as a TypeScript importer naming other exports is not.
/// Proven rows (an implementer's file, a method that takes the type) come
/// before any import-only candidate, whatever their depth.
#[test]
fn impact_on_a_go_interface_ranks_proven_rows_before_import_only_candidates() {
    let dir = git_app(&[
        ("go.mod", "module example.com/app\n"),
        (
            "storage/interface.go",
            "package storage\n\ntype Appender interface {\n\tAppend(v int) error\n}\n",
        ),
        (
            "storage/query.go",
            "package storage\n\ntype Queryable interface {\n\tQuery() int\n}\n",
        ),
        (
            "impl/impl.go",
            "package impl\n\nimport \"example.com/app/storage\"\n\ntype mine struct{}\n\nfunc (m mine) Append(v int) error { return nil }\n\nfunc Wrap(a storage.Appender) error { return a.Append(1) }\n",
        ),
        (
            "other/user.go",
            "package other\n\nimport \"example.com/app/storage\"\n\nfunc Q(q storage.Queryable) int { return q.Query() }\n",
        ),
        (
            "bare/bare.go",
            "package bare\n\nimport _ \"example.com/app/storage\"\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let out = ask(
        root,
        "impact",
        Some("storage/interface.go#Appender"),
        |_| {},
    );
    let rows = out.value["rows"].as_array().expect("rows");
    let risk = |file: &str| {
        rows.iter()
            .find(|row| row["id"] == format!("app/{file}"))
            .map(|row| row["risk"].as_str().unwrap_or_default().to_owned())
    };
    assert_eq!(
        risk("impl/impl.go").as_deref(),
        Some("direct"),
        "{}",
        out.value
    );
    assert_eq!(
        risk("other/user.go"),
        None,
        "names only Queryable of the package: {}",
        out.value
    );
    assert_eq!(
        risk("bare/bare.go").as_deref(),
        Some("candidate"),
        "{}",
        out.value
    );
    let first_candidate = rows.iter().position(|row| row["risk"] == "candidate");
    let last_proven = rows.iter().rposition(|row| row["risk"] != "candidate");
    if let (Some(candidate), Some(proven)) = (first_candidate, last_proven) {
        assert!(proven < candidate, "proven rows first: {}", out.value);
    }
}

/// `testsToRun` names each test file once and no helper: a file under a
/// test directory that test files import, and whose name marks no test, is
/// their support code, not a test to run.
#[test]
fn tests_to_run_lists_each_test_file_once_and_no_helper() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app"}"#),
        ("src/errors.ts", "export function fail() { return 1; }\n"),
        (
            "tests/helpers.ts",
            "import { fail } from '../src/errors';\nexport function boom() { return fail(); }\n",
        ),
        (
            "tests/a.test.ts",
            "import { boom } from './helpers';\nimport { fail } from '../src/errors';\nboom(); fail();\n",
        ),
        (
            "tests/b.test.ts",
            "import { boom } from './helpers';\nboom();\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    for target in ["src/errors.ts", "src/errors.ts#fail"] {
        let tests = tests_to_run(root, target, |_| {});
        let unique = tests.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), tests.len(), "{target}: {tests:?}");
        for test in &tests {
            assert!(
                octocode_engine::text::test_paths::is_test_path(test),
                "{target}: {test} is no test path"
            );
        }
        assert!(
            !tests.iter().any(|test| test.ends_with("tests/helpers.ts")),
            "{target}: a helper is no test to run: {tests:?}"
        );
        for test in ["app/tests/a.test.ts", "app/tests/b.test.ts"] {
            assert!(tests.iter().any(|t| t == test), "{target}: {tests:?}");
        }
    }
}

/// A depth bound that leaves affected files unlisted counts them in a
/// warning and leads to the depth that lists every one: the related call's
/// rows are the whole walk's, and that call is no longer depth limited. A
/// changed file is its depth-0 row, never repeated in summary.changed; a
/// changed symbol, which no row names, is listed there.
#[test]
fn a_depth_limited_impact_leads_to_the_depth_that_lists_every_file() {
    let chain = ["a", "b", "c", "d", "e", "f"];
    let files = chain
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let text = match at.checked_sub(1).map(|before| chain[before]) {
                None => format!("export function {name}() {{ return 1; }}\n"),
                Some(prev) => format!(
                    "import {{ {prev} }} from './{prev}';\nexport function {name}() {{ return {prev}(); }}\n"
                ),
            };
            (format!("src/{name}.ts"), text)
        })
        .collect::<Vec<_>>();
    let files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&files);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let bounded = ask(root, "impact", Some("src/a.ts"), |_| {});
    let summary = &bounded.value["summary"];
    assert_eq!(summary["depthLimited"], true, "{summary}");
    assert!(summary.get("changed").is_none(), "{summary}");
    assert_eq!(bounded.value["rows"][0]["id"], "app/src/a.ts");
    assert_eq!(bounded.value["rows"][0]["risk"], "changed");
    let listed = impacted(&bounded).len();
    let unlisted = chain.len() - listed;
    assert!(unlisted > 0, "{}", bounded.value);
    let warnings = bounded.value["warnings"].to_string();
    assert!(
        warnings.contains(&format!("{unlisted} affected files")) && warnings.contains("related"),
        "{}",
        bounded.value
    );
    let lead = bounded.value["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["input"]["queries"][0]["operation"] == "impact")
        .expect("depth lead");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["target"], "src/a.ts", "{lead}");
    let depth = row["depth"]
        .as_u64()
        .and_then(|depth| u32::try_from(depth).ok())
        .expect("depth");
    let whole = ask(root, "impact", Some("src/a.ts"), |o| {
        o.depth = Some(depth);
    });
    assert_eq!(impacted(&whole).len(), chain.len(), "{}", whole.value);
    assert_ne!(
        whole.value["summary"]["depthLimited"], true,
        "{}",
        whole.value
    );
    assert!(whole.value.get("warnings").is_none(), "{}", whole.value);

    let symbol = ask(root, "impact", Some("app/src/a.ts#a"), |o| {
        o.depth = Some(depth);
    });
    assert_eq!(
        symbol.value["summary"]["changed"],
        json!(["app/src/a.ts#a"]),
        "{}",
        symbol.value
    );
}
