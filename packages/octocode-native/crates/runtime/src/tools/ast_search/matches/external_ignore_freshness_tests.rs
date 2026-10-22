use super::super::test_rows::run_row;
use super::*;
use crate::tools::test_support::{Fixture, simple_policy};

fn sources(root: &std::path::Path) -> std::path::PathBuf {
    let scope = root.join("scope");
    std::fs::create_dir_all(scope.join("hidden")).expect("scope");
    std::fs::write(scope.join("a.ts"), "target(\"one\");\ntarget(\"two\");\n").expect("source");
    std::fs::write(scope.join("hidden/b.ts"), "target(\"hidden\");\n").expect("hidden source");
    scope
}

fn settle() {
    octocode_engine::stamp::settle_written_so_far();
}

fn query(scope: &std::path::Path, include_ignored: bool) -> Value {
    json!({"operation":"match", "path":scope, "pattern":"target($A)",
            "captureText":true, "matchPageSize":1, "includeIgnored":include_ignored})
}

#[test]
fn held_memo_is_evicted_when_an_external_ignore_dependency_appears() {
    for dependency in [".ignore", ".git", ".jj"] {
        let root = Fixture::new();
        let scope = sources(&root.0);
        let (paths, security) = simple_policy(&root.0);
        settle();
        let first = run_row(&query(&scope, false), &paths, &security);
        assert_eq!(first["matchCount"], 3, "{first}");
        let snapshot = first["snapshot"].as_str().expect("snapshot");
        assert!(SCANS.holds(snapshot), "exercise a held no-repository memo");
        let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
        assert_eq!(run_row(&next, &paths, &security)["matchCount"], 3);
        assert!(
            SCANS.holds(snapshot),
            "unchanged continuation really reuses"
        );
        if dependency == ".ignore" {
            std::fs::write(root.0.join(dependency), "scope/hidden/\n").expect("parent ignore");
        } else {
            std::fs::create_dir(root.0.join(dependency)).expect("repository marker");
            std::fs::write(root.0.join(".gitignore"), "scope/hidden/\n").expect("parent gitignore");
        }
        let changed = run_row(&next, &paths, &security);
        assert_eq!(
            changed["errorCode"], "staleSnapshot",
            "{dependency}: {changed}"
        );
        assert!(
            !SCANS.holds(snapshot),
            "ineligible old scan must be evicted"
        );
        assert_eq!(
            run_row(&query(&scope, false), &paths, &security)["matchCount"],
            2
        );
    }
}

