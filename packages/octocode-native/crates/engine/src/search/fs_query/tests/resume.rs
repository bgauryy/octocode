//! Time budgets and walks resumed at a position.

use super::*;

#[test]
fn a_spent_time_budget_stops_at_the_first_unread_entry_and_names_it() {
    let root = TmpDir::new();
    for dir in ["a", "b"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
        File::create(root.join(dir).join("f.ts")).expect("file");
    }
    let run = |time_budget| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            time_budget,
            ..Default::default()
        })
        .expect("query")
    };
    let cut = run(Some(std::time::Duration::ZERO));
    assert_eq!(cut.time_cut.as_deref(), Some("a"));
    assert!(cut.entries.is_empty(), "nothing before the cut was read");
    let whole = run(Some(std::time::Duration::from_secs(3600)));
    assert_eq!(whole.time_cut, None);
    assert_eq!(whole.entries.len(), 4);
    assert_eq!(run(None).time_cut, None);
}

#[test]
fn a_walk_resumed_at_any_position_lists_exactly_the_entries_after_it() {
    let root = TmpDir::new();
    for dir in ["a", "a/inner", "b", "c/deep/er"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
    }
    // Unlisted entries (`.md`, directories) are visited all the same.
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
    let run = |skip_entries: u32, time_budget| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            extensions: Some(vec!["ts".to_owned()]),
            skip_entries: Some(skip_entries),
            time_budget,
            ..Default::default()
        })
        .expect("query")
    };
    let names = |result: &FileSystemQueryResult| {
        result
            .entries
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect::<Vec<_>>()
    };
    let whole = run(0, None);
    let all = names(&whole);
    assert_eq!(all.len(), 5, "{all:?}");
    // 6 directories and 7 files: every entry is visited.
    let visited = whole.walk_position;
    assert_eq!(visited, 13);
    let mut resumed_from_everywhere = 0;
    for skip in 0..=visited {
        // A walk that stops at its first new entry names where it
        // resumes: the position it was resumed at.
        let cut = run(skip, Some(std::time::Duration::ZERO));
        assert_eq!(cut.time_cut.is_some(), skip < visited, "skip {skip}");
        assert_eq!(cut.walk_position, skip, "skip {skip}");
        // Resumed to the end, it lists what the whole walk lists past
        // the skipped entries, once each.
        let rest = run(skip, None);
        assert_eq!(rest.walk_position, visited, "skip {skip}");
        let before = all
            .iter()
            .filter(|path| {
                let position =
                    |p: &str| whole_position(&root, p).expect("every listed entry has a position");
                position(path) <= skip
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut joined = before.clone();
        joined.extend(names(&rest));
        assert_eq!(joined, all, "skip {skip}");
        resumed_from_everywhere += 1;
    }
    assert_eq!(resumed_from_everywhere, visited + 1);
}

#[test]
fn a_listed_entry_names_its_walk_position_and_a_walk_resumed_there_starts_after_it() {
    let root = TmpDir::new();
    for dir in ["a", "b/inner"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
    }
    for file in ["a/1.ts", "b/inner/2.ts", "b/3.md", "4.ts"] {
        File::create(root.join(file)).expect("file");
    }
    let run = |skip_entries| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            skip_entries: Some(skip_entries),
            ..Default::default()
        })
        .expect("query")
    };
    let whole = run(0);
    assert!(!whole.entries.is_empty());
    for entry in &whole.entries {
        assert_eq!(
            whole_position(&root, &entry.relative_path),
            Some(entry.walk_position),
            "{}",
            entry.relative_path
        );
    }
    for entry in &whole.entries {
        let rest = run(entry.walk_position);
        let expected = whole
            .entries
            .iter()
            .filter(|other| other.walk_position > entry.walk_position)
            .map(|other| other.relative_path.clone())
            .collect::<Vec<_>>();
        let found = rest
            .entries
            .iter()
            .map(|other| other.relative_path.clone())
            .collect::<Vec<_>>();
        assert_eq!(found, expected, "after {}", entry.relative_path);
    }
}

/// The policy callback sees every entry as a fresh one exactly once over
/// the windows of a resumed walk, and a replayed entry only as a
/// directory (the only entry a replay needs a verdict on).
#[test]
fn a_resumed_walk_shows_its_policy_callback_each_entry_fresh_once() {
    let root = TmpDir::new();
    for dir in ["a", "a/inner", "b", "c/deep"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
    }
    for file in ["a/1.ts", "a/inner/2.ts", "b/3.ts", "c/deep/4.ts", "5.ts"] {
        File::create(root.join(file)).expect("file");
    }
    let calls = |skip_entries: u32| {
        let seen = std::sync::Mutex::new(Vec::new());
        let result = query_file_system(
            FileSystemQueryOptions {
                path: root.to_string_lossy().to_string(),
                skip_entries: Some(skip_entries),
                ..Default::default()
            },
            &|path, file_type, visit| {
                if path != &*root {
                    seen.lock().expect("lock").push((
                        path.strip_prefix(&root).expect("under root").to_owned(),
                        file_type.is_some_and(|kind| kind.is_dir()),
                        visit,
                    ));
                }
                true
            },
            &|| false,
        )
        .expect("query");
        (seen.into_inner().expect("calls"), result.walk_position)
    };
    let (whole, visited) = calls(0);
    assert_eq!(visited, 10);
    assert!(whole.iter().all(|(_, _, visit)| visit.fresh_at().is_some()));
    for skip in 0..=visited {
        let (seen, _) = calls(skip);
        let fresh = seen
            .iter()
            .filter(|(_, _, visit)| visit.fresh_at().is_some())
            .map(|(path, _, _)| path.clone())
            .collect::<Vec<_>>();
        let first_new = whole.len() - fresh.len();
        let expected = whole[first_new..]
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            fresh, expected,
            "skip {skip}: the fresh entries are the new ones"
        );
        assert!(
            seen.iter()
                .filter(|(_, _, visit)| *visit == Visit::Replayed)
                .all(|(_, is_dir, _)| *is_dir),
            "skip {skip}: a replay judges directories only"
        );
    }
}

