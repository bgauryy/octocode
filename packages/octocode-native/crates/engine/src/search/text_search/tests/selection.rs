//! Which files a search walks: depth, globs, pruning, ignore rules, hidden entries, symlinks, file
//! terms.

use super::*;

#[test]
fn max_depth_prunes_the_native_walk_and_statistics() {
    let t = TmpDir::new();
    t.write("root.ts", "needle\n");
    t.write("one/nested.ts", "needle\n");
    t.write("one/two/deep.ts", "needle\n");

    let mut options = opts(t.path(), "needle");
    options.max_depth = Some(0);
    let result = search(options).expect("search ok");

    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("root.ts"));
    assert_eq!(result.stats.files_searched, Some(1));
    assert_eq!(result.stats.files_matched, Some(1));
    assert_eq!(result.stats.match_count, Some(1));
    assert_eq!(result.stats.matched_lines, Some(1));
}

/// One file-selection option decides which files are listed, in order.
#[test]
fn file_selection_options_decide_the_listed_files() {
    type Configure = fn(&mut TextSearchOptions);
    // (case, files, pattern, option, listed path suffixes in order)
    type Case = (
        &'static str,
        &'static [(&'static str, &'static str)],
        &'static str,
        Configure,
        &'static [&'static str],
    );
    let cases: [Case; 4] = [
        (
            "files_without_match inverts the file set",
            &[("a.txt", "needle\n"), ("b.txt", "haystack\n")],
            "needle",
            |o| o.output = SearchOutput::FilesWithoutMatch,
            &["b.txt"],
        ),
        (
            "lang_type filters by extension",
            &[("a.ts", "target\n"), ("b.py", "target\n")],
            "target",
            |o| o.lang_type = Some("ts".to_owned()),
            &["a.ts"],
        ),
        (
            "include glob restricts files",
            &[("a.ts", "target\n"), ("b.js", "target\n")],
            "target",
            |o| o.include = Some(vec!["*.ts".to_owned()]),
            &["a.ts"],
        ),
        (
            "sort_reverse flips the order",
            &[("a.txt", "m\n"), ("z.txt", "m\n")],
            "m",
            |o| o.sort_reverse = Some(true),
            &["z.txt", "a.txt"],
        ),
    ];
    for (case, files, pattern, configure, listed) in cases {
        let t = TmpDir::new();
        for (name, content) in files {
            t.write(name, content);
        }
        let mut o = opts(t.path(), pattern);
        configure(&mut o);
        let r = search(o).expect(case);
        assert_eq!(r.files.len(), listed.len(), "{case}");
        for (file, suffix) in r.files.iter().zip(listed) {
            assert!(
                file.path.ends_with(suffix),
                "{case}: {} vs {suffix}",
                file.path
            );
        }
    }
}

#[test]
fn exclude_globs_drop_files_with_or_without_a_leading_bang() {
    for exclude in ["*.log", "!*.log"] {
        let t = TmpDir::new();
        t.write("a.txt", "target\n");
        t.write("b.log", "target\n");
        let mut o = opts(t.path(), "target");
        o.exclude = Some(vec![exclude.to_owned()]);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1, "{exclude}");
        assert!(r.files[0].path.ends_with("a.txt"), "{exclude}");
    }
}

#[test]
fn exclude_dir_prunes_directory() {
    let t = TmpDir::new();
    t.write("keep/a.txt", "target\n");
    t.write("skip/b.txt", "target\n");
    let mut o = opts(t.path(), "target");
    o.exclude_dir = Some(vec!["skip".to_owned()]);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.contains("keep"));
    // A pruned directory is disclosed, root-relative: absence there is unproven.
    assert_eq!(r.stats.pruned_dirs, Some(vec!["skip".to_owned()]));
}

#[test]
fn the_search_root_itself_is_never_pruned() {
    let t = TmpDir::new();
    t.write("build/a.txt", "target\n");
    let mut o = opts(format!("{}/build", t.path()), "target");
    o.exclude_dir = Some(vec!["build".to_owned()]);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.stats.pruned_dirs, None);
}

#[test]
fn respects_gitignore_by_default_and_no_ignore_overrides() {
    let t = TmpDir::new();
    // The `ignore` crate only applies .gitignore inside a git repo
    // (require_git defaults true); a bare `.git` dir marks the temp tree as one.
    fs::create_dir_all(t.0.join(".git")).expect("create .git");
    t.write(".gitignore", "ignored.txt\n");
    t.write("ignored.txt", "target\n");
    t.write("kept.txt", "target\n");

    let r = search(opts(t.path(), "target")).expect("ok");
    assert_eq!(r.files.len(), 1, "gitignore'd file should be skipped");
    assert!(r.files[0].path.ends_with("kept.txt"));

    let mut o = opts(t.path(), "target");
    o.ignore.include_ignored = true;
    let r2 = search(o).expect("ok");
    assert_eq!(r2.files.len(), 2, "--no-ignore searches the ignored file");
}

/// A symlink the walk admits but does not follow (to a file or a directory)
/// is reported to the path filter, never dropped unseen.
#[cfg(unix)]
#[test]
fn an_unfollowed_symlink_is_reported_to_the_filter() {
    struct Recording(std::sync::Mutex<Vec<String>>);
    impl TextSearchPathFilter for Recording {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_symlink(&self, path: &Path) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            self.0.lock().expect("lock").push(name.into_owned());
        }
    }
    let t = TmpDir::new();
    t.write("real.txt", "needle\n");
    t.write("dir/inner.txt", "needle\n");
    std::os::unix::fs::symlink(t.0.join("real.txt"), t.0.join("file-link")).expect("link");
    std::os::unix::fs::symlink(t.0.join("dir"), t.0.join("dir-link")).expect("link");
    let filter = Arc::new(Recording(std::sync::Mutex::new(Vec::new())));
    let result =
        search_cancellable(opts(t.path(), "needle"), filter.clone(), &|| false).expect("search");
    assert_eq!(result.files.len(), 2);
    let mut seen = filter.0.lock().expect("lock").clone();
    seen.sort();
    assert_eq!(seen, ["dir-link", "file-link"]);
}

