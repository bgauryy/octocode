//! File identity, source-read guidance, and identical evidence at multiple paths.
use super::super::leads::unsearched_ref_read;
use super::*;

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
                json!({"owner":"o","repo":"r","path":"a.rs","matches":[]}),
                json!({"owner":"o","repo":"r","path":"b.rs","matches":[]}),
            ];
            let resolution = resolved.then(|| Resolution {
                sha: Some(sha.clone()),
                reference: None,
                reads: 2,
                hits: (0..2)
                    .map(|_| {
                        (
                            sha.clone(),
                            FileHits::Lines {
                                lines: vec!["1\tneedle".into()],
                                first: 1,
                                last: 1,
                                best: 1,
                                total: 1,
                                line_count: 1,
                                declaration: None,
                            },
                        )
                    })
                    .collect(),
            });
            shape_files(&mut items, &query, resolution);
            for (row, path) in items.iter().zip(["a.rs", "b.rs"]) {
                assert_eq!(row["owner"], "o");
                assert_eq!(row["repo"], "r");
                assert_eq!(row["path"], path);
                if resolved {
                    assert_eq!(row["commitSha"], sha);
                    assert_eq!(row["lines"], json!(["1\tneedle"]));
                } else {
                    assert!(row.get("commitSha").is_none());
                }
            }
        }
    }
}

#[test]
fn unsearched_ref_guidance_distinguishes_named_files_and_scopes() {
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"o","repo":"r","keywords":["x"],"ref":"dev"
    }))
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
    named.path_prefix = Some("crates/x".parse().expect("path"));
    let read = unsearched_ref_read(&named).expect("read at the ref");
    assert_eq!(read["tool"], "ghGetFileContent", "{read}");
    assert_eq!(
        read["input"]["queries"][0]["path"], "crates/x/Cargo.toml",
        "{read}"
    );
    assert_eq!(read["input"]["queries"][0]["ref"], "dev", "{read}");
    let plain: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"o","keywords":["x"]
    }))
    .expect("query");
    assert!(empty_hints(&plain, None).is_empty());
    // A requested ref plus a path scope are two hints, never one composed
    // sentence.
    let mut both = query.clone();
    both.path_prefix = Some("crates/x".parse().expect("path"));
    let hints = empty_hints(&both, None);
    assert_eq!(hints.len(), 2, "{hints:?}");
    assert!(
        hints[0].starts_with("Code search indexes only"),
        "{hints:?}"
    );
    assert!(
        hints[1].starts_with("`pathPrefix:\"crates/x\"` is a directory scope"),
        "{hints:?}"
    );
}

/// Files whose listed evidence is identical are listed once, naming
/// every path that shares it; path-only rows and differing rows stay.
#[test]
fn identical_files_are_listed_once_with_every_path() {
    let header = json!(["1\t# Octocode"]);
    let mut items = vec![
        json!({"path": "a/README.md", "lines": header}),
        json!({"path": "src/x.rs", "lines": ["3\tfn octocode()"]}),
        json!({"path": "b/README.md", "lines": header}),
        json!({"path": "c/README.md", "lines": header, "matchCount": 40}),
        json!({"path": "d/README.md", "lines": header}),
        json!({"path": "p1"}),
        json!({"path": "p2"}),
    ];
    merge_identical(&mut items);
    assert_eq!(
        items,
        vec![
            json!({"path": "a/README.md", "alsoAt": ["b/README.md", "d/README.md"], "lines": header}),
            json!({"path": "src/x.rs", "lines": ["3\tfn octocode()"]}),
            json!({"path": "c/README.md", "lines": header, "matchCount": 40}),
            json!({"path": "p1"}),
            json!({"path": "p2"}),
        ]
    );
}

/// Evidence is every field but `path`, wherever `path` sits in the row and
/// in the row's order; a row without a `path` is never merged.
#[test]
fn identical_evidence_ignores_only_the_path_entry() {
    let lines = json!(["1\tfn main()"]);
    let mut items = vec![
        json!({"lines": lines, "path": "a.rs", "matchCount": 1}),
        json!({"lines": lines, "path": "b.rs", "matchCount": 1}),
        json!({"lines": lines, "matchCount": 1, "path": "c.rs"}),
        json!({"matchCount": 1, "lines": lines, "path": "d.rs"}),
        json!({"lines": lines, "matchCount": 1}),
        json!({"lines": lines, "matchCount": 1}),
    ];
    merge_identical(&mut items);
    assert_eq!(
        items,
        vec![
            json!({"path": "a.rs", "alsoAt": ["b.rs", "c.rs"], "lines": lines, "matchCount": 1}),
            json!({"matchCount": 1, "lines": lines, "path": "d.rs"}),
            json!({"lines": lines, "matchCount": 1}),
            json!({"lines": lines, "matchCount": 1}),
        ]
    );
}
