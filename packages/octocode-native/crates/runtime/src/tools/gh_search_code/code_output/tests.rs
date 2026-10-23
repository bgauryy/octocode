//! Row shaping: hoisted shared fields, repo-scoped echoes, identical files listed once.
use super::super::leads::unsearched_ref_read;
use super::*;

/// An owner-wide page whose rows share a repository and commit names
/// them once on the row; a field the rows disagree on stays per row.
#[test]
fn owner_wide_rows_hoist_shared_repository_and_commit() {
    let mut value = json!({});
    let mut items = vec![
        json!({"owner":"o","repo":"r","commitSha":"c1","path":"a.rs"}),
        json!({"owner":"o","repo":"r","commitSha":"c1","path":"b.rs"}),
    ];
    hoist_shared(&mut value, &mut items);
    assert_eq!(value, json!({"owner":"o","repo":"r","commitSha":"c1"}));
    assert_eq!(items, vec![json!({"path":"a.rs"}), json!({"path":"b.rs"})]);
    let mut value = json!({});
    let mut items = vec![
        json!({"owner":"o","repo":"r","commitSha":"c1","path":"a.rs"}),
        json!({"owner":"o","repo":"s","commitSha":"c2","path":"b.rs"}),
        json!({"owner":"o","repo":"s","path":"c.rs"}),
    ];
    hoist_shared(&mut value, &mut items);
    assert_eq!(value, json!({"owner":"o"}));
    assert_eq!(items[0], json!({"repo":"r","commitSha":"c1","path":"a.rs"}));
    assert_eq!(items[2], json!({"repo":"s","path":"c.rs"}));
}

/// A repo-scoped page names no owner/repo: they are the query's, and the
/// shared minimizer would drop the echo.
#[test]
fn repo_scoped_pages_carry_no_owner_repo_echo() {
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "mainGoal":"test","reasoning":"test","owner":"o","repo":"r","keywords":["x"],"ref":"dev"
    }))
    .expect("query");
    let mut value = json!({});
    let mut items = vec![json!({"owner":"o","repo":"r","path":"src/a.rs","matches":[]})];
    shape_files(&mut value, &mut items, &query, None);
    assert!(value.get("owner").is_none(), "{value}");
    assert!(value.get("repo").is_none(), "{value}");
    // The index is always the default branch (the description says
    // so): no constant `indexRef` echo.
    assert!(value.get("indexRef").is_none(), "{value}");
    assert!(items[0].get("owner").is_none(), "{:?}", items[0]);
    // The empty row's one fact is the ref the index did not cover; a
    // named file at that ref is read there.
    assert_eq!(
        empty_hints(&query),
        ["Code search indexes only the default branch; `dev` was not searched."]
    );
    assert!(unsearched_ref_read(&query).is_none());
    let mut named = query.clone();
    named.filename = Some("Cargo.toml".parse().expect("filename"));
    named.path = Some("crates/x".parse().expect("path"));
    let read = unsearched_ref_read(&named).expect("read at the ref");
    assert_eq!(read["tool"], "ghGetFileContent", "{read}");
    assert_eq!(
        read["query"]["queries"][0]["path"], "crates/x/Cargo.toml",
        "{read}"
    );
    assert_eq!(read["query"]["queries"][0]["ref"], "dev", "{read}");
    let plain: GhSearchCodeQuery = serde_json::from_value(json!({
        "mainGoal":"test","reasoning":"test","owner":"o","keywords":["x"]
    }))
    .expect("query");
    assert!(empty_hints(&plain).is_empty());
    // A requested ref plus a path scope are two hints, never one composed
    // sentence.
    let mut both = query.clone();
    both.path = Some("crates/x".parse().expect("path"));
    let hints = empty_hints(&both);
    assert_eq!(hints.len(), 2, "{hints:?}");
    assert!(
        hints[0].starts_with("Code search indexes only"),
        "{hints:?}"
    );
    assert!(hints[1].starts_with("No indexed match under"), "{hints:?}");
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
