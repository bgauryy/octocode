use super::*;
use crate::process_env::Env;
use crate::search::walk::{GlobalFilter, IgnoreRules, walk_builder_under};
use crate::types::IgnorePolicy;
use std::collections::BTreeSet;

fn env(pairs: &[(&str, &Path)]) -> Env {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.to_string_lossy().into_owned()))
        .collect()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

/// A call's home with a git config that names `ignore` as its excludes file.
fn home_naming(dir: &Path, ignore: &str, rule: &str) -> PathBuf {
    let home = dir.join("home");
    let file = home.join(ignore);
    write(
        &home.join(".gitconfig"),
        &format!("[core]\n\texcludesFile = {}\n", file.display()),
    );
    write(&file, rule);
    home
}

/// The entries a walk of `root` under `global` yields.
fn walked(root: &Path, global: GlobalIgnore) -> BTreeSet<String> {
    walked_paths(root, global)
        .into_iter()
        .map(|path| path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect()
}

/// The root-relative files a walk of `root` under `global` yields: the
/// walker with its entry filter asking the call's global rules.
fn walked_paths(root: &Path, global: GlobalIgnore) -> BTreeSet<String> {
    let (mut builder, filter) = walk_builder_under(root, IgnorePolicy::default(), global, None);
    builder.filter_entry(move |entry| !filter.hides_entry(entry));
    builder
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() > 0 && entry.path().is_file())
        .map(|entry| crate::text::slash_path(entry.path().strip_prefix(root).expect("below")))
        .collect()
}

fn repository(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join(".git")).expect("git dir");
    write(&repo.join("keep.txt"), "x");
    write(&repo.join("a.secret"), "x");
    write(&repo.join("b.cache"), "x");
    repo
}

/// Two calls of one process, each with its own environment, resolve their
/// own global ignore file: the walk and the per-path rules of each apply that
/// call's rules, and the process's environment is read by neither.
#[test]
fn two_calls_with_different_environments_see_different_global_ignores() {
    let dir = tempfile::tempdir().expect("fixture");
    let one = tempfile::tempdir().expect("fixture");
    let home_one = home_naming(one.path(), "ignore-one", "*.secret\n");
    let two = tempfile::tempdir().expect("fixture");
    let home_two = home_naming(two.path(), "ignore-two", "*.cache\n");
    let repo = repository(dir.path());
    let first = GlobalIgnore::resolve(&env(&[("HOME", &home_one)]), None);
    let second = GlobalIgnore::resolve(&env(&[("HOME", &home_two)]), None);
    assert_ne!(first, second);
    assert_eq!(
        walked(&repo, first),
        BTreeSet::from(["b.cache".to_owned(), "keep.txt".to_owned()])
    );
    assert_eq!(
        walked(&repo, second),
        BTreeSet::from(["a.secret".to_owned(), "keep.txt".to_owned()])
    );
    let (rules_one, rules_two) = (
        IgnoreRules::with_global(&repo, first),
        IgnoreRules::with_global(&repo, second),
    );
    assert!(rules_one.is_ignored(&repo.join("a.secret"), false));
    assert!(!rules_one.is_ignored(&repo.join("b.cache"), false));
    assert!(rules_two.is_ignored(&repo.join("b.cache"), false));
    assert!(!rules_two.is_ignored(&repo.join("a.secret"), false));
    // The files each call watches are its own.
    assert!(first.inputs().contains(&home_one.join("ignore-one")));
    assert!(!first.inputs().contains(&home_two.join("ignore-two")));
    assert_eq!(
        first,
        GlobalIgnore::resolve(&env(&[("HOME", &home_one)]), None)
    );
}

/// Git's global rules apply inside a work tree only.
#[test]
fn global_rules_apply_inside_a_work_tree_only() {
    let dir = tempfile::tempdir().expect("fixture");
    let home = home_naming(dir.path(), "ignore", "*.secret\n");
    let global = GlobalIgnore::resolve(&env(&[("HOME", &home)]), None);
    let plain = dir.path().join("plain");
    write(&plain.join("a.secret"), "x");
    assert_eq!(
        walked(&plain, global),
        BTreeSet::from(["a.secret".to_owned()])
    );
}

