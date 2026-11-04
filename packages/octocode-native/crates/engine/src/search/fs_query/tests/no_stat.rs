//! What a walk reads per entry: what it already holds is not read twice.

use super::*;

fn stats() -> usize {
    super::super::filters::ENTRY_STATS.with(std::cell::Cell::get)
}

fn levels_stated() -> usize {
    crate::search::walk::LEVELS_STATED.with(std::cell::Cell::get)
}

fn tree(root: &Path, dirs: usize) {
    for dir in 0..dirs {
        let dir = root.join(format!("d{dir:03}"));
        fs::create_dir(&dir).expect("dir");
        for file in 0..3 {
            fs::write(dir.join(format!("f{file}.rs")), "x").expect("file");
        }
    }
}

#[test]
fn a_names_only_walk_stats_no_entry_and_still_knows_each_kind() {
    let root = TmpDir::new();
    tree(&root, 4);
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("d000"), root.join("link")).expect("link");
    let before = stats();
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        names_only: true,
        ..Default::default()
    })
    .expect("query");
    assert_eq!(stats(), before, "a names-only walk stats no entry");
    assert_eq!(found.entries.len(), if cfg!(unix) { 17 } else { 16 });
    assert!(found.entries.iter().all(|entry| entry.size.is_none()
        && entry.modified_time.is_none()
        && entry.stamp.is_none()));
    let kind = |name: &str| {
        found
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.entry_type)
    };
    assert_eq!(kind("d000"), Some(FileKind::Directory));
    assert_eq!(kind("f0.rs"), Some(FileKind::File));
    #[cfg(unix)]
    assert_eq!(kind("link"), Some(FileKind::Symlink));
}

/// A filter that reads metadata needs the stat a names-only walk leaves out.
#[test]
fn a_metadata_filter_still_stats_a_names_only_walk() {
    let root = TmpDir::new();
    fs::write(root.join("small"), "x").expect("small");
    fs::write(root.join("large"), "x".repeat(100)).expect("large");
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        names_only: true,
        size_min: Some(50),
        ..Default::default()
    })
    .expect("query");
    let names = found
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["large"]);
}

/// The stamp an entry and a directory carry is the one a stat would give.
#[test]
fn a_walk_carries_the_stamps_it_read_and_each_directorys_rule_files() {
    let root = TmpDir::new();
    fs::create_dir(root.join("sub")).expect("sub");
    fs::write(root.join("sub/a.rs"), "x").expect("file");
    fs::write(root.join("sub/.gitignore"), "z\n").expect("rules");
    fs::create_dir(root.join("sub/.git")).expect("repository");
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        ignore: IgnorePolicy::new(Some(true), Some(false)),
        ..Default::default()
    })
    .expect("query");
    let stamp_of = |path: &Path| {
        crate::stamp::SourceStamp::of(&fs::symlink_metadata(path).expect("stat")).expect("stamp")
    };
    let entry = found
        .entries
        .iter()
        .find(|e| e.name == "a.rs")
        .expect("a.rs");
    assert_eq!(entry.stamp, Some(stamp_of(Path::new(&entry.path))));
    let sub = root.join("sub");
    let dirs = found
        .walked_dirs
        .iter()
        .map(|dir| dir.path.clone())
        .collect::<Vec<_>>();
    assert!(
        dirs.contains(&root.to_path_buf()) && dirs.contains(&sub),
        "{dirs:?}"
    );
    let walked = found
        .walked_dirs
        .iter()
        .find(|dir| dir.path == sub)
        .expect("sub");
    assert_eq!(walked.stamp, Some(stamp_of(&sub)));
    assert_eq!(walked.ignore_files, [(sub.join(".gitignore"), true)]);
    assert!(walked.holds_git);
    let top = found
        .walked_dirs
        .iter()
        .find(|dir| dir.path == *root)
        .expect("root");
    assert!(top.ignore_files.is_empty() && !top.holds_git);
}

