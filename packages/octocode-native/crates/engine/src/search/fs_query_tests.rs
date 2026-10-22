use super::*;
use crate::test_support::TmpDir;
use std::fs::{self, File};

fn query_file_system_inner(
    options: FileSystemQueryOptions,
) -> Result<FileSystemQueryResult, String> {
    query_file_system(options, &|_, _, _| true, &|| false).map_err(|err| err.reason)
}

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
        (paths, found.ignored)
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

#[test]
fn finds_files_by_name_glob() {
    let root = TmpDir::new();
    File::create(root.join("a.ts")).expect("create a.ts");
    File::create(root.join("b.js")).expect("create b.js");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        names: Some(vec!["*.ts".to_owned()]),
        ..Default::default()
    })
    .expect("query");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].name, "a.ts");
}

#[test]
fn filters_files_by_extension_without_returning_traversal_directories() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src")).expect("create src");
    File::create(root.join("src/a.TS")).expect("create ts");
    File::create(root.join("src/b.rs")).expect("create rs");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        recursive: Some(true),
        extensions: Some(vec![".ts".to_owned()]),
        ..Default::default()
    })
    .expect("query");

    let mut names = result
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, vec!["a.TS"]);
}

#[test]
fn respects_depth_and_exclude_dir() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src/nested")).expect("create src");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("create node_modules");
    File::create(root.join("src/nested/a.ts")).expect("create nested");
    File::create(root.join("node_modules/pkg/index.js")).expect("create ignored");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        max_depth: Some(3),
        exclude_dir: Some(vec!["node_modules".to_owned()]),
        names: Some(vec!["*.ts".to_owned()]),
        ..Default::default()
    })
    .expect("query");

    assert_eq!(result.entries.len(), 1);
    assert!(result.entries[0].path.ends_with("src/nested/a.ts"));
    assert_eq!(result.pruned_dirs, ["node_modules"]);
}

#[test]
fn exclude_globs_skip_matching_files_and_whole_directories() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src/gen/deep")).expect("create gen");
    fs::create_dir_all(root.join("vendor/lib")).expect("create vendor");
    File::create(root.join("src/a.ts")).expect("a");
    File::create(root.join("src/a.min.js")).expect("min");
    File::create(root.join("src/gen/deep/b.ts")).expect("b");
    File::create(root.join("vendor/lib/c.ts")).expect("c");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        entry_type: Some(EntryType::File),
        exclude: Some(vec![
            "vendor".to_owned(),
            "src/gen/**".to_owned(),
            "*.min.js".to_owned(),
        ]),
        ..Default::default()
    })
    .expect("query");

    let paths = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.replace('\\', "/"))
        .collect::<Vec<_>>();
    assert_eq!(paths, ["src/a.ts"]);
    // Caller exclusions are the caller's own filter, not a default prune.
    assert!(result.pruned_dirs.is_empty());
}

#[test]
fn omitted_exclusions_prune_no_directory() {
    // The engine carries no default prune list: the runtime prune policy
    // (`octocode_native::policy::prune`) hands every walk its names.
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::create_dir_all(root.join("target/debug")).expect("create target");
    File::create(root.join("src/lib.rs")).expect("create source");
    File::create(root.join("target/debug/generated.rs")).expect("create generated source");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        recursive: Some(true),
        entry_type: Some(EntryType::File),
        ..Default::default()
    })
    .expect("query");

    let mut paths = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.replace('\\', "/"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths, ["src/lib.rs", "target/debug/generated.rs"]);
}

#[test]
fn names_with_a_slash_match_the_root_relative_path_ored_with_basenames() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("scrape/deep")).expect("dirs");
    fs::create_dir_all(root.join("other")).expect("dirs");
    for file in [
        "scrape/a.go",
        "scrape/deep/b.go",
        "other/c.go",
        "other/d_test.go",
    ] {
        File::create(root.join(file)).expect("file");
    }
    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        names: Some(vec!["scrape/**/*.go".to_owned(), "*_test.go".to_owned()]),
        ..Default::default()
    })
    .expect("query");
    let mut paths = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.replace('\\', "/"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(
        paths,
        ["other/d_test.go", "scrape/a.go", "scrape/deep/b.go"]
    );
}

#[test]
fn filters_by_size_and_empty() {
    let root = TmpDir::new();
    File::create(root.join("empty.txt")).expect("create empty");
    fs::write(root.join("full.txt"), "hello").expect("write full");

    let empty = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        empty: Some(true),
        names: Some(vec!["*.txt".to_owned()]),
        ..Default::default()
    })
    .expect("query empty");
    assert_eq!(empty.entries.len(), 1);
    assert_eq!(empty.entries[0].name, "empty.txt");

    let full = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        size_greater: Some("1b".to_owned()),
        names: Some(vec!["*.txt".to_owned()]),
        ..Default::default()
    })
    .expect("query full");
    assert_eq!(full.entries.len(), 1);
    assert_eq!(full.entries[0].name, "full.txt");
}

