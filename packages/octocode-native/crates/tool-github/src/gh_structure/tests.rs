use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn materialize_never_writes_a_provider_path_outside_its_root() {
    let base = tempfile::tempdir().expect("tempdir");
    let root = base.path().join("snapshot");
    std::fs::create_dir_all(&root).expect("root");
    let outside = base.path().join("escape.txt");
    let mut writer = MaterializeWriter {
        root: &root,
        home: base.path(),
        written: 0,
        cursor: 0,
        stopped: None,
        skips: MaterializeSkips::default(),
        recorded: Vec::new(),
    };
    let read = || {
        Acquired::Read(Ok(octocode_github::ContentResponse {
            bytes: b"owned".to_vec().into(),
            resolved_ref: "main".into(),
            raw_response_bytes: 5,
            blob: None,
        }))
    };
    for path in [
        "../escape.txt".to_owned(),
        outside.to_string_lossy().into_owned(),
        "a/../../escape.txt".to_owned(),
        "ok/kept.txt".to_owned(),
    ] {
        let entry = TreeEntry {
            path,
            kind: EntryKind::File,
            size: Some(5),
            oid: None,
        };
        assert!(writer.write(&entry, read()).await.expect("write"));
    }
    assert!(!outside.exists());
    assert_eq!(writer.written, 1);
    assert_eq!(
        std::fs::read(root.join("ok/kept.txt")).expect("kept"),
        b"owned"
    );
    assert_eq!(
        writer.skips.unreadable.len(),
        3,
        "{:?}",
        writer.skips.unreadable
    );
}

#[test]
fn materialize_skips_name_each_path_once_and_warn_with_counts() {
    let skips = MaterializeSkips {
        read_limit: 16 * 1024 * 1024,
        oversized: (0..30)
            .map(|n| (format!("big/{n}.bin"), 16 * 1024 * 1024))
            .collect(),
        unreadable: vec![
            ("a.dat".into(), "binary".into()),
            ("b.dat".into(), "binary".into()),
            ("c".into(), "unsupported entry".into()),
        ],
        binary: Vec::new(),
        withheld: Vec::new(),
    };
    let warnings = skips.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Skipped 30 file(s) larger than"),
        "{warnings:?}"
    );
    assert!(!warnings[0].contains("big/0.bin"), "{warnings:?}");
    assert_eq!(
        warnings[1],
        "Skipped 3 unreadable file(s) (location.skipped): binary; unsupported entry."
    );
    let paths = skips.paths();
    assert_eq!(paths.len(), 33);
    assert_eq!(paths[0], "big/0.bin");
    assert_eq!(paths[32], "c");
    assert!(MaterializeSkips::default().warnings().is_empty());
}

