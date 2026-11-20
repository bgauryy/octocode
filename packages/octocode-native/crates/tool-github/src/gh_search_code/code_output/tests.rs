//! File identity, source-read guidance, and identical evidence at multiple paths.
use super::super::leads::unsearched_ref_read;
use super::*;
use octocode_test_support::json_fixture;

/// Path projection avoids blob reads but preserves the indexed commit in its read lead.
#[test]
fn path_only_handoffs_preserve_index_commits_without_using_blob_shas() {
    let commit = "a".repeat(40);
    let blob = "b".repeat(40);
    for repo in [Some("r"), None] {
        for requested in [None, Some("release/v1")] {
            for indexed in [commit.as_str(), "main", "0123"] {
                let query: GhSearchCodeQuery = serde_json::from_value(json!({
                    "owner":"o", "repo":repo, "keywords":["needle"],
                    "fields":["path"], "ref":requested
                }))
                .expect("query");
                let mut items = vec![json_fixture(
                    r#"{
                        "owner": "o",
                        "repo": "r",
                        "path": "src/lib.rs",
                        "fragments": [{"highlight": "needle"}]
                    }"#,
                )];
                let found: CodeSearchItem = serde_json::from_value(json!({
                    "name":"lib.rs", "path":"src/lib.rs", "sha":blob,
                    "html_url":format!("https://github.com/o/r/blob/{indexed}/src/lib.rs"),
                    "repository":{
                        "full_name":"o/r", "html_url":"https://github.com/o/r",
                        "url":"https://api.github.com/repos/o/r"
                    }
                }))
                .expect("provider item");
                index_commits(&mut items, &[found], &query);
                let reads = shape_files(
                    &mut items,
                    &query,
                    &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
                    None,
                );
                assert!(reads.hits.is_empty(), "paths do not require source reads");
                let lead = reads.top.expect("top path has an executable read");
                octocode_runtime_base::contracts::validate_saved(
                    "ghGetFileContent",
                    lead["input"].clone(),
                )
                .expect("read follows the published contract");
                let next = &lead["input"]["queries"][0];
                let expected =
                    requested.or_else(|| (indexed == commit.as_str()).then_some(indexed));
                assert_eq!(
                    next["ref"].as_str(),
                    expected,
                    "{repo:?}, {requested:?}, {indexed}"
                );
                assert_ne!(
                    next["ref"].as_str(),
                    Some(blob.as_str()),
                    "blob identity is not a revision"
                );
                assert_eq!(next["path"], "src/lib.rs");
                if expected.is_none() {
                    assert!(
                        items[0].get("commitSha").is_none(),
                        "no immutable identity was proven"
                    );
                }
            }
        }
    }
}

/// Missing commit metadata cannot silently remove the requested source lines.
/// Every unresolved row retains its own read, without an invented immutable ref.
#[test]
fn unresolved_code_rows_keep_required_reads_without_inventing_commits() {
    use super::super::lines::FileHits;
    let query: GhSearchCodeQuery =
        serde_json::from_value(json_fixture(r#"{"owner": "o", "keywords": ["needle"]}"#))
            .expect("query");
    for planned in [false, true] {
        let mut items = vec![
            json_fixture(r#"{"owner": "o", "repo": "r", "path": "a.rs", "fragments": []}"#),
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "s",
                    "path": "README.md",
                    "fragments": []
                }"#,
            ),
        ];
        let resolution = planned.then(|| Resolution {
            sha: None,
            reference: None,
            hits: vec![(String::new(), FileHits::Unavailable); items.len()],
            unlisted: Vec::new(),
        });
        let reads = shape_files(
            &mut items,
            &query,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
            resolution,
        );
        assert_eq!(reads.hits.len(), items.len(), "{planned}: {items:?}");
        assert_eq!(reads.unanchored, items.len());
        let mut page = json!({});
        super::super::page_facts::disclose_reads(
            &mut page,
            &reads,
            &octocode_tools_shared::gh_shared::test_support::default_context(),
        );
        assert!(
            page["warnings"]
                .to_string()
                .contains("cannot verify the historical index snapshot"),
            "{page}"
        );
        for (row, read) in items.iter().zip(&reads.hits) {
            assert_eq!(row["lineResolved"], false, "{row}");
            assert!(row.get("commitSha").is_none(), "{row}");
            assert_eq!(read["kind"], "continue", "{read}");
            let input = &read["input"];
            octocode_runtime_base::contracts::validate_saved("ghGetFileContent", input.clone())
                .expect("required read is executable");
            assert_eq!(input["queries"][0]["path"], row["path"]);
            assert!(input["queries"][0].get("ref").is_none(), "{read}");
        }
    }
}

/// Shaping preserves source identity for both repository and owner scopes,
/// with or without resolved source lines.
#[test]
fn shaped_files_retain_repository_and_commit_identity() {
    use super::super::lines::FileHits;
    for repo in [Some("r"), None] {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"o", "repo":repo, "keywords":["needle"]
        }))
        .expect("query");
        for resolved in [false, true] {
            let sha = "a".repeat(40);
            let mut items = vec![
                json_fixture(r#"{"owner": "o", "repo": "r", "path": "a.rs", "fragments": []}"#),
                json_fixture(r#"{"owner": "o", "repo": "r", "path": "b.rs", "fragments": []}"#),
            ];
            let resolution = resolved.then(|| Resolution {
                sha: Some(sha.clone()),
                reference: None,
                hits: (0..2)
                    .map(|_| {
                        (
                            sha.clone(),
                            FileHits::Lines {
                                lines: vec!["1\tneedle".into()],
                                matched: vec![1],
                                first: 1,
                                last: 1,
                                best: 1,
                                total: 1,
                                line_count: 1,
                                declaration: None,
                                redacted: 0,
                            },
                        )
                    })
                    .collect(),
                unlisted: Vec::new(),
            });
            shape_files(
                &mut items,
                &query,
                &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
                resolution,
            );
            for (row, path) in items.iter().zip(["a.rs", "b.rs"]) {
                assert_eq!(row["owner"], "o");
                assert_eq!(row["repo"], "r");
                assert_eq!(row["path"], path);
                if resolved {
                    assert_eq!(row["commitSha"], sha);
                    assert_eq!(row["matches"], json!(["1\tneedle"]));
                } else {
                    assert!(row.get("commitSha").is_none());
                }
            }
        }
    }
}

