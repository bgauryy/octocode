//! Option validation, error kinds, and ignore rules.

use super::super::compiled::parse_duration;
use super::*;

/// A duration the walk cannot apply fails the query like a bad size:
/// returning the unfiltered listing would answer another question.
#[test]
fn an_unusable_duration_fails_the_query_instead_of_listing_unfiltered() {
    let root = TmpDir::new();
    File::create(root.join("f.ts")).expect("file");
    for (raw, set) in [("99999999999999999999d", 0), ("7x", 1), ("", 2)] {
        let mut options = FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            ..Default::default()
        };
        let field = [
            &mut options.modified_within,
            &mut options.modified_before,
            &mut options.accessed_within,
        ];
        *field.into_iter().nth(set).expect("field") = Some(raw.to_owned());
        let error =
            query_file_system(options, &|_, _, _| true, &|| false).expect_err("unusable duration");
        assert_eq!(error.kind(), &ErrorKind::InvalidInput, "{raw}");
        assert!(
            error.reason.contains(&format!("\"{raw}\"")),
            "{}",
            error.reason
        );
    }
}

#[test]
fn parse_duration_handles_units_and_rejects_garbage() {
    assert_eq!(parse_duration("7d"), Some(7 * 24 * 60 * 60));
    assert_eq!(parse_duration("30m"), Some(30 * 60));
    assert_eq!(parse_duration("2h"), Some(2 * 60 * 60));
    assert_eq!(parse_duration("1w"), Some(7 * 24 * 60 * 60));
    assert_eq!(parse_duration("7"), None); // no unit
    assert_eq!(parse_duration("d"), None); // no number
    assert_eq!(parse_duration("30min"), None); // multi-char unit unsupported
    assert_eq!(parse_duration(""), None);
    // Regression: a multibyte unit must return None, not panic on a non-char
    // boundary split.
    assert_eq!(parse_duration("7€"), None);
    // Regression: an absurd-but-valid numeric must not silently overflow u64
    // (release builds have no overflow checks). It must return None via the
    // invalid-duration path.
    let huge = u64::MAX.to_string();
    assert_eq!(parse_duration(&format!("{huge}m")), None);
    assert_eq!(parse_duration(&format!("{huge}h")), None);
    assert_eq!(parse_duration(&format!("{huge}d")), None);
    assert_eq!(parse_duration(&format!("{huge}w")), None);
}

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
            size_greater: Some("lots".to_owned()),
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
