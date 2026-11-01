//! Name, extension, depth, exclusion, size and type filters.

use super::*;

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
        size_min: Some(1),
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

    for (size_min, size_max) in [(Some(1), None), (None, Some(1_000_000))] {
        let result = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            size_min,
            size_max,
            ..Default::default()
        })
        .expect("query");
        assert!(
            result
                .entries
                .iter()
                .all(|entry| entry.entry_type == FileKind::File),
            "{size_min:?}/{size_max:?}: {:?}",
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

/// A glob given as an extension is refused with the field that takes it.
#[test]
fn a_glob_extension_is_refused() {
    let error = NameFilter::new(Path::new("."), &[], &["fs_*".to_owned()], None)
        .err()
        .expect("refused");
    assert!(error.to_string().contains("pass it as include"), "{error}");
    assert!(
        NameFilter::new(
            Path::new("."),
            &[],
            &["rs".to_owned(), ".ts".to_owned()],
            None
        )
        .is_ok()
    );
}

/// The caller's listing rule lists only what it keeps, yet the walk still
/// enters a directory it leaves unlisted, so entries under it are found.
#[test]
fn a_listing_rule_lists_what_it_keeps_and_walks_into_the_rest() {
    let root = TmpDir::new();
    for dir in ["src/tests", "lib"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
    }
    for file in ["src/a.rs", "src/tests/b.rs", "lib/c_test.rs"] {
        File::create(root.join(file)).expect("file");
    }
    let result = query_file_system_inner(FileSystemQueryOptions {
        path: root.to_string_lossy().to_string(),
        lists: Some(crate::types::ListsEntry(std::sync::Arc::new(
            |path, is_dir, _| {
                let text = path.to_string_lossy();
                !is_dir && (text.contains("/tests/") || text.ends_with("_test.rs"))
            },
        ))),
        ..Default::default()
    })
    .expect("query");
    let mut listed = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect::<Vec<_>>();
    listed.sort();
    assert_eq!(listed, ["lib/c_test.rs", "src/tests/b.rs"]);
    assert_eq!(result.total_discovered, 2);
}