/// File-level AND/NOT: a matched file is kept only when it also contains
/// every `file_contains_all` term and none of `file_contains_none`; totals
/// count only kept files. Each term is smart-case on its own.
#[test]
fn file_terms_keep_files_containing_all_and_none_of_the_excluded() {
    let t = TmpDir::new();
    t.write("a.rs", "alpha\nbeta\n");
    t.write("b.rs", "alpha\nGAMMA\n");
    t.write("c.rs", "alpha\nbeta\ngamma\n");
    t.write("d.rs", "alpha only\n");
    let mut o = opts(t.path(), "alpha");
    o.file_contains_all = Some(vec!["beta".into()]);
    o.file_contains_none = Some(vec!["gamma".into()]);
    let r = search(o).expect("search ok");
    let paths: Vec<_> = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(paths, ["a.rs"], "{paths:?}");
    assert_eq!(r.stats.files_matched, Some(1));
    // An uppercase term is case-sensitive: `GAMMA` excludes only b.rs.
    let mut o = opts(t.path(), "alpha");
    o.file_contains_none = Some(vec!["GAMMA".into()]);
    let r = search(o).expect("search ok");
    let mut paths: Vec<_> = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect();
    paths.sort();
    assert_eq!(paths, ["a.rs", "c.rs", "d.rs"], "{paths:?}");
}

/// Without `includeHidden` the walk leaves dot entries out and reports each
/// one to the path filter (a dot directory once, never its contents); an
/// include glob or `language` type that selects a dot file keeps it, as the
/// `ignore` walker's own rule does. With `includeHidden` nothing is skipped.
#[test]
fn hidden_entries_are_skipped_and_reported() {
    struct Recording(Mutex<BTreeSet<String>>);
    impl TextSearchPathFilter for Recording {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_hidden(&self, path: &Path, _: bool) {
            self.0
                .lock()
                .expect("lock")
                .insert(path.file_name().expect("name").to_string_lossy().into());
        }
    }
    let t = TmpDir::new();
    t.write("src/a.js", "needle\n");
    t.write(".github/workflows/ci.yml", "needle\n");
    t.write(".eslintrc.js", "needle\n");
    t.write("src/.local.js", "needle\n");
    let run = |configure: &dyn Fn(&mut TextSearchOptions)| {
        let filter = Arc::new(Recording(Mutex::default()));
        let mut options = opts(t.path(), "needle");
        configure(&mut options);
        let found = search_cancellable(options, filter.clone(), &|| false).expect("search");
        let mut files = found
            .files
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .strip_prefix(&t.0)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        files.sort();
        let skipped = filter.0.lock().expect("lock").clone();
        (files, skipped.into_iter().collect::<Vec<_>>())
    };
    assert_eq!(
        run(&|_| {}),
        (
            vec!["src/a.js".to_owned()],
            vec![
                ".eslintrc.js".to_owned(),
                ".github".to_owned(),
                ".local.js".to_owned()
            ]
        )
    );
    // The dot directory may hold another `.eslintrc.js`: still a skip.
    assert_eq!(
        run(&|options| options.include = Some(vec!["**/.eslintrc.js".into()])),
        (vec![".eslintrc.js".to_owned()], vec![".github".to_owned()])
    );
    let (typed, skipped) = run(&|options| options.lang_type = Some("js".into()));
    assert_eq!(typed, [".eslintrc.js", "src/.local.js", "src/a.js"]);
    assert_eq!(skipped, [".github"]);
    let (all, skipped) = run(&|options| options.ignore = crate::types::IgnorePolicy::ALL);
    assert_eq!(
        all,
        [
            ".eslintrc.js",
            ".github/workflows/ci.yml",
            "src/.local.js",
            "src/a.js"
        ]
    );
    assert!(skipped.is_empty());
}

/// A file root is searched on the calling thread: the parallel walker's
/// pool (and its idle 1 ms polls) is a fixed cost one entry never repays.
/// A directory root still walks on the pool. Both find the same match.
#[test]
fn a_file_root_is_searched_on_the_calling_thread() {
    struct Threads(Mutex<Vec<std::thread::ThreadId>>);
    impl TextSearchPathFilter for Threads {
        fn allows(&self, _: &Path, is_dir: bool) -> bool {
            if !is_dir {
                self.0
                    .lock()
                    .expect("lock")
                    .push(std::thread::current().id());
            }
            true
        }
    }
    let t = TmpDir::new();
    t.write("one.rs", "fn needle() {}\n");
    let file = Path::new(&t.path()).join("one.rs");
    for (root, serial) in [
        (file.to_string_lossy().into_owned(), true),
        (t.path(), false),
    ] {
        let threads = Arc::new(Threads(Mutex::new(Vec::new())));
        let result = search_cancellable(opts(root.clone(), "needle"), threads.clone(), &|| false)
            .expect("search ok");
        assert_eq!(result.files.len(), 1, "{root}");
        assert_eq!(result.files[0].matches[0].line, 1, "{root}");
        let seen = threads.0.lock().expect("lock").clone();
        assert_eq!(seen.len(), 1, "{root}");
        assert_eq!(seen[0] == std::thread::current().id(), serial, "{root}");
    }
}