#[test]
fn materialization_page_boundary_and_pages_past_the_former_ceiling_continue() {
    let tree = |materialize: bool| -> GhStructureQueryTree {
        let mut row = json!({"operation":"tree",
            "owner": "a", "repo": "b", "ref": "a".repeat(40), "path": ""});
        if materialize {
            row["materialize"] = json!(true);
        }
        serde_json::from_value(row).expect("query")
    };
    let scope_of = |query: &GhStructureQueryTree| Scope::of(query).expect("scope");
    let query = tree(true);
    let mut value = json_fixture(
        r#"{
            "files": [{"dir": ".", "files": ["a.rs"]}],
            "location": {"path": "/t"}
        }"#,
    );
    let page = |current| ListingPage {
        current,
        per_page: 1,
        total_entries: current + 1,
        total_pages: current + 1,
        entries: &[],
    };
    let resume = Some(MaterializeResume { offset: 7 });
    attach_continuations(
        &mut value,
        &query,
        &scope_of(&query),
        &page(1),
        false,
        resume,
    )
    .expect("continuation");
    assert_eq!(value["location"], json_fixture(r#"{"path": "/t"}"#));
    assert_eq!(value["isPartial"], true);
    assert_eq!(
        value["partialReasons"],
        json_fixture(r#"["materializeIncomplete"]"#)
    );
    let resumed = &value["calls"]["continueMaterialize"]["input"]["queries"][0];
    assert!(resumed.get("page").is_none(), "{resumed}");
    assert_eq!(resumed["materializeOffset"], 7);
    assert_eq!(resumed["ref"], "a".repeat(40));
    assert_eq!(value["calls"]["nextPage"]["input"]["queries"][0]["page"], 2);
    for materialize in [false, true] {
        let query = tree(materialize);
        let scope = scope_of(&query);
        let mut value = json_fixture(r#"{"files": [{"dir": ".", "files": ["last.rs"]}]}"#);
        attach_continuations(&mut value, &query, &scope, &page(1000), false, None)
            .expect("continuation");
        assert!(value.get("terminalLimit").is_none(), "{value}");
        let next = &value["calls"]["nextPage"]["input"]["queries"][0];
        assert_eq!(next["page"], 1001);
        assert!(
            next.get("materialize").is_none(),
            "listing pages write nothing: {next}"
        );
        let read: GhStructureQueryTree = serde_json::from_value(next.clone()).expect("next page");
        assert_eq!(
            scope_of(&read).depth,
            scope.depth,
            "a page is cut from the same listing: {next}"
        );
        assert_eq!(value["files"][0]["files"][0], "last.rs");
    }
}

/// A depth-3 listing of the repository root of `a/b`.
fn test_listing() -> Listing<'static> {
    Listing {
        owner: "a",
        repo: "b",
        branch: "main",
        root: "",
        max_depth: 3,
    }
}

/// A submodule is not listed, and not dropped silently: the listing names
/// it by its repository-relative path; a symlink is listed as a link entry.
#[test]
fn a_submodule_is_named_not_silently_dropped() {
    let tree: octocode_github::TreeResponse = serde_json::from_value(json_fixture(
        r#"{
            "sha": "s",
            "truncated": false,
            "tree": [
                {"path": "src", "type": "tree"},
                {"path": "third_party/zlib", "type": "commit"},
                {
                    "path": "src/link",
                    "type": "blob",
                    "mode": "120000",
                    "size": 3
                },
                {"path": "README.md", "type": "blob", "size": 1}
            ]
        }"#,
    ))
    .expect("tree");
    let traversal = traverse::traversal_from_git_tree(tree, &test_listing());
    assert_eq!(traversal.entries.len(), 3);
    assert_eq!(
        serde_json::to_value(&traversal.omitted).expect("omitted"),
        json_fixture(r#"{"submodule": ["third_party/zlib"]}"#)
    );
}

/// Globs relative to the listed path select entries and omitted paths
/// consistently; what they keep stays repo-relative.
#[test]
fn include_globs_select_entries_and_omitted_paths_below_the_listed_path() {
    let query: GhStructureQueryTree = serde_json::from_value(json_fixture(
        r#"{
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "path": "pkg",
            "include": ["src/*.rs"]
        }"#,
    ))
    .expect("query");
    let scope = Scope::of(&query).expect("scope");
    let entry = |path: &str| TreeEntry {
        path: path.into(),
        kind: EntryKind::File,
        size: Some(1),
        oid: None,
    };
    let mut traversal = Traversal {
        entries: vec![
            entry("pkg/src/lib.rs"),
            entry("pkg/src/nested/a.rs"),
            entry("pkg/lib.rs"),
        ],
        omitted: [(
            "submodule".to_owned(),
            ["pkg/src/vendor.rs".to_owned(), "pkg/vendor".to_owned()].into(),
        )]
        .into(),
        ..Traversal::default()
    };
    scope.select(&mut traversal);
    let kept = traversal
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kept, ["pkg/src/lib.rs"]);
    assert_eq!(
        serde_json::to_value(&traversal.omitted).expect("omitted"),
        json_fixture(r#"{"submodule": ["pkg/src/vendor.rs"]}"#)
    );
}

/// A git tree types a symlink `blob` of mode `120000`: it is named
/// listed as a link entry, never as a file.
#[test]
fn a_git_tree_symlink_is_listed_as_a_link() {
    let tree: octocode_github::TreeResponse = serde_json::from_value(json_fixture(
        r#"{
            "sha": "s",
            "truncated": false,
            "tree": [
                {
                    "path": "README.md",
                    "type": "blob",
                    "mode": "100644",
                    "size": 1
                },
                {
                    "path": "docs",
                    "type": "blob",
                    "mode": "120000",
                    "size": 9
                }
            ]
        }"#,
    ))
    .expect("tree");
    let traversal = traverse::traversal_from_git_tree(tree, &test_listing());
    let listed = traversal
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(listed, ["README.md", "docs"]);
    assert_eq!(traversal.entries[1].kind, EntryKind::Link);
    assert!(traversal.omitted.is_empty());
    let targets = LinkTargets::from([("docs".to_owned(), "README.md".to_owned())]);
    assert_eq!(
        build_structure(&traversal.entries, &targets),
        vec![json_fixture(
            r#"{
                "dir": ".",
                "files": ["README.md (1)", "docs (link -> README.md)"]
            }"#
        )]
    );
    assert_eq!(summary(&traversal.entries)["fileCount"], 2);
}

