//! `testCode` on `references` and `callers`: the local tools' test-code
//! rule decides which sites stay, and what it leaves out is counted.

use super::*;

type Sites = (
    crate::tools::test_support::Fixture,
    std::path::PathBuf,
    crate::policy::path::PathPolicy,
    Vec<serde_json::Value>,
);

/// A workspace with a production file, a test file by path, and a Rust
/// file whose `#[cfg(test)]` module calls `foo`; one site in each part.
fn sites() -> Sites {
    let (dir, root, paths) = temp_workspace();
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("src/a.ts"), "foo();\n").expect("a.ts");
    std::fs::write(root.join("src/a.test.ts"), "foo();\n").expect("a.test.ts");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn foo() {}\nfn bar() { foo(); }\n#[cfg(test)]\nmod tests {\n    fn t() { super::foo(); }\n}\n",
    )
    .expect("lib.rs");
    let at = |name: &str, line: u32| {
        let uri = octocode_engine::lsp::uri::path_to_uri(&root.join(name).to_string_lossy())
            .expect("uri");
        serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 3}}
        })
    };
    let found = vec![
        at("src/a.ts", 0),
        at("src/a.test.ts", 0),
        at("src/lib.rs", 1),
        at("src/lib.rs", 4),
    ];
    (dir, root, paths, found)
}

async fn reference_files(test_code: Option<&str>) -> (serde_json::Value, std::path::PathBuf) {
    let (_dir, root, paths, found) = sites();
    let uri = octocode_engine::lsp::uri::path_to_uri(&root.join("src/a.ts").to_string_lossy())
        .expect("uri");
    let mut input = serde_json::json!({
        "operation": "references", "path": uri, "symbolName": "foo", "line": 1, "view": "files"
    });
    if let Some(test_code) = test_code {
        input["testCode"] = serde_json::json!(test_code);
    }
    let row = locations(
        &query(input),
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        found,
        TEST_BUDGET,
        None,
    )
    .await;
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("testCode rows satisfy the output contract");
    (row, root)
}

/// Each listed file with its 1-based lines, relative to `root`.
fn listed(row: &serde_json::Value, root: &std::path::Path) -> Vec<(String, serde_json::Value)> {
    row["files"]
        .as_array()
        .expect("files view")
        .iter()
        .map(|file| {
            let path = file["path"].as_str().expect("path");
            let relative = std::path::Path::new(path)
                .strip_prefix(root)
                .expect("under root");
            (
                relative.to_string_lossy().into_owned(),
                file["lines"].clone(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn references_test_code_exclude_keeps_production_sites_and_counts_the_rest() {
    let (row, root) = reference_files(Some("exclude")).await;
    assert_eq!(
        listed(&row, &root),
        vec![
            ("src/a.ts".to_owned(), serde_json::json!([1])),
            ("src/lib.rs".to_owned(), serde_json::json!([2])),
        ],
        "{row}"
    );
    // The test file is left out whole; the cfg(test) site is a line.
    assert_eq!(
        row["testCodeExcluded"],
        serde_json::json!({"files": 1, "lines": 1}),
        "{row}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn references_test_code_only_keeps_test_sites() {
    let (row, root) = reference_files(Some("only")).await;
    assert_eq!(
        listed(&row, &root),
        vec![
            ("src/a.test.ts".to_owned(), serde_json::json!([1])),
            ("src/lib.rs".to_owned(), serde_json::json!([5])),
        ],
        "{row}"
    );
    assert_eq!(
        row["testCodeExcluded"],
        serde_json::json!({"files": 1, "lines": 1}),
        "{row}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn references_without_test_code_keep_every_site() {
    for test_code in [None, Some("include")] {
        let (row, root) = reference_files(test_code).await;
        assert_eq!(listed(&row, &root).len(), 3, "{row}");
        assert!(row.get("testCodeExcluded").is_none(), "{row}");
    }
}

/// Callers are kept or left out by the caller's own declaration line.
#[test]
fn callers_test_code_scopes_by_the_calling_symbol() {
    let (_dir, root, paths, _) = sites();
    let caller = |name: &str, line: u64| {
        serde_json::json!({
            "from": {"symbolName": "c", "kind": "function", "path": root.join(name).to_string_lossy(),
                     "displayRange": {"startLine": line, "startCharacter": 1, "endLine": line}},
            "fromRanges": [{"startLine": line, "startCharacter": 1, "endLine": line}],
            "level": 1
        })
    };
    let all = vec![
        caller("src/a.ts", 1),
        caller("src/a.test.ts", 1),
        caller("src/lib.rs", 2),
        caller("src/lib.rs", 5),
    ];
    let q = |test_code: &str| {
        query(serde_json::json!({
            "operation": "callers", "path": root.join("src/lib.rs").to_string_lossy(),
            "symbolName": "foo", "line": 1, "testCode": test_code
        }))
    };
    let mut kept = all.clone();
    let excluded = super::super::test_scope::retain_callers(&q("exclude"), &paths, &mut kept);
    let lines = |items: &[serde_json::Value]| {
        items
            .iter()
            .map(|item| {
                item["from"]["displayRange"]["startLine"]
                    .as_u64()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(kept, vec![all[0].clone(), all[2].clone()]);
    assert_eq!(
        excluded.map(|excluded| (excluded.files, excluded.lines)),
        Some((1, 1))
    );
    let mut only = all.clone();
    super::super::test_scope::retain_callers(&q("only"), &paths, &mut only);
    assert_eq!(lines(&only), [1, 5]);
    let mut every = all.clone();
    assert!(super::super::test_scope::retain_callers(&q("include"), &paths, &mut every).is_none());
    assert_eq!(every, all);
}
