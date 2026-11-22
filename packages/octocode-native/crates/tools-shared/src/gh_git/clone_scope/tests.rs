use super::{CloneRecord, clone_scope, read_record, write_record};
use std::path::Path;

/// A linked-worktree checkout at `root` whose administration lives outside
/// its work tree, as a ghCloneRepo checkout's does, with `record` written.
fn checkout(root: &Path, admin: &Path, record: Option<&CloneRecord>) {
    std::fs::create_dir_all(root).expect("root");
    std::fs::create_dir_all(admin).expect("admin");
    std::fs::write(root.join(".git"), format!("gitdir: {}\n", admin.display())).expect("pointer");
    if let Some(record) = record {
        write_record(admin, record).expect("record");
    }
}

fn record(paths: Option<&[&str]>, root_files: bool) -> CloneRecord {
    CloneRecord {
        owner: "o".into(),
        repo: "r".into(),
        commit_sha: "6ecc29b9c8c84d08a7e860f7f69c22b113b480ea".into(),
        paths: paths.map(|paths| paths.iter().map(|path| (*path).to_owned()).collect()),
        root_files,
        history_depth: 1,
    }
}

#[test]
fn a_record_round_trips_and_an_unreadable_one_reads_as_none() {
    let temp = tempfile::tempdir().expect("temp");
    let written = record(Some(&["src"]), true);
    write_record(temp.path(), &written).expect("write");
    assert_eq!(read_record(temp.path()), Some(written));
    std::fs::write(temp.path().join(super::RECORD_FILE), "{not json").expect("garbage");
    assert_eq!(read_record(temp.path()), None);
}

/// Only a recorded sparse scope answers: a full checkout and a checkout
/// with no record (scope unknown) are never read as one.
#[test]
fn only_a_recorded_sparse_checkout_has_a_scope() {
    let temp = tempfile::tempdir().expect("temp");
    let sparse = temp.path().join("sparse");
    checkout(
        &sparse,
        &temp.path().join("a1"),
        Some(&record(Some(&["src"]), true)),
    );
    let full = temp.path().join("full");
    checkout(&full, &temp.path().join("a2"), Some(&record(None, true)));
    let unknown = temp.path().join("unknown");
    checkout(&unknown, &temp.path().join("a3"), None);

    let scope = clone_scope(&sparse.join("tests/integration_tests.rs")).expect("sparse scope");
    assert_eq!(scope.root(), sparse);
    assert_eq!(scope.paths(), ["src"]);
    assert_eq!(
        clone_scope(&sparse).map(|scope| scope.root().to_owned()),
        Some(sparse)
    );
    for path in [
        full.join("tests"),
        unknown.join("tests"),
        temp.path().join("x"),
    ] {
        assert_eq!(clone_scope(&path), None, "{}", path.display());
    }
}

/// A missing path is excluded exactly when the checkout could not hold it:
/// outside every scope path and every directory leading to one, in cone and
/// exact scopes alike (a missing name beside a cone scope can only be a
/// directory, which the checkout leaves out).
#[test]
fn a_missing_path_is_excluded_only_outside_the_scope() {
    let temp = tempfile::tempdir().expect("temp");
    for (root_files, scope, held, excluded) in [
        (
            true,
            &["src/nested", "other/"][..],
            &["src/nested/x.rs", "src/nested", "src", "other/a", ""][..],
            &["tests", "README.md", "src/lib.rs", "srcx/a", "src/nestedx"][..],
        ),
        (
            false,
            &["src/lib.rs"][..],
            &["src/lib.rs", "src", ""][..],
            &["README.md", "src/main.rs", "tests/a.rs"][..],
        ),
    ] {
        let root = temp.path().join(format!("c{root_files}"));
        checkout(
            &root,
            &temp.path().join(format!("a{root_files}")),
            Some(&record(Some(scope), root_files)),
        );
        let checkout_scope = clone_scope(&root).expect("scope");
        for path in held {
            assert!(!checkout_scope.excludes(&root.join(path)), "{path} is held");
        }
        for path in excluded {
            assert!(
                checkout_scope.excludes(&root.join(path)),
                "{path} is excluded"
            );
        }
        // A path outside the checkout is no question of its scope.
        assert!(!checkout_scope.excludes(&temp.path().join("elsewhere")));
    }
}

/// The re-clone checks the missing path out beside the scope, at the same
/// commit and depth, and is a runnable ghCloneRepo call.
#[test]
fn the_reclone_adds_the_missing_path_at_the_same_commit() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("c");
    let mut recorded = record(Some(&["src"]), true);
    recorded.history_depth = 3;
    checkout(&root, &temp.path().join("a"), Some(&recorded));
    let scope = clone_scope(&root).expect("scope");
    let call = scope
        .reclone(&root.join("tests/integration_tests.rs"))
        .expect("call");
    assert_eq!(call["tool"], "ghCloneRepo", "{call}");
    let row = &call["input"]["queries"][0];
    assert_eq!(row["owner"], recorded.owner.as_str(), "{call}");
    assert_eq!(row["repo"], recorded.repo.as_str(), "{call}");
    assert_eq!(row["ref"], recorded.commit_sha.as_str(), "{call}");
    assert_eq!(row["historyDepth"], 3, "{call}");
    assert_eq!(
        row["paths"],
        serde_json::json!(["src", "tests/integration_tests.rs"]),
        "{call}"
    );
    octocode_runtime_base::contracts::validate_saved("ghCloneRepo", call["input"].clone())
        .expect("runnable call");
    assert_eq!(scope.reclone(&root), None, "the root is no path to add");
    // Each note names every scope path.
    for note in [scope.outside_message("tests"), scope.coverage_warning()] {
        assert!(note.contains("src"), "{note}");
    }
}

/// The coverage note's verb agrees with what it lists: one path alone is
/// singular; several paths, or a path beside the root's files, are plural.
#[test]
fn the_coverage_warning_agrees_in_number_with_the_scope() {
    let temp = tempfile::tempdir().expect("temp");
    let note = |paths: &[&str], root_files: bool| {
        let root = temp.path().join(format!("c{}{root_files}", paths.len()));
        checkout(
            &root,
            &temp.path().join(format!("a{}{root_files}", paths.len())),
            Some(&record(Some(paths), root_files)),
        );
        clone_scope(&root).expect("scope").coverage_warning()
    };
    let one = note(&["lib/response.js"], false);
    assert!(one.contains("lib/response.js is on disk"), "{one}");
    for plural in [
        note(&["lib", "src"], false),
        note(&["lib/response.js"], true),
    ] {
        assert!(plural.contains(" are on disk"), "{plural}");
    }
}