/// A Contents listing types a submodule `file` with size 0; its git
/// URL names another repository's tree.
#[test]
fn a_submodule_typed_file_is_not_a_zero_byte_file() {
    let entries: Vec<ContentsEntry> = serde_json::from_value(json_fixture(
        r#"[
            {
                "name": "book",
                "path": "src/doc/book",
                "type": "file",
                "size": 0,
                "git_url": "https://api.github.com/repos/rust-lang/book/git/trees/abc"
            },
            {
                "name": "empty.txt",
                "path": "src/doc/empty.txt",
                "type": "file",
                "size": 0,
                "git_url": "https://api.github.com/repos/rust-lang/rust/git/blobs/def"
            }
        ]"#,
    ))
    .expect("entries");
    assert!(entry_kind(&entries[0]).is_none());
    assert_eq!(entry_kind(&entries[1]), Some(EntryKind::File));
}

#[test]
fn lists_symlinks_and_skips_submodules() {
    let entry = |kind: &str| ContentsEntry {
        name: "x".into(),
        path: "x".into(),
        kind: kind.into(),
        size: None,
        sha: None,
        git_url: None,
    };
    assert_eq!(entry_kind(&entry("symlink")), Some(EntryKind::Link));
    assert!(entry_kind(&entry("submodule")).is_none());
}
/// The `dir` of a row is repo-relative, the same base as `path`.
#[test]
fn structure_dirs_are_repo_relative() {
    let rows = build_structure(
        &[
            TreeEntry {
                path: "src/lib.rs".into(),
                kind: EntryKind::File,
                size: Some(1),
                oid: None,
            },
            TreeEntry {
                path: "src/nested".into(),
                kind: EntryKind::Dir,
                size: None,
                oid: None,
            },
            TreeEntry {
                path: "src/nested/a.rs".into(),
                kind: EntryKind::File,
                size: Some(1),
                oid: None,
            },
        ],
        &LinkTargets::new(),
    );
    assert_eq!(
        rows,
        vec![
            json_fixture(r#"{"dir": "src", "files": ["lib.rs (1)", "nested/"]}"#),
            json!("src/nested/a.rs (1)"),
        ]
    );
    let root = build_structure(
        &[TreeEntry {
            path: "README.md".into(),
            kind: EntryKind::File,
            size: Some(1),
            oid: None,
        }],
        &LinkTargets::new(),
    );
    assert_eq!(root, vec![json!("README.md (1)")]);
}

/// Every entry a page's `files` rows list, as its repo-relative path with
/// its fields (`"<path> (<fields>)"`, a folder `"<path>/"`), in order: a
/// flat row is that already, a `{dir, files}` group names each entry under
/// its `dir`. What a reader rebuilds from either form.
fn listed_paths(data: &Value) -> Vec<String> {
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| match row {
            Value::String(path) => vec![path.clone()],
            group => {
                let dir = group["dir"].as_str().expect("a group's dir");
                let entries = group["files"].as_array().expect("a group's files");
                assert!(entries.len() > 1, "a group of one entry: {group}");
                entries
                    .iter()
                    .map(|entry| {
                        octocode_runtime_base::response::groups::path_of(
                            dir,
                            entry.as_str().expect("entry"),
                        )
                    })
                    .collect()
            }
        })
        .collect()
}

fn file(path: &str, size: u64) -> TreeEntry {
    TreeEntry {
        path: path.into(),
        kind: EntryKind::File,
        size: Some(size),
        oid: None,
    }
}