#[test]
fn filters_by_entry_type() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src")).expect("create src");
    File::create(root.join("src/file.rs")).expect("create file");

    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        entry_type: Some(EntryType::Directory),
        ..Default::default()
    })
    .expect("query dirs");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].name, "src");
    assert_eq!(result.entries[0].entry_type, FileKind::Directory);
}

/// A directory's own length is its entry table, not content: no size
/// filter lists a directory, however many entries it holds.
#[test]
fn size_filters_list_no_directory() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("crowded")).expect("create dir");
    for index in 0..64 {
        File::create(root.join(format!("crowded/entry-with-a-long-name-{index:03}")))
            .expect("create entry");
    }
    fs::write(root.join("full.txt"), "hello").expect("write full");
    assert!(fs::metadata(root.join("crowded")).expect("dir").len() > 5);

    for (size_greater, size_less) in [(Some("1b"), None), (None, Some("1m"))] {
        let result = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            size_greater: size_greater.map(str::to_owned),
            size_less: size_less.map(str::to_owned),
            ..Default::default()
        })
        .expect("query");
        assert!(
            result
                .entries
                .iter()
                .all(|entry| entry.entry_type == FileKind::File),
            "{size_greater:?}/{size_less:?}: {:?}",
            result
                .entries
                .iter()
                .map(|entry| &entry.relative_path)
                .collect::<Vec<_>>()
        );
        assert!(result.entries.iter().any(|entry| entry.name == "full.txt"));
    }
}

/// A caller exclusion decides before the policy callback: the callback
/// never sees (or counts) an excluded entry or anything under it.
#[test]
fn excluded_entries_never_reach_the_policy_callback() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("vendor/lib")).expect("create vendor");
    File::create(root.join("vendor/lib/c.ts")).expect("c");
    File::create(root.join("secret.key")).expect("key");
    File::create(root.join("a.ts")).expect("a");

    let seen = std::sync::Mutex::new(Vec::new());
    let result = query_file_system(
        FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            exclude: Some(vec!["vendor".to_owned(), "*.key".to_owned()]),
            ..Default::default()
        },
        &|path, _, _| {
            if path != &*root {
                let relative = path.strip_prefix(&root).expect("under root");
                seen.lock()
                    .expect("lock")
                    .push(relative.to_string_lossy().replace('\\', "/"));
            }
            true
        },
        &|| false,
    )
    .expect("query");

    assert_eq!(seen.into_inner().expect("calls"), ["a.ts"]);
    assert_eq!(result.entries.len(), 1);
}

#[test]
fn slash_names_match_paths_below_the_root_with_braces_and_never_cross_a_slash_with_star() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("packages/react/src")).expect("react dir");
    fs::create_dir_all(root.join("packages/react-dom/src/deep")).expect("react-dom dir");
    fs::create_dir_all(root.join("packages/scheduler/src")).expect("scheduler dir");
    File::create(root.join("packages/react/src/ReactHooks.js")).expect("react hooks");
    File::create(root.join("packages/react-dom/src/deep/ReactDOMHooks.js"))
        .expect("react-dom hooks");
    File::create(root.join("packages/scheduler/src/Scheduler.js")).expect("scheduler file");
    let names = |glob: &str| {
        let mut names = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            names: Some(vec![glob.to_owned()]),
            entry_type: Some(EntryType::File),
            ..Default::default()
        })
        .expect("query")
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
        names.sort_unstable();
        names
    };
    assert_eq!(
        names("packages/{react,react-dom}/src/**"),
        ["ReactDOMHooks.js", "ReactHooks.js"]
    );
    assert_eq!(
        names("packages/*/src/*.js"),
        ["ReactHooks.js", "Scheduler.js"]
    );
    assert_eq!(names("*.[jt]s").len(), 3);
    assert_eq!(names("[!R]*.js"), ["Scheduler.js"]);
}

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
    assert_eq!(whole.ignored, 2);
    let a = whole.entries[0].walk_position;
    let c = whole.entries[1].walk_position;
    let mut window = walk(0);
    window.end_at(c);
    assert_eq!((window.ignored, window.entries.len()), (1, 2));
    assert_eq!(window.walk_position, c);
    let rest = walk(c);
    assert_eq!(window.ignored + rest.ignored, whole.ignored);
    let mut early = walk(0);
    early.end_at(a);
    assert_eq!((early.ignored, early.entries.len()), (0, 1));
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

/// A glob given as an extension is refused with the field that takes it.
#[test]
fn a_glob_extension_is_refused() {
    let error = NameFilter::new(Path::new("."), &[], &["fs_*".to_owned()], None)
        .err()
        .expect("refused");
    assert!(error.to_string().contains("pass it as include"), "{error}");
    assert!(NameFilter::new(Path::new("."), &[], &["rs".to_owned(), ".ts".to_owned()], None).is_ok());
}
