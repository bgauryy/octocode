//! The graph's `test` role is the shared test-code rule
//! ([`crate::tools::test_code::TestCodeIndex`]), so `graphQuery` and a
//! `testCode` filter agree on every file. Examples, benchmarks, stories and
//! test setup files are not tests by that rule: the graph names them
//! `support`, which detectors still treat as non-production.
use super::*;
use crate::tools::test_code::TestCodeIndex;

const FILES: &[(&str, &str)] = &[
    ("src/lib.ts", "export function run() { return 1; }\n"),
    ("src/lib.test.ts", "import { run } from './lib';\nrun();\n"),
    (
        "examples/demo.ts",
        "import { run } from '../src/lib';\nrun();\n",
    ),
    (
        "benches/speed.ts",
        "import { run } from '../src/lib';\nrun();\n",
    ),
    (
        "testing/helper.ts",
        "import { run } from '../src/lib';\nexport const h = () => run();\n",
    ),
    (
        "src/button.stories.ts",
        "import { run } from './lib';\nrun();\n",
    ),
    (
        "src/setupTests.ts",
        "import { run } from './lib';\nrun();\n",
    ),
    (
        "tests/e2e.ts",
        "import { run } from '../src/lib';\nrun();\n",
    ),
];

fn roles(root: &Path, file: &str) -> Vec<String> {
    let node = ask(root, "node", Some(file), |_| {});
    assert_eq!(
        node.value["node"]["id"],
        format!("app/{file}"),
        "{}",
        node.value
    );
    node.value["node"]["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|role| role.as_str().map(str::to_owned))
        .collect()
}

#[test]
fn graph_test_role_agrees_with_test_code_on_every_file() {
    let dir = git_app(FILES);
    let root = dir.0.as_path();
    let built = ingest_fixture(root, None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let test_code = TestCodeIndex::new(&workspace_policy(root));
    let mut support = Vec::new();
    for (file, _) in FILES {
        let roles = roles(root, file);
        let graph_test = roles.iter().any(|role| role == "test");
        let shared_test = test_code.test_file(&root.join("app").join(file));
        assert_eq!(graph_test, shared_test, "{file}: {roles:?}");
        if roles.iter().any(|role| role == "support") {
            support.push(*file);
        }
    }
    assert_eq!(
        support,
        [
            "examples/demo.ts",
            "benches/speed.ts",
            "testing/helper.ts",
            "src/button.stories.ts",
            "src/setupTests.ts",
        ]
    );
}

/// A production file only a support entry (an example that is a Cargo
/// target, a bench) reaches is labelled as one only tests reach, not
/// reachable.
#[test]
fn a_file_only_support_entries_reach_is_not_production_reachable() {
    let dir = git_app(&[
        ("src/main.ts", "import { run } from './lib';\nrun();\n"),
        ("src/lib.ts", "export function run() { return 1; }\n"),
        ("src/extra.ts", "export function extra() { return 2; }\n"),
        (
            "examples/demo.ts",
            "import { extra } from '../src/extra';\nextra();\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.min_tier = Some(0);
        o.entries = vec!["src/main.ts".into(), "examples/demo.ts".into()];
    });
    let detectors = |subject: &str| {
        issues.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["subject"] == format!("app/{subject}"))
            .map(|row| row["detector"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(detectors("src/extra.ts"), ["test-only"], "{}", issues.value);
    assert!(detectors("src/lib.ts").is_empty(), "{}", issues.value);
}

/// A support file stays out of production analysis as a test file does: an
/// export only it uses is no orphan, and it is no orphan itself.
#[test]
fn support_files_stay_out_of_production_findings() {
    let dir = git_app(FILES);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| o.min_tier = Some(0));
    for (file, _) in &FILES[2..7] {
        let subjects = issues.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["subject"].as_str().is_some_and(|s| s.starts_with(file)))
            .map(|row| row["detector"].clone())
            .collect::<Vec<_>>();
        assert!(subjects.is_empty(), "{file}: {subjects:?}");
    }
}

/// A Rust module a production root mounts without `#[cfg(test)]` is
/// production whatever it is called (`tests.rs`, `foo_test.rs`, a `spec/`
/// directory), and so is what it mounts; the graph asks the same mounts as
/// the `testCode` filter. An unmounted `*_test.rs` stays test by spelling.
#[test]
fn production_mounted_modules_named_like_tests_are_production_in_the_graph() {
    let files: &[(&str, &str)] = &[
        CARGO_APP_FILE,
        (
            "src/lib.rs",
            "pub mod tests;\npub mod foo_test;\npub mod spec;\n",
        ),
        ("src/tests.rs", "pub mod helper;\npub fn t() {}\n"),
        ("src/tests/helper.rs", "pub fn h() {}\n"),
        ("src/foo_test.rs", "pub fn f() {}\n"),
        ("src/spec/mod.rs", "pub mod inner;\npub fn s() {}\n"),
        ("src/spec/inner.rs", "pub fn i() {}\n"),
        ("src/orphan_test.rs", "pub fn o() {}\n"),
    ];
    let dir = git_app(files);
    let root = dir.0.as_path();
    let built = ingest_fixture(root, None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let test_code = TestCodeIndex::new(&workspace_policy(root));
    let mut mismatches = Vec::new();
    for (file, _) in files.iter().filter(|(file, _)| file.ends_with(".rs")) {
        let graph_test = roles(root, file).iter().any(|role| role == "test");
        let shared_test = test_code.test_file(&root.join("app").join(file));
        if graph_test != shared_test {
            mismatches.push(format!(
                "{file}: graph test={graph_test}, testCode={shared_test}"
            ));
        }
        if graph_test != (*file == "src/orphan_test.rs") {
            mismatches.push(format!("{file}: graph test={graph_test} is wrong"));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

const CARGO_APP_FILE: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
);