/// One file in each of several directories is flat rows: no row is a group,
/// each row is the file's repo-relative path with its size, and the listing
/// is shorter than its grouped form by every group header it does not write.
#[test]
fn one_entry_per_directory_lists_flat() {
    let entries = (0..6)
        .map(|n| file(&format!("packages/p{n}/package.json"), 10 + n))
        .chain([file("package.json", 3)])
        .collect::<Vec<_>>();
    let rows = build_structure(&entries, &LinkTargets::new());
    assert_eq!(rows.len(), entries.len());
    assert!(rows.iter().all(Value::is_string), "{rows:?}");
    let mut expected = entries
        .iter()
        .map(|entry| format!("{} ({})", entry.path, entry.size.expect("size")))
        .collect::<Vec<_>>();
    // The root leads, as in a grouped listing.
    expected.rotate_right(1);
    assert_eq!(listed_paths(&json!({ "files": rows })), expected);
    let grouped = entries
        .iter()
        .map(|entry| {
            let (dir, name) = octocode_runtime_base::response::groups::split(&entry.path);
            octocode_runtime_base::response::groups::header_chars(dir) + name.len()
        })
        .sum::<usize>();
    let flat = octocode_engine::json_size::utf16_units(&rows).expect("size");
    assert!(flat < grouped, "{flat} chars flat, {grouped} grouped");
}

/// A directory with several entries on the page stays one group beside the
/// flat rows of its neighbors, in the listing's order; a lone folder and a
/// lone symlink are flat like a lone file.
#[test]
fn a_directory_with_several_entries_stays_grouped_among_flat_rows() {
    let entries = [
        file("a/one.rs", 1),
        file("b/x.rs", 2),
        file("b/y.rs", 3),
        TreeEntry {
            path: "c/sub".into(),
            kind: EntryKind::Dir,
            size: None,
            oid: None,
        },
        TreeEntry {
            path: "d/link".into(),
            kind: EntryKind::Link,
            size: None,
            oid: None,
        },
    ];
    let targets = LinkTargets::from([("d/link".to_owned(), "a/one.rs".to_owned())]);
    let expected = json!([
        "a/one.rs (1)",
        {"dir": "b", "files": ["x.rs (2)", "y.rs (3)"]},
        "c/sub/",
        "d/link (link -> a/one.rs)",
    ]);
    assert_eq!(json!(build_structure(&entries, &targets)), expected);
    // A rank lists runs: a run of one is flat, a longer run a group.
    assert_eq!(json!(ranked_structure(&entries, &targets)), expected);
}

mod provider_tests;

mod backed;

/// The one include rule (`include_globs::tests::RULE`), read below the
/// listed `path` and below the repository root alike: the kept entries stay
/// repo-relative.
#[test]
fn include_globs_read_paths_below_the_listed_path_by_the_one_rule() {
    for base in ["", "pkg/sub"] {
        for (glob, below, kept) in crate::include_globs::tests::RULE {
            let mut row = serde_json::json!({
                "operation": "tree", "owner": "a", "repo": "b", "include": [glob]
            });
            if !base.is_empty() {
                row["path"] = base.into();
            }
            let query: GhStructureQueryTree = serde_json::from_value(row).expect("query");
            let scope = Scope::of(&query).expect("scope");
            let path = if base.is_empty() {
                (*below).to_owned()
            } else {
                format!("{base}/{below}")
            };
            let mut traversal = Traversal {
                entries: vec![TreeEntry {
                    path: path.clone(),
                    kind: EntryKind::File,
                    size: Some(1),
                    oid: None,
                }],
                omitted: [("submodule".to_owned(), [path.clone()].into())].into(),
                ..Traversal::default()
            };
            scope.select(&mut traversal);
            let listed = traversal
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>();
            let expected = if *kept {
                vec![path.as_str()]
            } else {
                Vec::new()
            };
            assert_eq!(listed, expected, "{glob} below {base:?}");
            // An omitted path is named by the same rule.
            assert_eq!(
                traversal.omitted.is_empty(),
                !*kept,
                "{glob} below {base:?}: omitted"
            );
        }
    }
}

/// A walk that missed entries counts what stays unread, in one short warning
/// each; the retry is the row's `next`, never restated.
#[test]
fn a_walk_that_missed_entries_counts_them() {
    let traversal = Traversal {
        failed_subtrees: 2,
        unchecked: 3,
        ..Traversal::default()
    };
    let query: GhStructureQueryTree = serde_json::from_value(json_fixture(
        r#"{"operation": "tree", "owner": "a", "repo": "b"}"#,
    ))
    .expect("tree query");
    let page = ListingPage::of(&query, &traversal.entries);
    let mut value = json!({});
    disclose_limits(&traversal, &page, &mut value);
    assert_eq!(
        value["warnings"],
        json_fixture(
            r#"[
                "2 folder(s) could not be read: their entries are missing.",
                "3 file(s) left out: their folder's git tree did not answer."
            ]"#
        ),
        "{value}"
    );
    assert_eq!(
        value["partialReasons"],
        json!(["partialTreeFailures"]),
        "{value}"
    );
}
