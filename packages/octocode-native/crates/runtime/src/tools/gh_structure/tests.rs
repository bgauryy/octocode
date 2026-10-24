use super::*;

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
        total_bytes: 0,
        cursor: 0,
        skips: MaterializeSkips::default(),
        recorded: Vec::new(),
    };
    let read = || {
        Acquired::Read(Ok(octocode_github::ContentResponse {
            bytes: b"owned".to_vec().into(),
            resolved_ref: "main".into(),
            etag: None,
            from_cache: false,
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
        oversized: (0..30).map(|n| format!("big/{n}.bin")).collect(),
        unreadable: vec![
            ("a.dat".into(), "binary".into()),
            ("b.dat".into(), "binary".into()),
            ("c".into(), "unsupported entry".into()),
        ],
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
fn materialization_page_boundary_continues_and_page_ceiling_is_explicit() {
    let query: GhStructureQueryTree = serde_json::from_value(json!({"operation":"tree",
        "owner": "a", "repo": "b", "ref": "a".repeat(40),
        "path": ""}))
    .expect("query");
    let mut value = json!({
        "files": [{"dir": ".", "files": ["a.rs"]}],
        "location": {"localPath": "/t"}
    });
    let page = |current| ListingPage {
        current,
        per_page: 1,
        total_entries: current + 1,
        total_pages: current + 1,
        entries: &[],
    };
    attach_continuations(&mut value, &query, &page(1), false, true, None).expect("continuation");
    assert_eq!(value["location"], json!({"localPath": "/t"}));
    assert_eq!(value["isPartial"], true);
    assert_eq!(value["partialReasons"], json!(["materializeIncomplete"]));
    assert_eq!(
        value["calls"]["continueMaterialize"]["input"]["queries"][0]["page"],
        2
    );
    assert_eq!(
        value["calls"]["continueMaterialize"]["input"]["queries"][0]["materializeOffset"],
        0
    );
    assert_eq!(
        value["calls"]["continueMaterialize"]["input"]["queries"][0]["ref"],
        "a".repeat(40)
    );
    for materialize in [false, true] {
        let mut value = json!({"files": [{"dir": ".", "files": ["last.rs"]}]});
        attach_continuations(&mut value, &query, &page(1000), false, materialize, None)
            .expect("terminal");
        assert_eq!(value["terminalLimit"], true);
        assert!(value.get("next").is_none());
        assert_eq!(value["files"][0]["files"][0], "last.rs");
    }
}

/// A recursive git-tree listing of a path the tree lacks is a
/// missing path (not an empty listing), so the viewTree recovery runs.
#[test]
fn a_root_missing_from_the_git_tree_is_not_found() {
    let tree: octocode_github::TreeResponse = serde_json::from_value(json!({
        "sha":"s","truncated":false,"tree":[
            {"path":"src","type":"tree"},{"path":"src/lib.rs","type":"blob"}
        ]
    }))
    .expect("tree");
    assert!(root_is_directory(&tree, "").is_ok());
    assert!(root_is_directory(&tree, "src").is_ok());
    let missing = root_is_directory(&tree, "no/such").expect_err("missing");
    assert_eq!(missing.kind, ProviderErrorKind::NotFound);
    assert_eq!(missing.reason, None);
    let file = root_is_directory(&tree, "src/lib.rs").expect_err("file");
    assert_eq!(file.kind, ProviderErrorKind::Validation);
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

/// A submodule is not listed, and not dropped silently: the listing
/// names it by its repository-relative path.
#[test]
fn a_submodule_is_named_not_silently_dropped() {
    let tree: octocode_github::TreeResponse = serde_json::from_value(json!({
        "sha":"s","truncated":false,"tree":[
            {"path":"src","type":"tree"},
            {"path":"third_party/zlib","type":"commit"},
            {"path":"README.md","type":"blob","size":1}
        ]
    }))
    .expect("tree");
    let traversal = traverse::traversal_from_git_tree(tree, &test_listing());
    assert_eq!(traversal.entries.len(), 2);
    assert_eq!(
        serde_json::to_value(&traversal.omitted).expect("omitted"),
        json!(["third_party/zlib (submodule)"])
    );
}

/// Repository-relative globs select entries and omitted paths consistently.
#[test]
fn include_globs_match_repository_relative_paths() {
    let query: GhStructureQueryTree = serde_json::from_value(json!({"operation":"tree",
        "owner":"a","repo":"b","path":"pkg","include":["pkg/src/*.rs"]
    }))
    .expect("query");
    let scope = Scope::of(&query).expect("scope");
    let entry = |path: &str| TreeEntry {
        path: path.into(),
        kind: EntryKind::File,
        size: Some(1),
    };
    let mut traversal = Traversal {
        entries: vec![
            entry("pkg/src/lib.rs"),
            entry("pkg/src/nested/a.rs"),
            entry("pkg/lib.rs"),
        ],
        omitted: [
            "pkg/src/link.rs (symlink)".to_owned(),
            "pkg/vendor (submodule)".to_owned(),
        ]
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
        json!(["pkg/src/link.rs (symlink)"])
    );
}

/// A git tree types a symlink `blob` of mode `120000`: it is named
/// beside the skipped entries, never listed or sized as a file, and a
/// listing rooted at it says it is a symlink and is repaired by a read
/// (which follows the link to its file).
#[test]
fn a_git_tree_symlink_is_named_not_listed_as_a_file() {
    let tree: octocode_github::TreeResponse = serde_json::from_value(json!({
        "sha":"s","truncated":false,"tree":[
            {"path":"README.md","type":"blob","mode":"100644","size":1},
            {"path":"docs","type":"blob","mode":"120000","size":9}
        ]
    }))
    .expect("tree");
    let traversal = traverse::traversal_from_git_tree(tree.clone(), &test_listing());
    let listed = traversal
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(listed, ["README.md"]);
    assert_eq!(
        serde_json::to_value(&traversal.omitted).expect("omitted"),
        json!(["docs (symlink)"])
    );
    let link = root_is_directory(&tree, "docs").expect_err("a symlink");
    assert_eq!(link.reason, Some(ProviderErrorReason::PathIsFile));
    assert_eq!(
        link.message.as_ref(),
        "Path \"docs\" is a symlink, not a directory"
    );
}

/// A Contents listing types a submodule `file` with size 0; its git
/// URL names another repository's tree.
#[test]
fn a_submodule_typed_file_is_not_a_zero_byte_file() {
    let entries: Vec<ContentsEntry> = serde_json::from_value(json!([
        {"name":"book","path":"src/doc/book","type":"file","size":0,
         "git_url":"https://api.github.com/repos/rust-lang/book/git/trees/abc"},
        {"name":"empty.txt","path":"src/doc/empty.txt","type":"file","size":0,
         "git_url":"https://api.github.com/repos/rust-lang/rust/git/blobs/def"}
    ]))
    .expect("entries");
    assert!(entry_kind(&entries[0]).is_none());
    assert_eq!(entry_kind(&entries[1]), Some(EntryKind::File));
}

#[test]
fn skips_symlinks_and_submodules() {
    for kind in ["symlink", "submodule"] {
        assert!(
            entry_kind(&ContentsEntry {
                name: "x".into(),
                path: "x".into(),
                kind: kind.into(),
                size: None,
                sha: None,
                git_url: None
            })
            .is_none()
        );
    }
}
/// The `dir` of a row is repo-relative, the same base as `path` and `include`.
#[test]
fn structure_dirs_are_repo_relative() {
    let rows = build_structure(
        &[
            TreeEntry {
                path: "src/lib.rs".into(),
                kind: EntryKind::File,
                size: Some(1),
            },
            TreeEntry {
                path: "src/nested".into(),
                kind: EntryKind::Dir,
                size: None,
            },
            TreeEntry {
                path: "src/nested/a.rs".into(),
                kind: EntryKind::File,
                size: Some(1),
            },
        ],
        &Default::default(),
    );
    assert_eq!(
        rows,
        vec![
            json!({"dir":"src","files":["lib.rs (1)","nested/"]}),
            json!({"dir":"src/nested","files":["a.rs (1)"]}),
        ]
    );
    let root = build_structure(
        &[TreeEntry {
            path: "README.md".into(),
            kind: EntryKind::File,
            size: Some(1),
        }],
        &Default::default(),
    );
    assert_eq!(root, vec![json!({"dir":".","files":["README.md (1)"]})]);
}

mod provider_tests;

mod backed;

#[test]
fn include_globs_preserve_literal_spelling_and_scope() {
    for (pattern, accepted, rejected) in [
        ("lib", "lib", vec!["library", "src/lib", "LIB"]),
        ("lib.rs", "lib.rs", vec!["lib.rs.bak", "src/lib.rs"]),
        (
            "src/*.rs",
            "src/lib.rs",
            vec!["src/deep/lib.rs", "SRC/lib.rs"],
        ),
        ("**/*.rs", "src/deep/lib.rs", vec!["src/deep/lib.ts"]),
        (" lib ", " lib ", vec!["lib"]),
        ("/lib", "/lib", vec!["lib"]),
    ] {
        let filter = PathFilter::new(pattern).expect("glob");
        assert!(filter.matches(accepted), "{pattern} -> {accepted}");
        for path in rejected {
            assert!(!filter.matches(path), "{pattern} widened to {path}");
        }
    }
}
