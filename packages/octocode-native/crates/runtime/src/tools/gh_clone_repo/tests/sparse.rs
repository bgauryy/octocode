//! Sparse checkouts: path types, single files, several paths, near misses and path validation.
use super::bench::*;
use super::*;

#[test]
fn canonical_sparse_paths_are_one_array_without_scalar_or_legacy_aliases() {
    let row = serde_json::json!({
        "owner": "fixture-owner", "repo": "fixture-repo", "paths": ["src", "docs"]
    });
    let decoded: GhCloneRepoQuery = serde_json::from_value(row.clone()).expect("canonical paths");
    assert_eq!(
        Sparse::of(&decoded).expect("sparse scope").paths(),
        ["src", "docs"]
    );
    for field in [
        serde_json::json!({"path": "src"}),
        serde_json::json!({"path": ["src"]}),
        serde_json::json!({"paths": "src"}),
        serde_json::json!({"paths": ["src\ndocs"]}),
    ] {
        let mut invalid = serde_json::json!({"owner": "fixture-owner", "repo": "fixture-repo"});
        invalid
            .as_object_mut()
            .expect("row")
            .extend(field.as_object().expect("fields").clone());
        assert!(serde_json::from_value::<GhCloneRepoQuery>(invalid).is_err());
    }
}

/// A runner whose `ls-tree` fails: the sparse scope cannot be resolved.
struct FailingLsTree(RewriteRunner);

impl GitRunner for FailingLsTree {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        if request.args.iter().any(|argument| argument == "ls-tree") {
            return Err(CloneError::new(ErrorCode::GitFailed, "ls-tree failed"));
        }
        self.0.run(request, control)
    }
    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.0.run_nul_records(request, control, record)
    }
}

/// A sparse path whose type `ls-tree` cannot resolve fails the clone with
/// that error instead of guessing cone mode, which would check out the
/// whole folder beside a requested file.
#[test]
fn an_unresolvable_sparse_path_type_fails_the_clone() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = FailingLsTree(fixture_runner(&fixture));
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("README.md"),
            ..query()
        },
        &context,
    )
    .expect_err("unresolved sparse scope");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert_eq!(error.message, "ls-tree failed");
}

#[test]
fn sparse_file_path_checks_out_only_that_file() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    for branch in [Some("main".to_owned()), Some(fixture.first_commit.clone())] {
        for (file, absent) in [
            ("README.md", ["src/lib.rs", "other/skip.txt"]),
            ("src/lib.rs", ["README.md", "src/nested/data.txt"]),
        ] {
            let clone = clone_pinned(
                &fixture,
                &GhCloneRepoQuery {
                    ref_: branch.as_deref().map(reference),
                    paths: one(file),
                    ..query()
                },
                &context,
            )
            .expect("sparse file clone");
            let local = Path::new(&clone.location.local_path);
            assert!(local.join(file).is_file(), "{file} missing ({branch:?})");
            for sibling in absent {
                assert!(
                    !local.join(sibling).exists(),
                    "{sibling} checked out beside {file} ({branch:?})"
                );
            }
        }
    }
}

/// Several sparse paths check out together; a file among them switches
/// to exact non-cone patterns so nothing else is included.
#[test]
fn multiple_sparse_paths_check_out_every_path() {
    let bench = Bench::new();
    let context = bench.context();
    let both = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            paths: many(&["src/nested", "other"]),
            ..query()
        },
        &context,
    )
    .expect("multi-directory sparse clone");
    let local = Path::new(&both.location.local_path);
    assert!(local.join("src/nested/data.txt").is_file());
    assert!(local.join("other/skip.txt").is_file());
    assert_eq!(
        both.location.requested_paths.as_deref(),
        Some(&["src/nested".to_owned(), "other".to_owned()][..])
    );
    let row = serde_json::to_value(&both).expect("serialize");
    assert_eq!(
        Path::new(
            crate::tools::test_support::checks::related_call(&row, "structureSearch", |_|true)["input"]["queries"][0]["path"]
                .as_str()
                .expect("path")
        ),
        local,
        "several paths explore the checkout root"
    );

    let reordered = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            paths: many(&["other", "src/nested"]),
            ..query()
        },
        &context,
    )
    .expect("same set, other order");
    assert!(
        reordered.location.cached,
        "request order does not split the cache"
    );

    let mixed = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            paths: many(&["README.md", "src/nested"]),
            ..query()
        },
        &context,
    )
    .expect("file plus directory sparse clone");
    let local = Path::new(&mixed.location.local_path);
    assert!(local.join("README.md").is_file());
    assert!(local.join("src/nested/data.txt").is_file());
    assert!(!local.join("src/lib.rs").exists());
    assert!(!local.join("other/skip.txt").exists());

    let missing = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            paths: many(&["src", "nope"]),
            ..query()
        },
        &context,
    )
    .expect_err("one missing path fails the clone");
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    assert!(missing.message.contains("\"nope\""), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "\"src\"");
}

