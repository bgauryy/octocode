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
    assert_eq!(
        symbol.value["summary"]["testsToRun"],
        json!(["app/src/main.test.ts"])
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
    assert_eq!(
        file.value["summary"]["affectedEntrypoints"],
        json!(["app/src/main.ts"])
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
    let tests = &out.value["summary"]["testFunctions"];
    assert!(
        tests
            .as_array()
            .is_some_and(|t| t.contains(&json!("src/lib.rs#tests.adds"))),
        "{}",
        out.value["summary"]
    );
    assert!(
        tests
            .as_array()
            .is_some_and(|t| t.contains(&json!("src/lib.rs#tests.doubles"))),
        "transitively via twice: {}",
        out.value["summary"]
    );
    assert_eq!(
        out.value["summary"]["testsToRun"],
        json!(["app/src/lib.rs"])
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
            out.value["summary"]["affectedEntrypoints"],
            json!(["app/src/lib.rs"]),
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
    assert_eq!(
        deep.value["summary"]["testsToRun"],
        json!(["app/tests/deep.test.ts"]),
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
        let out = ask(root, "impact", Some(target), |_| {});
        let tests = out.value["summary"]["testsToRun"]
            .as_array()
            .expect("testsToRun")
            .iter()
            .map(|test| test.as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
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