/// Inside a repository (ancestor `.git`, its `info/exclude`, a worktree's
/// shared exclude through `commondir`, a `.jj` marker, a repository nested
/// in the scope) the stored scan serves its continuation pages, and an edit
/// of the rules that decided the walk drops it: the page restarts.
#[test]
fn repository_rules_keep_the_memo_and_an_edit_drops_it() {
    // Every fixture is written first, so one wait settles them all.
    let mut fixtures = Vec::new();
    for kind in ["ancestor", "exclude", "worktree", "jj", "nested"] {
        let root = Fixture::new();
        let scope = sources(&root.0);
        let rules = match kind {
            "ancestor" => {
                std::fs::create_dir(root.0.join(".git")).expect("git marker");
                root.0.join(".gitignore")
            }
            "exclude" => {
                std::fs::create_dir_all(root.0.join(".git/info")).expect("git info");
                root.0.join(".git/info/exclude")
            }
            "worktree" => {
                let gitdir = root.0.join("metadata/worktrees/fixture");
                std::fs::create_dir_all(&gitdir).expect("worktree metadata");
                std::fs::create_dir_all(root.0.join("metadata/info")).expect("common git info");
                std::fs::write(gitdir.join("commondir"), "../..\n").expect("common dir");
                std::fs::write(
                    root.0.join(".git"),
                    format!("gitdir: {}\n", gitdir.display()),
                )
                .expect("worktree marker");
                root.0.join("metadata/info/exclude")
            }
            "jj" => {
                std::fs::create_dir(root.0.join(".jj")).expect("jj marker");
                root.0.join(".gitignore")
            }
            "nested" => {
                std::fs::create_dir(scope.join("hidden/.git")).expect("nested repository");
                scope.join("hidden/.gitignore")
            }
            _ => unreachable!(),
        };
        let ignored = if kind == "nested" {
            "b.ts\n"
        } else {
            "scope/hidden/\n"
        };
        std::fs::write(&rules, ignored).expect("ignore rule");
        let (paths, security) = simple_policy(&root.0);
        fixtures.push((kind, root, scope, rules, paths, security));
    }
    settle();
    for (kind, _root, scope, rules, paths, security) in fixtures {
        let first = run_row(&query(&scope, false), &paths, &security);
        assert_eq!(first["matchCount"], 2, "{kind}: {first}");
        let snapshot = first["snapshot"].as_str().expect("snapshot");
        assert!(SCANS.holds(snapshot), "a repository scan is stored: {kind}");
        let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
        let unchanged = run_row(&next, &paths, &security);
        assert_eq!(unchanged["matchCount"], 2, "{kind}: {unchanged}");
        assert!(SCANS.holds(snapshot), "the continuation reuses it: {kind}");
        SCANS.evict(snapshot);
        assert_eq!(
            unchanged,
            run_row(&next, &paths, &security),
            "stored and fresh pages are identical: {kind}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&rules, "").expect("remove ignore rule");
        let changed = run_row(&next, &paths, &security);
        assert_eq!(changed["errorCode"], "staleSnapshot", "{kind}: {changed}");
        assert_eq!(
            run_row(&query(&scope, false), &paths, &security)["matchCount"],
            3
        );
    }
}

#[test]
fn include_ignored_keeps_settled_repository_memo_reuse() {
    let root = Fixture::new();
    let scope = sources(&root.0);
    std::fs::create_dir(root.0.join(".git")).expect("git marker");
    let rules = root.0.join(".gitignore");
    std::fs::write(&rules, "scope/hidden/\n").expect("parent ignore");
    let (paths, security) = simple_policy(&root.0);
    settle();
    let first = run_row(&query(&scope, true), &paths, &security);
    assert_eq!(first["matchCount"], 3, "{first}");
    let snapshot = first["snapshot"].as_str().expect("snapshot");
    assert!(
        SCANS.holds(snapshot),
        "ignored rules cannot affect this selection"
    );
    let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    let unchanged = run_row(&next, &paths, &security);
    std::fs::write(&rules, "").expect("change ignored dependency");
    assert_eq!(unchanged, run_row(&next, &paths, &security));
    assert!(
        SCANS.holds(snapshot),
        "unrelated ancestor metadata must not evict"
    );
}

#[cfg(unix)]
#[test]
fn ignore_symlinks_never_reuse_unobserved_rules() {
    for include_ignored in [false, true] {
        let root = Fixture::new();
        let scope = sources(&root.0);
        let rules = root.0.join("outside-rules");
        std::fs::write(&rules, "hidden/\n").expect("external ignore rule");
        std::os::unix::fs::symlink(&rules, scope.join(".ignore")).expect("ignore symlink");
        // The source scope is allowed; the symlink target is outside it.
        let (paths, security) = simple_policy(&scope);
        settle();
        let first = run_row(&query(&scope, include_ignored), &paths, &security);
        assert_eq!(
            first["matchCount"],
            if include_ignored { 3 } else { 2 },
            "{first}"
        );
        let snapshot = first["snapshot"].as_str().expect("snapshot");
        let held_before = SCANS.holds(snapshot);
        let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
        let unchanged = run_row(&next, &paths, &security);
        assert_eq!(unchanged["matchCount"], first["matchCount"]);
        std::fs::write(&rules, "").expect("edit unobserved ignore target");
        let changed = run_row(&next, &paths, &security);
        if include_ignored {
            assert!(
                held_before,
                "includeIgnored must retain actual settled reuse"
            );
            assert_eq!(unchanged, changed);
            assert!(SCANS.holds(snapshot));
        } else {
            // On the old implementation this fails after proving held_before=true.
            assert_eq!(
                changed["errorCode"], "staleSnapshot",
                "held_before={held_before}: {changed}"
            );
            assert!(
                !held_before,
                "unobservable ignore rules deliberately disable caching"
            );
            assert!(!SCANS.holds(snapshot));
            assert_eq!(
                run_row(&query(&scope, false), &paths, &security)["matchCount"],
                3
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn broken_ignore_symlinks_do_not_pin_absent_external_rules() {
    let root = Fixture::new();
    let scope = sources(&root.0);
    let rules = root.0.join("future-rules");
    std::os::unix::fs::symlink(&rules, scope.join(".ignore")).expect("broken ignore link");
    let (paths, security) = simple_policy(&scope);
    settle();
    let first = run_row(&query(&scope, false), &paths, &security);
    assert_eq!(first["matchCount"], 3, "{first}");
    let snapshot = first["snapshot"].as_str().expect("snapshot");
    assert!(
        !SCANS.holds(snapshot),
        "an unobservable rule is not cacheable"
    );
    let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    assert_eq!(run_row(&next, &paths, &security)["matchCount"], 3);
    std::fs::write(&rules, "hidden/\n").expect("create external rules");
    let changed = run_row(&next, &paths, &security);
    assert_eq!(changed["errorCode"], "staleSnapshot", "{changed}");
    assert_eq!(
        run_row(&query(&scope, false), &paths, &security)["matchCount"],
        2
    );
}

#[test]
fn implicit_grammar_filters_do_not_override_file_ignore_rules() {
    for name in [".ignore", ".gitignore"] {
        for language in [None, Some("typescript")] {
            let root = Fixture::new();
            let scope = sources(&root.0);
            std::fs::create_dir(root.0.join(".git")).expect("repository marker");
            std::fs::write(scope.join(name), "hidden/b.ts\n").expect("file ignore rule");
            let (paths, security) = simple_policy(&root.0);
            let mut row = query(&scope, false);
            if let Some(language) = language {
                row["language"] = json!(language);
            }
            let filtered = run_row(&row, &paths, &security);
            assert_eq!(filtered["matchCount"], 2, "{name}/{language:?}: {filtered}");
            row["includeIgnored"] = json!(true);
            assert_eq!(run_row(&row, &paths, &security)["matchCount"], 3);
            row["includeIgnored"] = json!(false);
            row["include"] = json!(["**/*.ts"]);
            assert_eq!(
                run_row(&row, &paths, &security)["matchCount"],
                3,
                "the caller's explicit include override keeps its existing semantics"
            );
        }
    }
}
