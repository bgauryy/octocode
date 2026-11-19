use super::*;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;

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
            json_fixture(r#"{"include": ["src/**/*.rs"]}"#),
            vec!["src/a.rs", "src/deep/b.rs"],
        ),
        (
            json_fixture(r#"{"fileType": "rust", "exclude": ["src/deep/**"]}"#),
            vec!["other.rs", "src/a.rs"],
        ),
        (json_fixture(r#"{"maxDepth": 1}"#), vec!["other.rs"]),
        (json_fixture(r#"{"include": ["src"]}"#), vec![]),
    ] {
        let q = ls_query(
            fields,
            Some(&ls_query(
                json_fixture(r#"{"matchString": ["needle"], "view": "files"}"#),
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
            json_fixture(r#"{"includeHidden": true}"#),
            vec![".hidden.txt", "visible.txt"],
        ),
        (
            json_fixture(r#"{"includeIgnored": true}"#),
            vec!["ignored.txt", "visible.txt"],
        ),
        (
            json_fixture(r#"{"defaultExcludes": false}"#),
            vec!["node_modules/pkg/a.txt", "visible.txt"],
        ),
        (
            json_fixture(
                r#"{
                    "includeHidden": true,
                    "includeIgnored": true,
                    "defaultExcludes": false
                }"#,
            ),
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

/// Octocode's own graph store is skipped whatever the flags, and disclosed
/// as such; skill outputs elsewhere under `.octocode` stay searchable, and a
/// path inside the store searches it.
#[test]
fn the_graph_store_is_skipped_unless_the_path_names_it() {
    let root = tempfile::tempdir().expect("fixture");
    for path in [
        "visible.txt",
        ".octocode/graph/snapshot.txt",
        ".octocode/octocode-dev/report.txt",
    ] {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, "needle\n").expect("file");
    }
    let every = json_fixture(
        r#"{
            "matchString": ["needle"],
            "view": "files",
            "includeHidden": true,
            "includeIgnored": true,
            "defaultExcludes": false
        }"#,
    );
    let body = search_root(root.path(), every.clone())
        .to_row()
        .expect("row");
    assert_eq!(
        paths(&body),
        [".octocode/octocode-dev/report.txt", "visible.txt"]
            .map(str::to_owned)
            .into(),
        "{body}"
    );
    let skipped = body["skippedPaths"]["entries"].as_array().expect("skipped");
    assert!(
        skipped
            .iter()
            .any(|entry| entry["cause"] == "octocode-artifact"
                && entry["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(".octocode/graph"))),
        "{body}"
    );
    let inside = search_root(&root.path().join(".octocode/graph"), every)
        .to_row()
        .expect("row");
    assert_eq!(paths(&inside).len(), 1, "{inside}");
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
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "includeHidden": true,
                "includeIgnored": true,
                "defaultExcludes": false
            }"#,
        ),
    );
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path, "safe.rs");
    assert!(!values(&result).contains("outside"));
}

#[test]
fn cancellation_interrupts_the_walk() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfter(AtomicUsize);
    impl octocode_runtime_base::execution::cancel::CancellationCheck for CancelAfter {
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
        octocode_runtime_base::test_support::BUILD_IDENTITY,
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
            json_fixture(r#"{"matchString": ["foo"], "invertMatch": true}"#),
            None,
        ),
    );
    assert_eq!(inverted["matchCount"], 2, "{inverted}");
    assert!(inverted.get("matchedLineCount").is_none(), "{inverted}");
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
            json_fixture(r#"{"matchString": ["foo"], "view": "filesWithout"}"#),
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
        json_fixture(r#"{"path": ".", "matchString": ["needle"]}"#),
        None,
    );
    let body = serde_json::to_value(search_in(&request, &policy, &security, None).expect("search"))
        .expect("serialize");
    assert_eq!(body["files"][0]["path"], "src/a.rs", "{body}");
    let read =
        octocode_runtime_base::test_support::checks::related_call(&body, "localFetch", |_| true);
    assert_eq!(read["input"]["queries"][0]["path"], "src/a.rs", "{body}");
}

/// A file that is both left out by a walk rule and a credential (`secret.pem`
/// listed in `.ignore`; `.env`, a dot file) is disclosed exactly once, as the
/// path policy withholds it: counted by its holder and reason, never named,
/// and never among the ignored or hidden entries, whose flags would not read
/// it.
#[test]
fn an_ignored_or_hidden_credential_is_disclosed_once_as_withheld_never_named() {
    let root = tempfile::tempdir().expect("fixture");
    for (path, body) in [
        (".ignore", "secret.pem\nignored.txt\n"),
        ("secret.pem", "needle\n"),
        ("ignored.txt", "needle\n"),
        (".env", "password=needle\n"),
        ("visible.txt", "needle\n"),
    ] {
        fs::write(root.path().join(path), body).expect("file");
    }
    let query = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"],"view":"files"}),
        None,
    );
    let (policy, security) = policy_for(root.path());
    let body = search_in(&query, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("result");
    let entries = body["skippedPaths"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let counted = |cause: &str| {
        entries
            .iter()
            .filter(|entry| entry["cause"] == cause)
            .map(|entry| entry["count"].as_u64().unwrap_or(1))
            .sum::<u64>()
    };
    assert_eq!(
        counted("withheld"),
        2,
        "secret.pem and .env, once each: {body}"
    );
    assert_eq!(counted("ignored"), 1, "ignored.txt only: {body}");
    assert_eq!(counted("hidden"), 1, ".ignore only: {body}");
    let listed = serde_json::Value::Array(entries).to_string();
    assert!(
        !listed.contains("secret.pem") && !listed.contains(".env"),
        "a withheld credential is never named: {listed}"
    );
    assert_eq!(paths(&body), ["visible.txt".to_owned()].into());
}

/// Every call a row offers: its continuations and its related leads.
fn offered(row: &serde_json::Value) -> Vec<serde_json::Value> {
    let related = row["related"].as_array().into_iter().flatten().cloned();
    let next = row["next"]
        .as_object()
        .into_iter()
        .flat_map(|calls| calls.values().cloned());
    related.chain(next).collect()
}

/// Default-excluded paths name the rule that left them out, so two rules
/// are two entries, each naming its own path; one rule's paths count under
/// the directory holding them, never under one of their names.
#[test]
fn default_excluded_entries_name_their_rule() {
    let body = search_fixture(
        &[
            ("a.txt", "beacon\n"),
            ("bundle.min.js", "beacon\n"),
            ("blob.bin", "beacon\n"),
            ("node_modules/x.txt", "beacon\n"),
            ("pkg/node_modules/y.txt", "beacon\n"),
        ],
        ls_query(serde_json::json!({"matchString": ["beacon"]}), None),
    );
    let excluded = body["skippedPaths"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["cause"] == "default-excluded")
        .map(|entry| {
            (
                entry["reason"].as_str().unwrap_or_default().to_owned(),
                entry["path"].as_str().unwrap_or_default().to_owned(),
                entry["count"].as_u64(),
            )
        })
        .collect::<Vec<_>>();
    let by_rule = |rule: &str| {
        excluded
            .iter()
            .find(|(reason, ..)| reason == rule)
            .unwrap_or_else(|| panic!("no {rule} entry: {body}"))
    };
    assert!(by_rule("*.min.js").1.ends_with("bundle.min.js"), "{body}");
    assert_eq!(by_rule("*.min.js").2, None, "{body}");
    assert!(by_rule("*.bin").1.ends_with("blob.bin"), "{body}");
    let modules = by_rule("node_modules/");
    assert_eq!(modules.2, Some(2), "{body}");
    assert!(!modules.1.ends_with("node_modules"), "{body}");
    assert_eq!(excluded.len(), 3, "{body}");
}

/// A key path reads only JSON and TOML documents: a scope with none of them
/// searches nothing, and the row says so and offers the same search
/// without the key path, never a wider search with it. The globs and the
/// path are not blamed.
#[test]
fn a_key_path_over_files_it_cannot_read_says_why_and_leads_without_it() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("many.txt"), "beacon\n").expect("fixture");
    fs::write(root.path().join("notes.md"), "beacon\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let dir = root.path().to_string_lossy().into_owned();
    let file = root.path().join("many.txt").to_string_lossy().into_owned();
    for (path, include) in [
        (file.as_str(), None),
        (dir.as_str(), None),
        (dir.as_str(), Some(serde_json::json!(["*.txt"]))),
    ] {
        let mut fields =
            serde_json::json!({"path": path, "matchString": ["beacon"], "keyPath": "name"});
        if let Some(include) = include {
            fields["include"] = include;
        }
        let request = ls_query(fields, None);
        let row = search_in(&request, &policy, &security, None)
            .expect("search")
            .to_row()
            .expect("row");
        let hints = row["hints"].to_string();
        assert!(
            hints.contains("keyPath reads only .json, .jsonc, .json5 and .toml files"),
            "{path}: {row}"
        );
        assert!(
            !hints.contains("include/exclude matched no file")
                && !hints.contains("no file under path"),
            "{path}: {row}"
        );
        let calls = offered(&row);
        assert!(
            calls.iter().all(|call| call["input"]["queries"][0]
                .get("keyPath")
                .is_none_or(|_| call["input"]["queries"][0]["path"] == path)),
            "no wider search keeps the key path: {path}: {row}"
        );
        assert!(
            calls.iter().any(|call| {
                let row = &call["input"]["queries"][0];
                call["tool"] == "localSearch" && row["path"] == path && row.get("keyPath").is_none()
            }),
            "the same search without the key path: {path}: {row}"
        );
    }
}

/// An empty page reads nothing its walk skipped, as ripgrep does not: it says
/// the skipped paths were not searched, by cause, and `next.includeSkipped`
/// searches them with exactly those causes lifted. The page's own hits stay
/// none, whatever the skipped paths hold.
#[test]
fn an_empty_page_offers_its_skipped_paths_and_reads_none_of_them() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("visible.txt"), "nothing\n").expect("file");
    let excluded = root.path().join("node_modules");
    fs::create_dir_all(&excluded).expect("directory");
    fs::write(excluded.join("dep.txt"), "needle\n").expect("file");
    let query = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"],"view":"files"}),
        None,
    );
    let (policy, security) = policy_for(root.path());
    let body = search_in(&query, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("result");
    assert!(paths(&body).is_empty(), "an empty page: {body}");
    let hints = body["hints"].to_string();
    assert!(
        hints.contains("Skipped paths (1 default-excluded) were not searched"),
        "{body}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&body, "localSearch", |input| {
            input["queries"][0]["defaultExcludes"] == false
        });
    assert_eq!(
        lead["input"]["queries"][0]["matchString"],
        serde_json::json!(["needle"]),
        "{body}"
    );
    assert!(
        !body.to_string().contains("dep.txt"),
        "nothing behind it is named: {body}"
    );
}

/// An empty page whose walk skipped paths across the whole scope names each
/// top directory it skipped and offers a search of one of them, with only
/// the flags its cause needs, before the one over everything skipped: a
/// dependency directory the default prune names first. That scoped search
/// finds what the whole-scope lift would, reading only that directory.
#[test]
fn an_empty_page_offers_a_search_of_each_skipped_top_directory() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(
        root.join(".gitignore"),
        "node_modules/\nreleases/\npackages/*/dist/\n",
    )
    .expect("fixture");
    fs::write(root.join("visible.txt"), "nothing\n").expect("file");
    for (path, text) in [
        (
            "node_modules/pkce/index.js",
            "export function verifyChallenge() {}\n",
        ),
        ("releases/notes.txt", "nothing\n"),
        ("packages/a/dist/out.js", "nothing\n"),
        ("packages/b/dist/out.js", "nothing\n"),
        ("packages/a/src.txt", "nothing\n"),
    ] {
        let at = root.join(path);
        fs::create_dir_all(at.parent().expect("parent")).expect("dir");
        fs::write(at, text).expect("file");
    }
    let (policy, security) = policy_for(&root);
    let query = ls_query(
        serde_json::json!({"path": root, "matchString": ["verifyChallenge"]}),
        None,
    );
    let body = search_in(&query, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("result");
    assert!(paths(&body).is_empty(), "an empty page: {body}");
    let skipped = body["skippedPaths"]["entries"].to_string();
    assert!(
        skipped.contains("node_modules") && skipped.contains("ignored-default-excluded"),
        "{body}"
    );
    let related = body["related"].as_array().expect("related");
    let scoped = |call: &serde_json::Value| {
        let row = &call["input"]["queries"][0];
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("node_modules"))
            .then(|| row.clone())
    };
    let (at, lead) = related
        .iter()
        .enumerate()
        .find_map(|(at, call)| scoped(call).map(|row| (at, row)))
        .unwrap_or_else(|| panic!("a node_modules search: {body}"));
    assert_eq!(lead["includeIgnored"], true, "{lead}");
    assert_eq!(lead["defaultExcludes"], false, "{lead}");
    assert!(lead.get("includeHidden").is_none(), "{lead}");
    // The search over everything skipped: the scope's own path, every
    // cause lifted.
    let everything = related
        .iter()
        .position(|call| {
            let row = &call["input"]["queries"][0];
            row["path"] == serde_json::json!(root) && row["includeIgnored"] == true
        })
        .unwrap_or_else(|| panic!("the search over everything skipped: {body}"));
    assert!(at < everything, "the scoped search comes first: {body}");
    let found = search_in(
        &serde_json::from_value(lead.clone()).expect("lead query"),
        &policy,
        &security,
        None,
    )
    .expect("scoped search")
    .to_row()
    .expect("result");
    assert_eq!(
        found["filesScanned"], 1,
        "only node_modules is read: {found}"
    );
    assert!(found.to_string().contains("index.js"), "{found}");
}

/// TE1: a skipped directory named with a secret is never served as written:
/// the scoped `skippedPaths` and the leads that name it pass the output gate
/// like every other path, so the served answer shows its placeholder.
#[test]
fn a_secret_named_skipped_directory_is_served_redacted() {
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(
        root.join(".gitignore"),
        format!("{token}/\nnode_modules/\n"),
    )
    .expect("fixture");
    fs::write(root.join("visible.txt"), "nothing\n").expect("file");
    for path in [
        format!("{token}/dep.txt"),
        "node_modules/pkg/index.js".to_owned(),
    ] {
        let at = root.join(path);
        fs::create_dir_all(at.parent().expect("parent")).expect("dir");
        fs::write(at, "nothing\n").expect("file");
    }
    let (policy, security) = policy_for(&root);
    let query = ls_query(
        serde_json::json!({"path": root, "matchString": ["absentEverywhere"]}),
        None,
    );
    let body = search_in(&query, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("result");
    assert!(body.to_string().contains("skippedPaths"), "{body}");
    let served = ContentSecurity::new()
        .seal_output(
            octocode_runtime_base::test_support::published(
                octocode_runtime_base::id::ToolId::LocalSearch,
                &body,
            ),
            false,
            || Ok::<(), ()>(()),
        )
        .expect("sealed")
        .into_value();
    assert!(!served.to_string().contains(&token), "{served}");
    // No lead aims at a path the gate redacts: it could not run as served.
    let related = body["related"].as_array().expect("related");
    assert!(
        related
            .iter()
            .all(|call| !call["input"]["queries"][0]["path"]
                .as_str()
                .is_some_and(|path| path.contains("ghp_"))),
        "{body}"
    );
    assert!(
        related
            .iter()
            .any(|call| call["input"]["queries"][0]["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("node_modules"))),
        "{body}"
    );
}