/// A directory's listing shows whether it holds a rule file: the rules of a
/// directory holding none cost no stat, however many directories the walk
/// enters, and rules a listing shows still apply.
#[test]
fn ignore_rules_of_a_walked_directory_come_from_its_listing() {
    let count = |dirs| {
        let root = TmpDir::new();
        tree(&root, dirs);
        fs::write(root.join("d000/.ignore"), "f1.rs\n").expect("rules");
        let before = levels_stated();
        let found = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            ..Default::default()
        })
        .expect("query");
        let listed = found
            .entries
            .iter()
            .filter(|e| e.relative_path == "d000/f1.rs")
            .count();
        assert_eq!(listed, 0, "the rule its listing shows applies");
        let other = found
            .entries
            .iter()
            .filter(|e| e.relative_path == "d001/f1.rs")
            .count();
        assert_eq!(other, 1);
        levels_stated() - before
    };
    let (few, many) = (count(4), count(40));
    assert_eq!(
        few, many,
        "rule stats do not grow with the directories walked"
    );
}

/// A listing that could not read every entry proves nothing about what it
/// lacks: those rules still load by stat, and a `.git` file (a worktree) or a
/// linked rule file resolves as before.
#[cfg(unix)]
#[test]
fn a_linked_rule_file_and_a_git_marker_still_decide() {
    let root = TmpDir::new();
    fs::create_dir(root.join(".git")).expect("repository");
    fs::create_dir(root.join("rules")).expect("rules dir");
    fs::write(root.join("rules/source"), "a.rs\n").expect("rules");
    fs::create_dir(root.join("sub")).expect("sub");
    std::os::unix::fs::symlink(root.join("rules/source"), root.join("sub/.gitignore"))
        .expect("linked rules");
    fs::write(root.join("sub/a.rs"), "x").expect("file");
    fs::write(root.join("sub/b.rs"), "x").expect("file");
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        extensions: Some(vec!["rs".to_owned()]),
        ..Default::default()
    })
    .expect("query");
    let names = found
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["b.rs"]);
}

fn emptiness_reads() -> usize {
    super::super::filters::EMPTINESS_READS.with(std::cell::Cell::get)
}

fn empty_names(root: &Path, max_depth: Option<u32>) -> Vec<String> {
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        empty: Some(true),
        max_depth,
        ignore: IgnorePolicy {
            include_ignored: true,
            include_hidden: true,
        },
        ..Default::default()
    })
    .expect("query");
    found
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect()
}

/// `empty:true` tells a directory it enters from the listing it enters it by.
/// Only one the walk stops above (its depth bound) needs a read of its own.
#[test]
fn an_empty_filter_reads_only_directories_the_walk_does_not_enter() {
    let root = TmpDir::new();
    tree(&root, 20);
    fs::create_dir_all(root.join("hollow/inner")).expect("nested empty");
    let before = emptiness_reads();
    assert_eq!(empty_names(&root, None), ["hollow/inner"]);
    assert_eq!(emptiness_reads(), before, "every directory was entered");
    let before = emptiness_reads();
    assert_eq!(empty_names(&root, Some(1)), Vec::<String>::new());
    assert_eq!(emptiness_reads(), before + 21, "one read per unentered dir");
}

/// What counts as empty does not depend on how the walk learned it: a dot
/// file or an empty child fills a directory, a link never is one, a file with
/// no bytes is empty, and a directory past the depth bound is judged alike.
#[test]
fn an_empty_filter_lists_the_same_entries_entered_or_not() {
    let root = TmpDir::new();
    fs::create_dir(root.join("bare")).expect("bare");
    fs::create_dir(root.join("dotted")).expect("dotted");
    fs::write(root.join("dotted/.hidden"), "").expect("dot file");
    fs::create_dir_all(root.join("parent/child")).expect("parent");
    fs::write(root.join("none.txt"), "").expect("no bytes");
    fs::write(root.join("some.txt"), "x").expect("bytes");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("bare"), root.join("link")).expect("link");
    let mut unbounded = empty_names(&root, None);
    unbounded.sort();
    assert_eq!(
        unbounded,
        ["bare", "dotted/.hidden", "none.txt", "parent/child"]
    );
    let mut bounded = empty_names(&root, Some(1));
    bounded.sort();
    assert_eq!(bounded, ["bare", "none.txt"]);
}

/// A directory that cannot be read is not empty, and is counted once.
#[cfg(unix)]
#[test]
fn an_empty_filter_counts_an_unreadable_directory_once() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    let locked = root.join("locked");
    fs::create_dir(&locked).expect("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");
        return;
    }
    let found = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().into_owned(),
        empty: Some(true),
        ..Default::default()
    })
    .expect("query");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");
    assert!(found.entries.is_empty(), "{:?}", found.entries);
    assert_eq!(found.permission_denied, 1, "counted once");
}
