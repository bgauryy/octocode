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
    assert!(files.contains(&("src/lib.ts".into(), 0)), "{files:?}");
    assert!(
        files.iter().any(|(f, _)| f == "src/main.ts"),
        "greet calls shout, main calls greet: {files:?}"
    );
    assert!(
        !files.iter().any(|(f, _)| f == "src/fmt.ts"),
        "fmt never touches lib: {files:?}"
    );
    assert_eq!(
        symbol.value["summary"]["testsToRun"],
        json!(["src/main.test.ts"])
    );

    // Box is exported but nobody binds it: symbol precision keeps main.ts out.
    let unused = ask(root, "impact", Some("src/lib.ts#Box"), |_| {});
    assert_eq!(
        impacted(&unused),
        vec![("src/lib.ts".to_owned(), 0)],
        "{}",
        unused.value
    );

    let file = ask(root, "impact", Some("src/lib.ts"), |_| {});
    assert_eq!(file.value["summary"]["precision"], "file");
    let files = impacted(&file);
    assert!(files.contains(&("src/main.ts".into(), 1)), "{files:?}");
    assert!(files.contains(&("src/main.test.ts".into(), 2)), "{files:?}");
    assert_eq!(
        file.value["summary"]["affectedEntrypoints"],
        json!(["src/main.ts"])
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
    assert_eq!(out.value["summary"]["testsToRun"], json!(["src/lib.rs"]));
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
    assert!(files.iter().any(|(f, _)| f == "src/useA.ts"), "{files:?}");
    assert!(
        !files.iter().any(|(f, _)| f == "src/useB.ts"),
        "barrel passes through: {files:?}"
    );
    assert_eq!(
        out.value["summary"]["maxDepth"], 3,
        "{}",
        out.value["summary"]
    );
    assert!(
        !files.iter().any(|(f, _)| f == "src/c4.ts"),
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
    assert!(impacted(&deep).iter().any(|(f, _)| f == "src/c4.ts"));
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
        vec!["src/index.ts", "src/main.ts"],
        "{}",
        out.value
    );
    let through = &out.value["results"][1];
    assert_eq!(through["edge"], "uses", "{through}");
    assert_eq!(through["via"], "reexport", "{through}");
    assert_eq!(through["depth"], 1, "{through}");
    assert_eq!(through["edgeLine"], 1, "{through}");
    // An explicit edge filter walks exactly the edges it names.
    let imports = ask(root, "dependents", Some("src/a.ts"), |o| {
        o.edges = vec!["imports".into()];
    });
    assert_eq!(ids(&imports), vec!["src/index.ts"], "{}", imports.value);
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
    assert_eq!(first.value["truncated"], true, "{}", first.value);
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
    assert_eq!(second.value["results"].as_array().map(Vec::len), Some(1));
}
