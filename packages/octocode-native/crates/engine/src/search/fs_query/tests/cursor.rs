//! Resuming a walk from its cursor, and windows that end at a listed entry.

use super::*;

/// A tree whose walk is deep, with unlisted entries (`.md`, directories).
fn resume_tree() -> TmpDir {
    let root = TmpDir::new();
    for dir in ["a", "a/inner", "b", "c/deep/er"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
    }
    for file in [
        "a/1.ts",
        "a/2.md",
        "a/inner/3.ts",
        "b/4.ts",
        "c/deep/er/5.ts",
        "c/deep/6.md",
        "7.ts",
    ] {
        File::create(root.join(file)).expect("file");
    }
    root
}

/// A walk of `resume_tree` resumed at `skip`, with how many entries its
/// policy callback saw replayed and fresh.
fn resumed(
    root: &TmpDir,
    skip: u32,
    resume: Option<WalkCursor>,
    time_budget: Option<std::time::Duration>,
) -> (FileSystemQueryResult, usize, usize) {
    let replayed = std::sync::atomic::AtomicUsize::new(0);
    let fresh = std::sync::atomic::AtomicUsize::new(0);
    let result = query_file_system(
        FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            extensions: Some(vec!["ts".to_owned()]),
            skip_entries: Some(skip),
            resume,
            time_budget,
            window_rows: Some(2),
            ..Default::default()
        },
        &|path, _, visit| {
            // The root is judged once per walk, never an entry of it.
            if path == Path::new(&*root.to_string_lossy()) {
                return true;
            }
            let counter = if visit.fresh_at().is_some() {
                &fresh
            } else {
                &replayed
            };
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            true
        },
        &|| false,
    )
    .expect("query");
    (result, replayed.into_inner(), fresh.into_inner())
}

fn relative_paths(result: &FileSystemQueryResult) -> Vec<String> {
    result
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect()
}

/// A walk resumed from the cursor of the walk it resumes continues
/// there: nothing is replayed, and it lists and counts what a replaying
/// resume does, from every position a walk stops at.
#[test]
fn a_walk_resumed_from_its_cursor_replays_nothing_and_matches_a_replay() {
    let root = resume_tree();
    let (whole, _, _) = resumed(&root, 0, None, None);
    let visited = whole.walk_position;
    let mut replays_saved = 0;
    for skip in 0..visited {
        // A walk stopped by its time budget at its first new entry.
        let (cut, _, _) = resumed(&root, skip, None, Some(std::time::Duration::ZERO));
        let cursor = cut
            .cursor
            .clone()
            .expect("a walk cut by time keeps its cursor");
        assert_eq!(cursor.position(), skip, "skip {skip}");
        let (replay, replayed, _) = resumed(&root, skip, None, None);
        let (continued, none_replayed, fresh) = resumed(&root, skip, Some(cursor), None);
        assert_eq!(none_replayed, 0, "skip {skip}: nothing replayed");
        assert_eq!(
            relative_paths(&continued),
            relative_paths(&replay),
            "skip {skip}"
        );
        assert_eq!(continued.walk_position, visited, "skip {skip}");
        assert_eq!(
            fresh as u32,
            visited - skip,
            "skip {skip}: only new entries"
        );
        replays_saved += replayed;
    }
    assert!(
        replays_saved > 0,
        "the replaying resume replays directories"
    );
}

/// The window that ends at its `window_rows`-th listed entry keeps a
/// cursor there: a walk resumed from it lists the rest, and the two
/// windows hold every entry once.
#[test]
fn a_window_ended_at_a_listed_entry_resumes_from_there_without_a_gap_or_repeat() {
    let root = resume_tree();
    let (whole, _, _) = resumed(&root, 0, None, None);
    let all = relative_paths(&whole);
    for rows in 1..=all.len() as u32 {
        let mut first = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            extensions: Some(vec!["ts".to_owned()]),
            window_rows: Some(rows),
            ..Default::default()
        })
        .expect("window");
        let last = first.entries[rows as usize - 1].walk_position;
        first.end_at(last);
        let cursor = first.cursor.clone().expect("a cursor at the window's end");
        assert_eq!(cursor.position(), last);
        let (rest, replayed, _) = resumed(&root, last, Some(cursor), None);
        assert_eq!(replayed, 0, "rows {rows}");
        let mut joined = relative_paths(&first);
        joined.extend(relative_paths(&rest));
        assert_eq!(joined, all, "rows {rows}");
    }
}

/// A cursor of another walk shape is not trusted: the walk replays.
#[test]
fn a_cursor_of_another_walk_is_ignored_and_the_walk_replays() {
    let root = resume_tree();
    let (cut, _, _) = resumed(&root, 3, None, Some(std::time::Duration::ZERO));
    let cursor = cut.cursor.expect("cursor");
    let shallow = |resume| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            skip_entries: Some(3),
            max_depth: Some(1),
            resume,
            ..Default::default()
        })
        .expect("query")
    };
    assert_eq!(
        relative_paths(&shallow(Some(cursor))),
        relative_paths(&shallow(None))
    );
}

/// A resume reads the directories it holds as the walk it resumes read
/// them: an entry removed since is skipped, the rest is listed once.
#[test]
fn a_cursor_resume_survives_an_entry_removed_since() {
    let root = resume_tree();
    // The walk order is `7.ts`, `a`, `a/1.ts`, `a/2.md`, `a/inner`, ...:
    // after the 3rd entry, `a/2.md` and everything past it remain.
    let (cut, _, _) = resumed(&root, 3, None, Some(std::time::Duration::ZERO));
    let cursor = cut.cursor.expect("cursor");
    fs::remove_file(root.join("a/inner/3.ts")).expect("remove");
    let (rest, replayed, _) = resumed(&root, 3, Some(cursor), None);
    assert_eq!(replayed, 0);
    assert_eq!(relative_paths(&rest), ["b/4.ts", "c/deep/er/5.ts"]);
    let (replay, _, _) = resumed(&root, 3, None, None);
    assert_eq!(relative_paths(&rest), relative_paths(&replay));
}

/// Counts are the walk's own through the window's end: the window that
/// ends at an earlier entry leaves the rest to the walk that resumes
/// there, so the two count each entry once.
#[test]
fn a_window_counts_only_the_entries_it_covers() {
    let root = TmpDir::new();
    fs::create_dir(root.join(".git")).expect("marker");
    fs::write(root.join(".gitignore"), "*.log\n").expect("rules");
    for name in ["a.ts", "b.log", "c.ts", "d.log", "e.ts"] {
        File::create(root.join(name)).expect("file");
    }
    let walk = |skip| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            extensions: Some(vec!["ts".to_owned()]),
            skip_entries: Some(skip),
            ..Default::default()
        })
        .expect("query")
    };
    let whole = walk(0);
    assert_eq!(whole.ignored.len(), 2);
    let a = whole.entries[0].walk_position;
    let c = whole.entries[1].walk_position;
    let mut window = walk(0);
    window.end_at(c);
    assert_eq!((window.ignored.len(), window.entries.len()), (1, 2));
    assert_eq!(window.walk_position, c);
    let rest = walk(c);
    assert_eq!([window.ignored, rest.ignored].concat(), whole.ignored);
    let mut early = walk(0);
    early.end_at(a);
    assert_eq!((early.ignored.len(), early.entries.len()), (0, 1));
}