/// The precedence of the `ignore` crate: `GIT_CONFIG_GLOBAL`, then
/// `$HOME/.gitconfig`, then `$XDG_CONFIG_HOME/git/config`, then the system
/// config; with none naming a file, `$XDG_CONFIG_HOME/git/ignore`.
#[test]
fn the_excludes_file_follows_the_git_config_precedence() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path();
    let (home, xdg, system) = (root.join("home"), root.join("xdg"), root.join("system"));
    let global_config = root.join("global-config");
    write(&global_config, "[core]\nexcludesfile = \"/from/global\"\n");
    write(
        &home.join(".gitconfig"),
        "[core]\n excludesFile = ~/from-home\n",
    );
    write(
        &xdg.join("git/config"),
        "[core]\n excludesFile = /from/xdg\n",
    );
    write(&system, "[core]\n excludesFile = /from/system\n");
    let base = [("HOME", home.as_path()), ("XDG_CONFIG_HOME", xdg.as_path())];
    let with = |extra: &[(&str, &Path)]| {
        let mut all = base.to_vec();
        all.extend_from_slice(extra);
        GlobalIgnore::resolve(&env(&all), None).file()
    };
    let system_only = [("GIT_CONFIG_SYSTEM", system.as_path())];
    assert_eq!(
        with(&[("GIT_CONFIG_GLOBAL", &global_config), system_only[0]]),
        Some(PathBuf::from("/from/global"))
    );
    assert_eq!(with(&system_only), Some(home.join("from-home")));
    std::fs::remove_file(home.join(".gitconfig")).expect("remove");
    assert_eq!(with(&system_only), Some(PathBuf::from("/from/xdg")));
    std::fs::remove_file(xdg.join("git/config")).expect("remove");
    assert_eq!(with(&system_only), Some(PathBuf::from("/from/system")));
    std::fs::remove_file(&system).expect("remove");
    assert_eq!(with(&system_only), Some(xdg.join("git/ignore")));
    // `HOME` alone: the default ignore file is under `~/.config`.
    assert_eq!(
        GlobalIgnore::resolve(
            &env(&[("HOME", &home), ("GIT_CONFIG_SYSTEM", &system)]),
            None
        )
        .file(),
        Some(home.join(".config/git/ignore"))
    );
}

/// Two repositories below a root that is no work tree, and a global file of
/// one unanchored and one anchored pattern. Git reads each pattern relative
/// to the work tree an entry is in, so `/anchored.txt` hides the file at each
/// repository's top and nowhere deeper, and `*.secret` hides it everywhere
/// below a repository, never outside one. The walker's entry filter and the
/// per-path rules agree with that.
#[test]
fn global_rules_apply_per_repository_below_a_root_that_is_no_work_tree() {
    let dir = tempfile::tempdir().expect("fixture");
    let home = home_naming(dir.path(), "ignore", "*.secret\n/anchored.txt\n");
    let global = GlobalIgnore::resolve(&env(&[("HOME", &home)]), None);
    let root = dir.path().join("root");
    for repo in ["one", "deep/two"] {
        std::fs::create_dir_all(root.join(repo).join(".git")).expect("git dir");
        for file in [
            "keep.txt",
            "a.secret",
            "anchored.txt",
            "sub/anchored.txt",
            "sub/b.secret",
        ] {
            write(&root.join(repo).join(file), "x");
        }
    }
    for file in ["plain.secret", "anchored.txt", "loose/anchored.txt"] {
        write(&root.join(file), "x");
    }
    let expected: BTreeSet<String> = [
        "anchored.txt",
        "loose/anchored.txt",
        "plain.secret",
        "one/keep.txt",
        "one/sub/anchored.txt",
        "deep/two/keep.txt",
        "deep/two/sub/anchored.txt",
    ]
    .map(str::to_owned)
    .into();
    assert_eq!(walked_paths(&root, global), expected);
    let rules = IgnoreRules::with_global(&root, global);
    for repo in ["one", "deep/two"] {
        let repo = root.join(repo);
        assert!(rules.is_ignored(&repo.join("a.secret"), false));
        assert!(rules.is_ignored(&repo.join("sub/b.secret"), false));
        assert!(rules.is_ignored(&repo.join("anchored.txt"), false));
        assert!(!rules.is_ignored(&repo.join("sub/anchored.txt"), false));
    }
    assert!(!rules.is_ignored(&root.join("plain.secret"), false));
    assert!(!rules.is_ignored(&root.join("anchored.txt"), false));
}

/// A rule of the repository's own ignore files outranks the global file: an
/// entry its `.gitignore` re-includes stays in the walk.
#[test]
fn a_repository_rule_that_re_includes_outranks_a_global_rule() {
    let dir = tempfile::tempdir().expect("fixture");
    let home = home_naming(dir.path(), "ignore", "*.secret\n");
    let global = GlobalIgnore::resolve(&env(&[("HOME", &home)]), None);
    let repo = repository(dir.path());
    write(&repo.join(".gitignore"), "!a.secret\n");
    assert!(walked(&repo, global).contains("a.secret"));
    assert!(!IgnoreRules::with_global(&repo, global).is_ignored(&repo.join("a.secret"), false));
}

/// A walk that lifts the ignore rules hides nothing.
#[test]
fn a_walk_that_lifts_ignore_rules_ignores_the_global_file() {
    let dir = tempfile::tempdir().expect("fixture");
    let home = home_naming(dir.path(), "ignore", "*.secret\n");
    let global = GlobalIgnore::resolve(&env(&[("HOME", &home)]), None);
    let repo = repository(dir.path());
    let filter = GlobalFilter::new(&repo, IgnorePolicy::ALL, global);
    assert!(!filter.hides(&repo.join("a.secret"), false));
}
