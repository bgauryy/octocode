use super::*;
use std::collections::BTreeSet;
use std::fs;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(path, text).expect("write");
}

/// Every entry `walk_builder` yields under `root` (hidden entries walked).
fn walked(root: &Path, policy: IgnorePolicy) -> BTreeSet<String> {
    walk_builder(root, policy, None)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() > 0)
        .map(|entry| relative(root, entry.path()))
        .collect()
}

/// Every entry a plain recursive walk reaches when [`IgnoreRules`] decides
/// each entry and ignored directories are not entered.
fn decided(root: &Path) -> BTreeSet<String> {
    let rules = IgnoreRules::new(root);
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read dir").flatten() {
            let path = entry.path();
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if rules.is_ignored(&path, is_dir) {
                continue;
            }
            out.insert(relative(root, &path));
            if is_dir {
                stack.push(path);
            }
        }
    }
    out
}

fn relative(root: &Path, path: &Path) -> String {
    crate::text::slash_path(path.strip_prefix(root).expect("under root"))
}

fn fixture() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo");
    let root = repo.path();
    fs::create_dir_all(root.join(".git/info")).expect("git dir");
    write(&root.join(".git/info/exclude"), "excluded.txt\n");
    write(&root.join(".gitignore"), "*.log\nbuild/\n!keep.log\n");
    write(&root.join("app/.gitignore"), "gen/\n");
    write(&root.join("app/.ignore"), "!gen/\nlocal.txt\n");
    for file in [
        "a.rs",
        "a.log",
        "keep.log",
        "excluded.txt",
        "build/out.rs",
        "app/main.rs",
        "app/gen/x.rs",
        "app/local.txt",
        "app/nested/deep.log",
        "app/nested/ok.rs",
    ] {
        write(&root.join(file), "x\n");
    }
    repo
}

/// The per-path rules leave out exactly what the walker leaves out: nested
/// `.gitignore` files, a `!` re-include, `.ignore` beating `.gitignore`,
/// and `.git/info/exclude`.
#[test]
fn ignore_rules_decide_what_the_walker_walks() {
    let repo = fixture();
    let root = repo.path();
    let policy = IgnorePolicy {
        include_ignored: false,
        include_hidden: true,
    };
    let walked = walked(root, policy);
    assert_eq!(walked, decided(root));
    for left_out in [
        "a.log",
        "excluded.txt",
        "build",
        "app/local.txt",
        "app/nested/deep.log",
    ] {
        assert!(!walked.contains(left_out), "{left_out}: {walked:?}");
    }
    for kept in ["keep.log", "app/gen/x.rs", "app/nested/ok.rs", "a.rs"] {
        assert!(walked.contains(kept), "{kept}: {walked:?}");
    }
}

/// A subdirectory root still honors the ignore files above it, up to the
/// repository root.
#[test]
fn a_root_below_the_repository_honors_the_ignore_files_above_it() {
    let repo = fixture();
    let root = repo.path().join("app");
    let policy = IgnorePolicy {
        include_ignored: false,
        include_hidden: true,
    };
    let walked = walked(&root, policy);
    assert_eq!(walked, decided(&root));
    assert!(!walked.contains("nested/deep.log"), "{walked:?}");
}

/// Outside a repository `.gitignore` files do not apply; `.ignore` files do.
#[test]
fn gitignore_needs_a_repository_and_dot_ignore_does_not() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    write(&root.join(".gitignore"), "a.txt\n");
    write(&root.join(".ignore"), "b.txt\n");
    write(&root.join("a.txt"), "x");
    write(&root.join("b.txt"), "x");
    let rules = IgnoreRules::new(root);
    assert!(!rules.is_ignored(&root.join("a.txt"), false));
    assert!(rules.is_ignored(&root.join("b.txt"), false));
    let policy = IgnorePolicy {
        include_ignored: false,
        include_hidden: true,
    };
    assert_eq!(walked(root, policy), decided(root));
}

/// `includeIgnored` lifts every source.
#[test]
fn include_ignored_lifts_every_source() {
    let repo = fixture();
    let all = walked(repo.path(), IgnorePolicy::ALL);
    for file in [
        "a.log",
        "excluded.txt",
        "build/out.rs",
        "app/local.txt",
        ".gitignore",
    ] {
        assert!(all.contains(file), "{file}: {all:?}");
    }
    assert!(!walked(repo.path(), IgnorePolicy::default()).contains(".gitignore"));
}

