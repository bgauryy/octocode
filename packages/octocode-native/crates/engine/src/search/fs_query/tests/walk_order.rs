//! Walk order, cancellation, and directories read ahead.

use super::super::read_ahead::{self, ReadAhead};
use super::*;

#[test]
fn deep_recursion_terminates_and_finds_leaf() {
    // Recursion past several levels must complete (and stay bounded by the
    // depth ceiling) rather than risk a stack overflow.
    let root = TmpDir::new();
    let mut p = root.0.clone();
    for i in 0..12 {
        p = p.join(format!("d{i}"));
    }
    fs::create_dir_all(&p).expect("deep dirs");
    File::create(p.join("leaf.ts")).expect("leaf");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        names: Some(vec!["leaf.ts".to_owned()]),
        recursive: Some(true),
        ..Default::default()
    })
    .expect("query");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].name, "leaf.ts");
}

#[test]
fn walk_children_yields_deterministic_sorted_order() {
    // `fs::read_dir` yields entries in raw OS order; the walk must sort
    // them so output is deterministic across runs and sorted within a dir.
    let root = TmpDir::new();
    for name in ["z.txt", "a.txt", "m.txt", "b.txt"] {
        File::create(root.join(name)).expect("create root file");
    }
    fs::create_dir_all(root.join("sub")).expect("create sub");
    for name in ["y.txt", "c.txt"] {
        File::create(root.join("sub").join(name)).expect("create sub file");
    }
    let run = || {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            recursive: Some(true),
            entry_type: Some(EntryType::File),
            ..Default::default()
        })
        .expect("query")
        .entries
        .into_iter()
        .map(|entry| entry.relative_path)
        .collect::<Vec<_>>()
    };
    let first = run();
    for _ in 0..5 {
        assert_eq!(run(), first, "fs query order must be deterministic");
    }
    // Files directly under the root are emitted in sorted order.
    let root_files: Vec<&str> = first
        .iter()
        .filter(|p| !p.contains('/') && !p.contains('\\'))
        .map(String::as_str)
        .collect();
    let mut sorted = root_files.clone();
    sorted.sort_unstable();
    assert_eq!(root_files, sorted);
}

#[test]
fn a_stopped_walk_is_cancelled() {
    let root = TmpDir::new();
    File::create(root.join("a.ts")).expect("a");
    let error = query_file_system(
        FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            ..Default::default()
        },
        &|_, _, _| true,
        &|| true,
    )
    .expect_err("stopped");
    assert_eq!(*error.kind(), ErrorKind::Cancelled);
}

/// A walk past [`read_ahead::START_AFTER_DIRECTORIES`] directories reads the
/// next ones ahead: it still lists every entry once, in name order, with
/// consecutive walk positions, and nothing under a directory its policy
/// callback leaves out.
#[test]
fn a_walk_reading_ahead_lists_every_entry_once_in_name_order() {
    let root = TmpDir::new();
    let mut expected = Vec::new();
    for dir in 0..3 * read_ahead::START_AFTER_DIRECTORIES {
        let kept = if dir % 5 == 0 { "denied" } else { "d" };
        let dir = root.join(format!("{kept}{dir:02}"));
        for sub in ["a", "b"] {
            fs::create_dir_all(dir.join(sub)).expect("dirs");
            File::create(dir.join(sub).join("f.ts")).expect("file");
        }
    }
    let mut walk = vec![root.0.clone()];
    while let Some(dir) = walk.pop() {
        let mut children = fs::read_dir(&dir)
            .expect("dir")
            .map(|entry| entry.expect("entry").path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children.iter().rev() {
            if child.is_dir() && !display(child).starts_with("denied") {
                walk.push(child.clone());
            }
        }
        expected.extend(
            children
                .iter()
                .filter(|child| !display(child).starts_with("denied"))
                .map(|child| child.strip_prefix(&root.0).expect("below").to_path_buf()),
        );
    }
    expected.sort();
    let result = query_file_system(
        FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            recursive: Some(true),
            ..Default::default()
        },
        &|path, _, _| !display(path).starts_with("denied"),
        &|| false,
    )
    .expect("query");
    let listed = result
        .entries
        .iter()
        .map(|entry| PathBuf::from(&entry.relative_path))
        .collect::<Vec<_>>();
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(listed, sorted, "walk order is name order");
    assert_eq!(listed, expected);
    let positions = result
        .entries
        .iter()
        .map(|entry| entry.walk_position)
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{positions:?}"
    );
}

fn display(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// A directory read ahead is taken once, read as the walk reads it; one the
/// walk left out is dropped and never taken.
#[test]
fn a_listing_read_ahead_is_taken_once_or_dropped() {
    let root = TmpDir::new();
    for dir in ["a", "b"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
        File::create(root.join(dir).join("f.ts")).expect("file");
    }
    let mut ahead = ReadAhead::start().expect("workers");
    ahead.queue([root.join("a"), root.join("b")].into_iter());
    let taken = ahead.take(&root.join("a")).expect("read ahead");
    let (children, errors) = taken.entries.expect("listing");
    assert!(errors.is_empty());
    assert_eq!(
        children
            .iter()
            .map(|child| child.name.clone())
            .collect::<Vec<_>>(),
        ["f.ts"]
    );
    assert!(taken.read_at.is_some());
    assert!(ahead.take(&root.join("a")).is_none(), "taken once");
    ahead.drop_unused(&root.join("b"));
    assert!(ahead.take(&root.join("b")).is_none(), "dropped");
}
