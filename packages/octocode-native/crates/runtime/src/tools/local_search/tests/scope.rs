use super::*;
use octocode_contract_types::ErrorCode;

fn paths(body: &serde_json::Value) -> std::collections::BTreeSet<String> {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["path"].as_str().expect("path").to_owned())
        .collect()
}

#[test]
fn authored_globs_file_types_and_depth_select_the_requested_scope() {
    let files = [
        ("src/a.rs", "needle\n"),
        ("src/deep/b.rs", "needle\n"),
        ("src/c.ts", "needle\n"),
        ("other.rs", "needle\n"),
    ];
    for (fields, expected) in [
        (
            serde_json::json!({"include":["src/**/*.rs"]}),
            vec!["src/a.rs", "src/deep/b.rs"],
        ),
        (
            serde_json::json!({"fileType":"rust","exclude":["src/deep/**"]}),
            vec!["other.rs", "src/a.rs"],
        ),
        (serde_json::json!({"maxDepth":1}), vec!["other.rs"]),
        (serde_json::json!({"include":["src"]}), vec![]),
    ] {
        let q = ls_query(
            fields,
            Some(&ls_query(
                serde_json::json!({"matchString":["needle"],"view":"files"}),
                None,
            )),
        );
        let body = search_fixture(&files, q);
        assert_eq!(
            paths(&body),
            expected.into_iter().map(str::to_owned).collect(),
            "{body}"
        );
    }
}

#[test]
fn walk_options_lift_only_their_own_exclusions() {
    let root = tempfile::tempdir().expect("fixture");
    for (path, body) in [
        (".ignore", "ignored.txt\n"),
        ("visible.txt", "needle\n"),
        ("ignored.txt", "needle\n"),
        (".hidden.txt", "needle\n"),
        ("node_modules/pkg/a.txt", "needle\n"),
        (".env", "password=needle\n"),
    ] {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, body).expect("file");
    }
    for (options, expected) in [
        (serde_json::json!({}), vec!["visible.txt"]),
        (
            serde_json::json!({"includeHidden":true}),
            vec![".hidden.txt", "visible.txt"],
        ),
        (
            serde_json::json!({"includeIgnored":true}),
            vec!["ignored.txt", "visible.txt"],
        ),
        (
            serde_json::json!({"defaultExcludes":false}),
            vec!["node_modules/pkg/a.txt", "visible.txt"],
        ),
        (
            serde_json::json!({"includeHidden":true,"includeIgnored":true,"defaultExcludes":false}),
            vec![
                ".hidden.txt",
                "ignored.txt",
                "node_modules/pkg/a.txt",
                "visible.txt",
            ],
        ),
    ] {
        let base = ls_query(
            serde_json::json!({"path":root.path(),"matchString":["needle"],"view":"files"}),
            None,
        );
        let (policy, security) = policy_for(root.path());
        let result =
            search_in(&ls_query(options, Some(&base)), &policy, &security, None).expect("search");
        let body = result.to_row().expect("result");
        assert_eq!(
            paths(&body),
            expected.into_iter().map(str::to_owned).collect(),
            "{body}"
        );
    }
}

#[cfg(unix)]
#[test]
fn escaped_symlinks_and_sensitive_files_are_excluded_before_matching() {
    let root = tempfile::tempdir().expect("fixture");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret.rs"), "needle outside\n").expect("outside file");
    std::os::unix::fs::symlink(
        outside.path().join("secret.rs"),
        root.path().join("linked.rs"),
    )
    .expect("link");
    fs::write(root.path().join(".env"), "password=needle\n").expect("sensitive");
    fs::write(root.path().join("safe.rs"), "needle safe\n").expect("safe");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString":["needle"],"includeHidden":true,"includeIgnored":true,"defaultExcludes":false}),
    );
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path, "safe.rs");
    assert!(!values(&result).contains("outside"));
}

#[test]
fn cancellation_interrupts_the_walk() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfter(AtomicUsize);
    impl crate::execution::cancel::CancellationCheck for CancelAfter {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::SeqCst) >= 3 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    let root = tempfile::tempdir().expect("fixture");
    for n in 0..40 {
        fs::write(root.path().join(format!("f{n}.txt")), "needle\n").expect("file");
    }
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        None,
    );
    let cancelled = execute_local_search(
        &q,
        &policy,
        &security,
        &CancelAfter(AtomicUsize::new(0)),
        None,
        None,
    )
    .expect_err("cancelled");
    assert_eq!(cancelled.code, ErrorCode::Cancelled);
}

#[test]
fn counts_and_inversion_keep_occurrence_and_line_meanings_distinct() {
    let files = [("a.txt", "foo foo\nfoo\nnone\n"), ("b.txt", "none\n")];
    for (view, expected) in [("countMatches", 3), ("countLines", 2)] {
        let body = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString":["foo"],"view":view}), None),
        );
        let key = if view == "countLines" {
            "matchedLineCount"
        } else {
            "matchCount"
        };
        assert_eq!(body["files"][0][key], expected, "{body}");
    }
    let inverted = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":["foo"],"invertMatch":true}),
            None,
        ),
    );
    assert_eq!(inverted["matchedLineCount"], 2, "{inverted}");
    assert!(
        inverted["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|f| f["matches"]
                .as_array()
                .expect("rows")
                .iter()
                .all(|m| m["value"].as_str().expect("value") == "none"))
    );
    let without = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":["foo"],"view":"filesWithout"}),
            None,
        ),
    );
    assert_eq!(paths(&without), ["b.txt".to_owned()].into_iter().collect());
}

/// A search of the workspace root (`.`) leads to a read of its top hit by
/// the path its row shows, not a `./`-prefixed second spelling of it.
#[test]
fn a_root_search_reads_its_hit_by_the_row_path() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("src")).expect("src");
    fs::write(root.path().join("src/a.rs"), "fn needle() {}\n").expect("source");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":".","matchString":["needle"]}),
        None,
    );
    let body = serde_json::to_value(search_in(&request, &policy, &security, None).expect("search"))
        .expect("serialize");
    assert_eq!(body["files"][0]["path"], "src/a.rs", "{body}");
    let read = crate::tools::test_support::checks::related_call(&body, "localFetch", |_| true);
    assert_eq!(read["input"]["queries"][0]["path"], "src/a.rs", "{body}");
}
