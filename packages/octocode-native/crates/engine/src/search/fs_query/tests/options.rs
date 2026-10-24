//! Option validation, error kinds, and ignore rules.

use super::*;

#[test]
fn query_errors_carry_their_kind() {
    let kind = |options: FileSystemQueryOptions| {
        query_file_system(options, &|_, _, _| true, &|| false)
            .expect_err("rejected")
            .kind()
            .clone()
    };
    assert_eq!(
        kind(FileSystemQueryOptions {
            path: ".".to_owned(),
            regex: Some("(".to_owned()),
            ..Default::default()
        }),
        ErrorKind::InvalidPattern
    );
    assert_eq!(
        kind(FileSystemQueryOptions {
            path: ".".to_owned(),
            size_min: Some(6),
            size_max: Some(5),
            ..Default::default()
        }),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        kind(FileSystemQueryOptions {
            path: "/definitely/missing/octocode-root".to_owned(),
            ..Default::default()
        }),
        ErrorKind::NotFound
    );
    for (names, exclude) in [
        (Some(vec!["[.ts".to_owned()]), None),
        (None, Some(vec!["{a,b".to_owned()])),
    ] {
        assert_eq!(
            kind(FileSystemQueryOptions {
                path: ".".to_owned(),
                names,
                exclude,
                ..Default::default()
            }),
            ErrorKind::InvalidGlob
        );
    }
}

/// The walk leaves out what the ignore rules leave out, counts it, and
/// walks it with `includeIgnored`.
#[test]
fn ignore_rules_are_honored_counted_and_lifted() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join(".git")).expect("repository marker");
    fs::create_dir_all(root.join("gen")).expect("gen");
    fs::write(
        root.join(".gitignore"),
        "gen/
*.log
.cache
",
    )
    .expect("gitignore");
    for file in ["a.rs", "b.log", "gen/x.rs", ".cache"] {
        File::create(root.join(file)).expect("file");
    }
    let files = |ignore: IgnorePolicy| {
        let found = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            entry_type: Some(EntryType::File),
            ignore,
            ..Default::default()
        })
        .expect("query");
        let mut paths = found
            .entries
            .iter()
            .map(|entry| entry.relative_path.replace('\\', "/"))
            .collect::<Vec<_>>();
        paths.sort();
        (paths, found.ignored.len())
    };
    // A dot entry out of view is not counted as ignored.
    assert_eq!(files(IgnorePolicy::default()), (vec!["a.rs".to_owned()], 2));
    let shown = IgnorePolicy {
        include_ignored: false,
        include_hidden: true,
    };
    assert_eq!(
        files(shown),
        (vec![".gitignore".to_owned(), "a.rs".to_owned()], 3)
    );
    let (all, ignored) = files(IgnorePolicy::ALL);
    assert_eq!(all, [".cache", ".gitignore", "a.rs", "b.log", "gen/x.rs"]);
    assert_eq!(ignored, 0);
}

#[test]
fn rejects_min_depth_greater_than_max_depth() {
    let err = query_file_system_inner(FileSystemQueryOptions {
        path: ".".to_owned(),
        min_depth: Some(2),
        max_depth: Some(1),
        ..Default::default()
    })
    .expect_err("min_depth greater than max_depth must be rejected");

    assert!(err.contains("minDepth must be less than or equal to maxDepth"));
}