/// A missing sparse path is recovered from the commit's own trees: a unique
/// near-miss sibling at the deepest existing directory gives the same clone
/// with the path corrected; otherwise that directory is listed, pinned to the
/// commit.
#[test]
fn a_missing_sparse_path_leads_to_its_near_miss_or_its_nearest_directory() {
    let bench = Bench::new();
    let context = bench.context();
    let commit = bench.fixture.first_commit.clone();
    let failed = |path: Vec<GhCloneRepoQueryPathsItem>| {
        let missing = clone_pinned(
            &bench.fixture,
            &GhCloneRepoQuery {
                paths: path,
                ..query()
            },
            &context,
        )
        .expect_err("missing sparse path");
        assert_eq!(missing.code, ErrorCode::PathNotFound, "{missing:?}");
        assert!(missing.hints.is_empty(), "{missing:?}");
        *missing.calls.expect("recovery lead")
    };

    // One near miss in the deepest existing directory: the clone, corrected.
    let next = failed(one("src/nested/dta.txt"));
    let lead = &next["repair"];
    assert_eq!(lead["tool"], "ghCloneRepo", "{next}");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["paths"][0], "src/nested/data.txt", "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");
    assert_eq!(row["owner"], "fixture-owner", "{next}");

    // Only the letter case differs: missing on every filesystem (a commit's
    // paths are case-sensitive), corrected to the commit's spelling.
    let next = failed(one("src/Nested"));
    let row = &next["repair"]["input"]["queries"][0];
    assert_eq!(row["paths"][0], "src/nested", "{next}");

    // A directory is a near miss too, and a list is corrected in place.
    let next = failed(many(&["other", "sr/nested"]));
    let row = &next["repair"]["input"]["queries"][0];
    assert_eq!(
        row["paths"],
        serde_json::json!(["other", "src/nested"]),
        "{next}"
    );

    // Nothing near: the deepest directory that exists, at the commit.
    let next = failed(one("src/nested/deep/er/x.rs"));
    let lead = &next["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{next}");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["path"], "src/nested", "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");
    assert!(next.get("repair").is_none(), "{next}");

    // At the root, the listing names no path.
    let next = failed(one("zzzzzz/a"));
    let row = &next["viewTree"]["input"]["queries"][0];
    assert!(row.get("path").is_none(), "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");

    // One uncorrectable path among several keeps the listing, not a clone.
    let next = failed(many(&["src/nested/dta.txt", "src/qqqqqq/z"]));
    assert_eq!(
        next["viewTree"]["input"]["queries"][0]["path"], "src",
        "{next}"
    );
}

/// The native sparse-path rule holds on its own, whatever the contract
/// pattern admits: no root, drive, backslash, `.`/`..` segment or blank.
#[test]
fn sparse_path_validation_rejects_traversal_absolute_and_backslash() {
    for path in [
        "../escape",
        "a/../b",
        "a/..",
        "./a",
        "a/./b",
        ".",
        "..",
        "/abs",
        "/",
        "C:/x",
        "c:x",
        "a\\b",
        "\\\\server\\share",
        "",
        "  ",
    ] {
        assert!(!is_sparse_path(path), "{path:?} must be rejected");
    }
    for path in [
        "src",
        "a/b",
        "src/",
        ".github/workflows",
        "a..b",
        "...",
        "docs/C:x",
    ] {
        assert!(is_sparse_path(path), "{path:?} must be accepted");
        // The contract's lookaround-free `path` pattern admits every path
        // the native rule accepts: no valid path is refused at the edge.
        assert!(
            !rejects_sparse_path(path),
            "{path:?} rejected by the contract"
        );
    }
    // The pattern alone already refuses traversal, roots and backslashes.
    for path in ["../escape", "a/../b", "./a", ".", "..", "/abs", "a\\b"] {
        assert!(
            rejects_sparse_path(path),
            "{path:?} admitted by the contract"
        );
    }
}
