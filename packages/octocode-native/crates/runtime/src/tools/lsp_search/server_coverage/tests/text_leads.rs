//! Text leads inside the anchor file and shared-module copies.
use super::*;

/// A use the answer misses inside the anchor file itself (an answered
/// file, so no file count shows it) is still read by the lexical lead;
/// a fully answered anchor file is not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_lead_reads_misses_inside_the_anchor_file() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let package = root.join("pkg");
    std::fs::create_dir_all(package.join("src")).expect("src");
    std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(
        package.join("src/user.rs"),
        "fn u(q: &R) { q.include_diff(); }\n",
    )
    .expect("user");
    let lib = package.join("src/lib.rs");
    std::fs::write(
        &lib,
        "include!(\"../../contract/types.rs\");\nimpl R { fn include_diff(&self) -> bool { true } }\nfn t(r: &R) -> bool { r.include_diff() }\n",
    )
    .expect("lib");
    let lib_path = lib.to_string_lossy().into_owned();
    let user_path = package.join("src/user.rs").to_string_lossy().into_owned();
    let policy = crate::tools::test_support::workspace_policy(&root);
    let root_text = root.to_string_lossy().into_owned();
    let lead_paths =
        |covered: Vec<u32>| {
            let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
            let (lib_path, user_path, root_text, policy) = (
                lib_path.clone(),
                user_path.clone(),
                root_text.clone(),
                &policy,
            );
            async move {
                scope
                    .text_files(
                        "include_diff",
                        policy,
                        &crate::execution::cancel::NeverCancel,
                    )
                    .await
                    .expect("scan");
                scope.answer([lib_path.clone()]);
                scope.answer_anchor_lines(&lib_path, 1, covered);
                let mut row = json!({"kind":"references","matches":[{"path":"pkg/src/lib.rs"}]});
                annotate(
                    &mut row,
                    &query(json!({"path": lib_path, "operation": "references",
                    "symbolName": "include_diff", "line": 2})),
                    Some("rust"),
                    Server {
                        anchor: &lib_path,
                        root: &root_text,
                    },
                    &scope,
                );
                assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
                let mut paths =
                    crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)
                        ["input"]["queries"]
                        .as_array()
                        .expect("lead rows")
                        .iter()
                        .map(|search| search["path"].as_str().expect("path").to_owned())
                        .collect::<Vec<_>>();
                paths.sort();
                assert!(paths.contains(&user_path), "{row}");
                paths
            }
        };
    // Line 3 (zero-based 2) uses the name; the answer lists only the
    // declaration.
    let missed = lead_paths(vec![1]).await;
    assert_eq!(missed.len(), 2, "{missed:?}");
    assert!(missed.contains(&lib_path), "{missed:?}");
    let covered = lead_paths(vec![1, 2]).await;
    assert_eq!(covered, std::slice::from_ref(&user_path));
}

/// A support file a bench and a test both declare by `#[path]` is
/// compiled into each: rust-analyzer answers for one copy, so an answer
/// that misses the other declaring file is partial, names both, and
/// leads to the text check; an answer covering both is complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_module_files_flag_the_unanswered_copy() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let package = root.join("pkg");
    for sub in ["benches/support", "tests", "src"] {
        std::fs::create_dir_all(package.join(sub)).expect("dir");
    }
    std::fs::write(package.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(package.join("src/lib.rs"), "pub fn other() {}\n").expect("lib");
    let support = package.join("benches/support/frozen.rs");
    std::fs::write(&support, "pub fn frozen_graph() {}\n").expect("support");
    let bench = package.join("benches/graph.rs");
    std::fs::write(
        &bench,
        "#[path = \"support/frozen.rs\"]\nmod frozen;\nfn main() { frozen::frozen_graph(); }\n",
    )
    .expect("bench");
    let test = package.join("tests/graph.rs");
    std::fs::write(
        &test,
        "#[path = \"../benches/support/frozen.rs\"]\nmod frozen;\n#[test]\nfn t() { frozen::frozen_graph(); }\n",
    )
    .expect("test");
    let path = |file: &Path| file.to_string_lossy().into_owned();
    let (support_path, root_text) = (path(&support), path(&root));
    let policy = crate::tools::test_support::workspace_policy(&root);
    let row_for = |answered: Vec<String>| {
        let scope = Scope::new(root_text.clone(), vec!["*.rs".into()]);
        let (support_path, root_text, policy) = (support_path.clone(), root_text.clone(), &policy);
        async move {
            scope
                .text_files(
                    "frozen_graph",
                    policy,
                    &crate::execution::cancel::NeverCancel,
                )
                .await
                .expect("scan");
            scope.answer(answered);
            let mut row = json!({"kind":"references","matches":[{"path":"pkg/tests/graph.rs"}]});
            annotate(
                &mut row,
                &query(json!({"path": support_path, "operation": "references",
                    "symbolName": "frozen_graph", "line": 1})),
                Some("rust"),
                Server {
                    anchor: &support_path,
                    root: &root_text,
                },
                &scope,
            );
            row
        }
    };
    let missed = row_for(vec![support_path.clone(), path(&test)]).await;
    assert_eq!(
        missed["partialReasons"],
        json!([SHARED_MODULE_REASON]),
        "{missed}"
    );
    assert_eq!(missed["coverage"]["textOnlyFiles"], 1, "{missed}");
    assert!(
        missed["notices"][0].as_str().is_some_and(|notice| {
            notice.contains("pkg/benches/graph.rs") && notice.contains("pkg/tests/graph.rs")
        }),
        "{missed}"
    );
    assert!(
        missed["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localSearch"),
        "{missed}"
    );
    let complete = row_for(vec![support_path.clone(), path(&test), path(&bench)]).await;
    assert!(complete.get("isPartial").is_none(), "{complete}");
    assert_eq!(complete["coverage"]["textOnlyFiles"], 0, "{complete}");
}
