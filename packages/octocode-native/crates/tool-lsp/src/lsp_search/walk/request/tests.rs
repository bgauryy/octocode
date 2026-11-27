use super::production_first;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::path::Path;

fn ordered(items: &mut [serde_json::Value], root: &str) {
    let paths = octocode_runtime_base::test_support::workspace_policy(Path::new(root));
    production_first(items, &paths);
}

/// Callers in test files follow the production callers of their level;
/// every edge stays, in walk order within each group.
#[test]
fn production_callers_lead_each_level() {
    let edge = |path: &str, level: u64| json!({"from": {"path": path}, "level": level});
    let mut items = vec![
        edge("/r/src/a.ts", 1),
        edge("/r/tests/a.test.ts", 1),
        edge("/r/src/b.spec.ts", 1),
        edge("/r/src/c.ts", 1),
        json_fixture(
            r#"{
                    "to": {"path": "/r/src/d.ts"},
                    "callerPath": "/r/src/e.ts",
                    "level": 1
                }"#,
        ),
        edge("/r/src/f.ts", 2),
    ];
    ordered(&mut items, "/r");
    let order = items
        .iter()
        .map(|item| {
            item.pointer("/callerPath")
                .or_else(|| item.pointer("/from/path"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "/r/src/a.ts",
            "/r/src/c.ts",
            "/r/src/e.ts",
            "/r/tests/a.test.ts",
            "/r/src/b.spec.ts",
            "/r/src/f.ts"
        ]
    );
    // A root that is itself under a test directory does not mark all.
    let mut nested = vec![
        edge("/t/tests/repo/b.test.ts", 1),
        edge("/t/tests/repo/a.ts", 1),
    ];
    ordered(&mut nested, "/t/tests/repo");
    assert_eq!(nested[0]["from"]["path"], "/t/tests/repo/a.ts");
}

/// The one test-code rule decides: a Rust file only a `#[cfg(test)] mod`
/// mounts, and a caller declared inside a `#[cfg(test)]` item, are test
/// code though no path says so.
#[test]
fn production_callers_lead_by_the_shared_test_code_rule() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("root");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(
            root.join("src/lib.rs"),
            "#[cfg(test)]\nmod helpers;\nmod prod;\npub fn a() {}\n#[cfg(test)]\nmod t {\n    fn caller() {}\n}\n",
        )
        .expect("lib");
    std::fs::write(root.join("src/helpers.rs"), "fn h() { crate::a(); }\n").expect("helpers");
    std::fs::write(root.join("src/prod.rs"), "fn p() { crate::a(); }\n").expect("prod");
    let at = |file: &str, line: u64| {
        json!({"from": {"path": root.join(file).to_string_lossy(),
                "line": line}, "level": 1})
    };
    let mut items = vec![
        at("src/helpers.rs", 1),
        at("src/lib.rs", 7),
        at("src/prod.rs", 1),
    ];
    ordered(&mut items, &root.to_string_lossy());
    let files = items
        .iter()
        .map(|item| {
            item["from"]["path"]
                .as_str()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(files, ["prod.rs", "helpers.rs", "lib.rs"]);
}