/// An unreadable directory is counted by the window that entered it; a
/// window that replays past it does not count it again.
#[cfg(unix)]
#[test]
fn a_resumed_walk_does_not_count_an_unreadable_directory_again() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    for dir in ["a", "b", "c"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
        File::create(root.join(dir).join("f.ts")).expect("file");
    }
    let locked = root.join("a");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock");
    // Root users read it anyway: nothing to prove then.
    let readable = fs::read_dir(&locked).is_ok();
    let run = |skip_entries: u32| {
        query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            skip_entries: Some(skip_entries),
            ..Default::default()
        })
        .expect("query")
    };
    let whole = run(0);
    // `a` is the first entry: a walk resumed past it replays its read.
    let rest = run(1);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
    if !readable {
        assert_eq!(whole.skipped, 1);
        assert_eq!(whole.permission_denied, 1);
        assert_eq!(rest.skipped, 0, "the window that entered `a` counted it");
        assert_eq!(rest.permission_denied, 0);
    }
}

/// 1-based position of `relative` in the name-sorted depth-first walk of
/// `root` (every entry counted, directories before their children).
fn whole_position(root: &Path, relative: &str) -> Option<u32> {
    fn visit(dir: &Path, target: &Path, count: &mut u32) -> bool {
        let mut children = fs::read_dir(dir)
            .expect("read")
            .map(|entry| entry.expect("entry").path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            *count += 1;
            if child == target || (child.is_dir() && visit(&child, target, count)) {
                return true;
            }
        }
        false
    }
    let mut count = 0;
    visit(root, &root.join(relative), &mut count).then_some(count)
}

/// A positive time budget always inspects the walk's first new entry, so
/// every resumed window makes progress however late the clock reads; only
/// a zero budget stops before it.
#[test]
fn a_positive_time_budget_inspects_the_first_new_entry() {
    let root = TmpDir::new();
    for name in ["a.ts", "b.ts", "c.ts"] {
        File::create(root.join(name)).expect("file");
    }
    for skip in 0..3 {
        let cut = query_file_system(
            FileSystemQueryOptions {
                path: root.to_string_lossy().to_string(),
                skip_entries: Some(skip),
                time_budget: Some(std::time::Duration::from_nanos(1)),
                ..Default::default()
            },
            &|_, _, _| true,
            // Every entry check outlasts the budget.
            &|| {
                std::thread::sleep(std::time::Duration::from_millis(1));
                false
            },
        )
        .expect("query");
        assert_eq!(cut.walk_position, skip + 1, "skip {skip}");
    }
}

/// A directory whose listing outlasts the walk's time budget is given up
/// mid-read: the window stops before its first entry (naming the
/// directory), and a resume, by position or by cursor, lists it whole, so
/// the windows together list every entry exactly once.
#[test]
fn a_listing_that_outlasts_the_budget_is_left_whole_to_the_resume() {
    let root = TmpDir::new();
    File::create(root.join("a.ts")).expect("file");
    fs::create_dir(root.join("big")).expect("dir");
    for index in 0..300 {
        File::create(root.join("big").join(format!("{index:03}.ts"))).expect("file");
    }
    File::create(root.join("z.ts")).expect("file");
    let walk = |skip: u32, resume: Option<WalkCursor>, late_at_big: bool| {
        query_file_system(
            FileSystemQueryOptions {
                path: root.to_string_lossy().to_string(),
                skip_entries: Some(skip),
                resume,
                time_budget: late_at_big.then_some(std::time::Duration::from_secs(3600)),
                ..Default::default()
            },
            &|path, _, _| {
                // The budget runs out as the walk admits `big`: during its
                // listing, which the walk reads right after.
                if late_at_big && path.file_name().is_some_and(|name| name == "big") {
                    crate::scan_budget::advance_walk_clock(std::time::Duration::from_secs(7200));
                }
                true
            },
            &|| false,
        )
        .expect("query")
    };
    let names = |result: &FileSystemQueryResult| {
        result
            .entries
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect::<Vec<_>>()
    };
    let whole = names(&walk(0, None, false));
    let cut = walk(0, None, true);
    assert_eq!(cut.time_cut.as_deref(), Some("big/"), "{:?}", names(&cut));
    assert!(
        names(&cut).iter().all(|path| !path.starts_with("big/")),
        "nothing of the unread listing is listed"
    );
    for (rest, how) in [
        (walk(cut.walk_position, None, false), "position"),
        (walk(cut.walk_position, cut.cursor.clone(), false), "cursor"),
    ] {
        let mut joined = names(&cut);
        joined.extend(names(&rest));
        assert_eq!(joined, whole, "resumed by {how}");
    }
}
