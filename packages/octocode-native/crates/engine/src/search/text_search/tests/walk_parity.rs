//! The search walks exactly the entries the ignore walker walks.

use super::*;

/// The search applies the ignore rules itself (so it can report what they
/// leave out), and must walk exactly what the `ignore` walker with its own
/// ignore rules walks: nested `.gitignore` files, a `!` re-include, `.ignore`
/// beating `.gitignore`, `.git/info/exclude`, rules above a subdirectory
/// root, a nested repository, a directory outside any repository, and an
/// include glob or `language` type over ignored and dot entries, under each
/// `includeIgnored`/`includeHidden` combination. Every reported ignored
/// entry is one the walker left out, and nothing under it was searched.
#[test]
fn the_search_walks_exactly_what_the_ignore_walker_walks() {
    use crate::types::IgnorePolicy;
    struct Reported(Mutex<Vec<String>>);
    impl TextSearchPathFilter for Reported {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_ignored(&self, path: &Path, _: bool) {
            self.0
                .lock()
                .expect("lock")
                .push(path.to_string_lossy().into_owned());
        }
    }
    let repo = TmpDir::new();
    repo.write(".git/info/exclude", "excluded.txt\n");
    repo.write(".gitignore", "*.log\nbuild/\n!keep.log\n!.keepme\n");
    repo.write("app/.gitignore", "gen/\n");
    repo.write("app/.ignore", "!gen/\nlocal.txt\n");
    repo.write("nested/.git/HEAD", "ref: refs/heads/main\n");
    repo.write("nested/.gitignore", "inner.rs\n");
    for file in [
        "a.rs",
        "a.log",
        "keep.log",
        ".keepme",
        ".hidden.rs",
        "excluded.txt",
        "build/out.rs",
        "build/.dot.rs",
        "app/main.rs",
        "app/gen/x.rs",
        "app/local.txt",
        "app/nested/deep.log",
        "app/nested/ok.rs",
        "app/.cache/c.rs",
        "nested/inner.rs",
        "nested/outer.rs",
        "nested/x.log",
    ] {
        repo.write(file, "x\n");
    }
    let plain = TmpDir::new();
    plain.write(".gitignore", "a.txt\n");
    plain.write(".ignore", "b.txt\n");
    for file in ["a.txt", "b.txt", "c.txt", "sub/b.txt", "sub/.d.txt"] {
        plain.write(file, "x\n");
    }
    let roots = [
        repo.0.clone(),
        repo.0.join("app"),
        repo.0.join("nested"),
        plain.0.clone(),
    ];
    let policies = [
        IgnorePolicy::default(),
        IgnorePolicy::new(Some(true), None),
        IgnorePolicy::new(None, Some(true)),
        IgnorePolicy::ALL,
    ];
    type Scope = (Option<Vec<String>>, Option<String>);
    let scopes: [Scope; 3] = [
        (None, None),
        (Some(vec!["*.log".into(), "**/*.rs".into()]), None),
        (None, Some("rust".into())),
    ];
    let files_of = |root: &Path, paths: Vec<String>| -> BTreeSet<String> {
        paths
            .into_iter()
            .map(|path| {
                Path::new(&path)
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    };
    for root in &roots {
        for policy in policies {
            for (include, lang) in &scopes {
                let mut walker = crate::search::walk::walk_builder(root, policy, None);
                if let Some(include) = include {
                    walker.overrides(
                        crate::search::glob::compile(root, include, &[])
                            .expect("globs")
                            .overrides(),
                    );
                }
                if let Some(lang) = lang {
                    let mut types = TypesBuilder::new();
                    types.add_defaults();
                    types.select(lang);
                    walker.types(types.build().expect("types"));
                }
                let walked = files_of(
                    root,
                    walker
                        .build()
                        .filter_map(std::result::Result::ok)
                        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
                        .map(|entry| entry.path().to_string_lossy().into_owned())
                        .collect(),
                );
                let reported = Arc::new(Reported(Mutex::default()));
                // Every fixture line is non-empty: `.` lists every searched file.
                let mut options = opts(root.to_string_lossy().into_owned(), ".");
                options.ignore = policy;
                options.include.clone_from(include);
                options.lang_type.clone_from(lang);
                options.output = SearchOutput::FilesWithMatch;
                let found = search_cancellable(options.clone(), reported.clone(), &|| false)
                    .expect("search");
                let searched = files_of(
                    root,
                    found.files.into_iter().map(|file| file.path).collect(),
                );
                let case = format!("{root:?} {policy:?} {include:?} {lang:?}");
                assert_eq!(searched, walked, "{case}");
                // The lazy sorted walk a stream reads admits the same files
                // and reports the same left out entries.
                let streamed_reported = Arc::new(Reported(Mutex::default()));
                let Streaming::Streamed(streamed) = search_stream(
                    options.clone(),
                    streamed_reported.clone(),
                    &|| false,
                    Stream {
                        bind: String::new(),
                        from: 0,
                        pin: None,
                        anchor: Anchor::START,
                        enough: Box::new(|_| false),
                        prepare: None,
                    },
                )
                .expect("stream") else {
                    panic!("{case}: stale")
                };
                assert!(streamed.ended, "{case}");
                let streamed_files = streamed
                    .result
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>();
                assert!(
                    streamed_files
                        .windows(2)
                        .all(|pair| Path::new(&pair[0]) <= Path::new(&pair[1])),
                    "{case}: not in walk order: {streamed_files:?}"
                );
                assert_eq!(files_of(root, streamed_files), searched, "{case}");
                let left_out =
                    |reported: &Reported| files_of(root, reported.0.lock().expect("lock").clone());
                assert_eq!(
                    left_out(&streamed_reported),
                    left_out(&reported),
                    "{case}: left out entries"
                );
                let reported = files_of(root, reported.0.lock().expect("lock").clone());
                assert!(
                    !policy.include_ignored || reported.is_empty(),
                    "{case}: {reported:?}"
                );
                for entry in &reported {
                    assert!(
                        !walked.contains(entry),
                        "{case}: reported {entry} was walked"
                    );
                    let below = format!("{entry}/");
                    assert!(
                        searched.iter().all(|file| !file.starts_with(&below)),
                        "{case}: searched under ignored {entry}"
                    );
                }
                if !policy.include_ignored && root == &repo.0 && include.is_none() && lang.is_none()
                {
                    for left_out in ["a.log", "excluded.txt", "build", "nested/inner.rs"] {
                        assert!(
                            reported.contains(left_out),
                            "{case}: {left_out} unreported: {reported:?}"
                        );
                    }
                    // A nested repository's own rules decide below it.
                    assert!(searched.contains("nested/x.log"), "{case}: {searched:?}");
                }
            }
        }
    }
}

/// An ignore file the search cannot read applies no rules; the search counts
/// it as an error and names it, whichever way the walk is driven, and a
/// readable sibling's rules still apply.
#[cfg(unix)]
#[test]
fn an_unreadable_ignore_file_is_an_error_the_result_names() {
    use std::os::unix::fs::PermissionsExt;
    for sort in [SearchSort::Traversal, SearchSort::Path] {
        let repo = TmpDir::new();
        repo.write(".git/HEAD", "ref: refs/heads/main\n");
        repo.write(".gitignore", "locked.txt\n");
        repo.write(".ignore", "listed.txt\n");
        for file in ["locked.txt", "listed.txt", "kept.txt"] {
            repo.write(file, "needle\n");
        }
        let locked = repo.0.join(".gitignore");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read(&locked).is_ok() {
            // Root reads through the mode: nothing is unreadable here.
            return;
        }
        let mut o = opts(repo.path(), "needle");
        o.sort = sort;
        let found = search_cancellable(o, Arc::new(AllowAll), &|| false).expect("search");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("restore");
        assert_eq!(
            found.stats.error_count,
            Some(1),
            "{sort:?}: {:?}",
            found.stats
        );
        assert!(
            found
                .stats
                .first_error
                .as_deref()
                .is_some_and(|error| error.contains(".gitignore")),
            "{sort:?}: {:?}",
            found.stats.first_error
        );
        let files = found
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert!(
            files.iter().any(|path| path.ends_with("kept.txt")),
            "{files:?}"
        );
        assert!(
            !files.iter().any(|path| path.ends_with("listed.txt")),
            "{files:?}"
        );
    }
}