/// `why` names the rule of the topmost ignored entry on the way to a path,
/// with its file and line.
#[test]
fn why_names_the_deciding_rule_and_its_line() {
    let repo = fixture();
    let root = repo.path();
    let rules = IgnoreRules::new(root);
    let reason = rules
        .why(&root.join("build/out.rs"), false)
        .expect("ignored");
    assert_eq!(reason.entry, root.join("build"));
    assert_eq!(reason.pattern, "build/");
    assert_eq!(reason.line, Some(2));
    assert_eq!(
        reason.source.as_deref(),
        Some(root.join(".gitignore").as_path())
    );
    let excluded = rules
        .why(&root.join("excluded.txt"), false)
        .expect("excluded");
    assert_eq!(excluded.pattern, "excluded.txt");
    assert!(rules.why(&root.join("keep.log"), false).is_none());
    assert!(rules.why(&root.join("app/gen/x.rs"), false).is_none());
    assert!(rules.why(root, true).is_none());
    // A rule above the root is named along the root's own path.
    let app = root.join("app");
    let above = IgnoreRules::new(&app)
        .why(&app.join("nested/deep.log"), false)
        .expect("ignored from above");
    assert_eq!(
        above.source.as_deref(),
        Some(root.join(".gitignore").as_path())
    );
}

/// As a default prune ([`Prune::at`]), inside a git work tree a listed
/// name is pruned only where git ignores it (it or a directory on the
/// way) or it is `.git`; outside one, by its name alone.
#[test]
fn a_default_prune_inside_a_repository_skips_only_ignored_directories() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).expect("git dir");
    write(&root.join(".gitignore"), "/target/\nout/\n");
    let prune = || Prune::new([".git", "build", "target", "coverage"]);
    let rooted = prune().at(root);
    for (path, pruned) in [
        (".git", true),
        ("build", false),
        ("src/coverage", false),
        ("target", true),
        ("out/build", true),
    ] {
        assert_eq!(rooted.prunes(&root.join(path), true), pruned, "{path}");
    }
    assert!(
        !rooted.prunes(&root.join("target"), false),
        "files are not pruned"
    );
    let outside = tempfile::tempdir().expect("dir");
    let rooted = prune().at(outside.path());
    assert!(rooted.prunes(&outside.path().join("build"), true));
    assert!(rooted.prunes(&outside.path().join("src/coverage"), true));
}

/// A root's external ignore inputs: the ignore files of the directories
/// above it and the repository's exclude file, never its own walked
/// `.gitignore`.
#[test]
fn external_ignore_inputs_name_the_files_above_the_root() {
    let repo = fixture();
    let real = fs::canonicalize(repo.path()).expect("real");
    let inputs = external_ignore_inputs(&repo.path().join("app"));
    for expected in [
        real.join(".gitignore"),
        real.join(".ignore"),
        real.join(".git/info/exclude"),
    ] {
        assert!(inputs.contains(&expected), "{expected:?}: {inputs:?}");
    }
    assert!(!inputs.contains(&real.join("app/.gitignore")), "{inputs:?}");
    let unique = inputs.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), inputs.len(), "{inputs:?}");
}

#[test]
fn prune_skips_named_directories_below_the_root_only() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().join("target");
    write(&root.join("src/a.rs"), "x");
    write(&root.join("target/b.rs"), "x");
    write(&root.join("node_modules/c.rs"), "x");
    let prune = Prune::new(["target", "node_modules"]);
    let mut walk = walk_builder(&root, IgnorePolicy::default(), None);
    walk.filter_entry(move |entry| !prune.prunes_entry(entry));
    let files: BTreeSet<String> = walk
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| relative(&root, entry.path()))
        .collect();
    assert_eq!(files, BTreeSet::from(["src/a.rs".to_owned()]));
}

/// A lifted walk enters neither a clone nor a submodule below its root
/// (a `.git` directory or file), records both, and still walks the
/// root's own repository and its gitignored directories.
#[test]
fn a_lifted_walk_skips_and_records_nested_repositories() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    write(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&root.join(".gitignore"), "repos/\ngen/\n");
    write(&root.join("src/a.rs"), "x");
    write(&root.join("gen/b.rs"), "x");
    write(
        &root.join("repos/clone/.git/HEAD"),
        "ref: refs/heads/main\n",
    );
    write(&root.join("repos/clone/c.rs"), "x");
    write(
        &root.join("repos/module/.git"),
        "gitdir: ../../.git/modules/m\n",
    );
    write(&root.join("repos/module/d.rs"), "x");
    write(&root.join("repos/plain/e.rs"), "x");
    let nested = std::sync::Arc::new(NestedRepositories::new(root));
    let skips = std::sync::Arc::clone(&nested);
    let mut builder = walk_builder(root, IgnorePolicy::ALL, None);
    builder.filter_entry(move |entry| {
        !skips.skips(
            entry.path(),
            entry.file_type().is_some_and(|kind| kind.is_dir()),
        )
    });
    let files: BTreeSet<String> = builder
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| relative(root, entry.path()))
        .filter(|path| !path.starts_with(".git"))
        .collect();
    assert_eq!(
        files,
        BTreeSet::from(["gen/b.rs", "repos/plain/e.rs", "src/a.rs"].map(str::to_owned))
    );
    assert_eq!(nested.skipped(), ["repos/clone", "repos/module"]);
}