#[test]
fn unsearched_ref_guidance_distinguishes_named_files_and_scopes() {
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "o", "repo": "r", "keywords": ["x"], "ref": "dev"}"#,
    ))
    .expect("query");
    // The empty row's one fact is the ref the index did not cover; a
    // named file at that ref is read there.
    assert_eq!(
        empty_hints(&query, None),
        ["Code search indexes only the default branch; `dev` was not searched."]
    );
    // Known, the default branch is named; a ref naming it was searched.
    assert_eq!(
        empty_hints(&query, Some("main")),
        ["Code search indexes only the default branch `main`; `dev` was not searched."]
    );
    assert!(empty_hints(&query, Some("dev")).is_empty());
    assert_eq!(unsearched_ref(&query, Some("dev")), None);
    assert!(unsearched_ref_read(&query).is_none());
    let mut named = query.clone();
    named.name_contains = Some("Cargo.toml".parse().expect("filename"));
    named.path = Some("crates/x".parse().expect("path"));
    let read = unsearched_ref_read(&named).expect("read at the ref");
    assert_eq!(read["tool"], "ghGetFileContent", "{read}");
    assert_eq!(
        read["input"]["queries"][0]["path"], "crates/x/Cargo.toml",
        "{read}"
    );
    assert_eq!(read["input"]["queries"][0]["ref"], "dev", "{read}");
    let plain: GhSearchCodeQuery =
        serde_json::from_value(json_fixture(r#"{"owner": "o", "keywords": ["x"]}"#))
            .expect("query");
    assert!(empty_hints(&plain, None).is_empty());
    // A path scope adds no hint: `path` names a file or a folder.
    let mut both = query.clone();
    both.path = Some("crates/x".parse().expect("path"));
    assert_eq!(
        empty_hints(&both, None),
        ["Code search indexes only the default branch; `dev` was not searched."]
    );
}

/// Files whose listed evidence is identical are listed once, naming
/// every path that shares it; path-only rows and differing rows stay.
#[test]
fn identical_files_are_listed_once_with_every_path() {
    let header = json_fixture(r##"[{"line": 1, "value": "# Octocode"}]"##);
    let mut items = vec![
        json!({"path": "a/README.md", "matches": header}),
        json_fixture(
            r#"{
                "path": "src/x.rs",
                "matches": [{"line": 3, "value": "fn octocode()"}]
            }"#,
        ),
        json!({"path": "b/README.md", "matches": header}),
        json!({"path": "c/README.md", "matches": header, "matchCount": 40}),
        json!({"path": "d/README.md", "matches": header}),
        json_fixture(r#"{"path": "p1"}"#),
        json_fixture(r#"{"path": "p2"}"#),
    ];
    merge_identical(&mut items);
    assert_eq!(
        items,
        vec![
            json!({"path": "a/README.md", "alsoAt": ["b/README.md", "d/README.md"], "matches": header}),
            json_fixture(
                r#"{
                    "path": "src/x.rs",
                    "matches": [{"line": 3, "value": "fn octocode()"}]
                }"#
            ),
            json!({"path": "c/README.md", "matches": header, "matchCount": 40}),
            json_fixture(r#"{"path": "p1"}"#),
            json_fixture(r#"{"path": "p2"}"#),
        ]
    );
}

/// Evidence is every field but `path`, wherever `path` sits in the row and
/// in the row's order; a row without a `path` is never merged.
#[test]
fn identical_evidence_ignores_only_the_path_entry() {
    let lines = json_fixture(r#"[{"line": 1, "value": "fn main()"}]"#);
    let mut items = vec![
        json!({"matches": lines, "path": "a.rs", "matchCount": 1}),
        json!({"matches": lines, "path": "b.rs", "matchCount": 1}),
        json!({"matches": lines, "matchCount": 1, "path": "c.rs"}),
        json!({"matchCount": 1, "matches": lines, "path": "d.rs"}),
        json!({"matches": lines, "matchCount": 1}),
        json!({"matches": lines, "matchCount": 1}),
    ];
    merge_identical(&mut items);
    assert_eq!(
        items,
        vec![
            json!({"path": "a.rs", "alsoAt": ["b.rs", "c.rs"], "matches": lines, "matchCount": 1}),
            json!({"matchCount": 1, "matches": lines, "path": "d.rs"}),
            json!({"matches": lines, "matchCount": 1}),
            json!({"matches": lines, "matchCount": 1}),
        ]
    );
}
