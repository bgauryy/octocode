//! Skipped entries: dot entries, default-pruned and ignored directories,
//! withheld entries, walk warnings, and the one call an empty listing offers
//! to lift them.

use super::*;
use octocode_test_support::json_fixture;

/// A tree without `hidden` skips dot entries and counts how many it skipped
/// in `skippedPaths`; a listing with entries, filtered or not, offers no
/// call. `.git`, which the path policy withholds, stays out of the count,
/// and `includeHidden` lists what the count names.
#[test]
fn tree_counts_skipped_dot_entries_and_offers_no_call() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git");
    std::fs::create_dir_all(root.0.join(".github")).expect("github");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join(".editorconfig"), "x\n").expect("dotfile");
    std::fs::write(root.0.join("src/.keep"), "").expect("nested dotfile");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":2}),
    )
    .expect("tree");
    assert!(
        paths(&out).iter().any(|path| path.ends_with("/src/lib.rs")),
        "{out}"
    );
    assert!(!out["files"].to_string().contains(".keep"), "{out}");
    assert_eq!(skipped_count(&out, "hidden"), 3, "{out}");
    assert_listing_guides_nothing(&out);
    let query = json!({"operation":"tree","path":root.0,"maxDepth":2,"extensions":["rs"]});
    let filtered = run(&root.0, query.clone()).expect("filtered tree");
    assert!(skipped_count(&filtered, "hidden") > 0, "{filtered}");
    assert_listing_guides_nothing(&filtered);
    // Every counted dot entry is reachable: `includeHidden` lifts the cause.
    let all = run(&root.0, lifting(query, json!({"includeHidden": true}))).expect("lifted");
    assert_eq!(paths(&all), ["./src/lib.rs"], "{all}");
    assert_listing_guides_nothing(&all);
    assert_eq!(skipped_count(&all, "hidden"), 0, "{all}");
}

/// Default-pruned directories are disclosed: the tree summary names them,
/// and an empty listing leads to the same listing with them walked.
#[test]
fn default_pruned_directories_are_named_and_an_empty_listing_walks_them() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("build/gen")).expect("build");
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("deps");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("build/gen/electron.ts"), "x\n").expect("generated");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");

    let tree = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    // One entry per rule, each naming its directory: both are counted.
    let names = skipped_names(&tree, "default-excluded");
    assert_eq!(names, ["build", "node_modules"], "{tree}");
    assert_eq!(skipped_count(&tree, "default-excluded"), 2, "{tree}");
    assert!(related(&tree, "structureSearch").is_null(), "{tree}");

    let files = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert!(skipped_count(&files, "default-excluded") > 0, "{files}");
    assert!(
        skipped_names(&files, "default-excluded").contains(&"build".to_owned()),
        "{files}"
    );
    let retry = skipped_recovery(&files);
    assert_eq!(retry["defaultExcludes"], false, "{files}");
    assert!(retry.get("snapshot").is_none(), "{files}");
    let found = run(&root.0, retry).expect("retry");
    assert_eq!(listed(&found).len(), 1, "{found}");
}

/// The policy-withheld warning only counts entries the query's filters
/// could have listed.
#[test]
fn withheld_entries_count_only_when_the_filters_could_list_them() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    std::fs::write(root.0.join("main.go"), "package main\n").expect("go");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["go"],"includeIgnored":true}),
    )
    .expect("listing");
    assert_eq!(listed(&out).len(), 1, "{out}");
    octocode_runtime_base::test_support::checks::assert_absent(&out, "withheld");
}

/// The withheld count applies the walk's own glob dialect: `*` never crosses
/// `/`, so a withheld entry one directory below what the glob reaches is not
/// one the listing could have shown.
#[test]
fn withheld_entries_follow_the_walks_glob_dialect() {
    let root = Fixture::new();
    root.write("cfg/.env.production", "DUMMY_VALUE=placeholder\n");
    root.write("cfg/deep/.env.production", "DUMMY_VALUE=placeholder\n");
    root.write("cfg/app.production", "x\n");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["cfg/*.production"],"includeIgnored":true,"includeHidden":true}),
    )
    .expect("listing");
    let paths = listed(&out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert!(
        paths.len() == 1 && paths[0].ends_with("cfg/app.production"),
        "{out}"
    );
    let count = skipped_count(&out, "withheld");
    assert_eq!(count, 1, "only cfg/.env.production is counted: {out}");
}

/// A gitignored directory named like a default-pruned one is lifted
/// in one hop: the retry lifts both the ignore rules and the prune, and
/// `skippedPaths` names the directory.
#[test]
fn include_ignored_lead_lifts_gitignore_and_prune_in_one_hop() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("target/debug")).expect("target");
    std::fs::write(root.0.join("target/debug/x.rlib"), "rlib\n").expect("rlib");
    std::fs::write(root.0.join("main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["*.rlib"]}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    let retry = skipped_recovery(&out);
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert_eq!(retry["includeIgnored"], true, "{out}");
    assert_eq!(
        skipped_names(&out, "ignored-default-excluded"),
        ["target"],
        "{out}"
    );
    octocode_runtime_base::test_support::checks::assert_absent(&out["hints"], "target/");
    let found = run(&root.0, retry).expect("retry");
    assert!(
        found["files"]
            .as_array()
            .is_some_and(|dirs| dirs.iter().any(|dir| {
                dir["files"].as_array().is_some_and(|names| {
                    names
                        .iter()
                        .any(|name| name.as_str().is_some_and(|n| n.starts_with("x.rlib")))
                })
            })),
        "{found}"
    );
}

/// An empty files listing names withheld entries once (`skippedPaths`).
#[test]
fn files_empty_withheld_not_duplicated() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".aws")).expect("aws");
    std::fs::write(root.0.join(".aws/a.ts"), "x\n").expect("secret");
    std::fs::write(root.0.join("b.rs"), "x\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"],"includeHidden":true}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    let reasons = skipped_reasons(&out, "withheld");
    assert_eq!(reasons, [("security-policy dir .aws/".into(), 1)], "{out}");
    assert!(
        !out.to_string().contains("withheld by path policy"),
        "{out}"
    );
}

/// A paged tree states the walk's withheld entries once, on its first page,
/// like its summary; later pages carry only their own warnings.
#[test]
fn walk_warnings_are_stated_on_the_first_page_only() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".npmrc"), "token\n").expect("credential");
    for n in 0..6 {
        std::fs::write(root.0.join(format!("f{n}.txt")), "x\n").expect("file");
    }
    let pages = walk_pages_budget(
        &root.0,
        json!({"operation":"tree","path":root.0,"includeHidden":true}),
        Some(1),
    );
    assert!(pages.len() > 1, "{pages:?}");
    let withheld = |page: &Value| skipped_count(page, "withheld") > 0;
    assert!(withheld(&pages[0]), "{}", pages[0]);
    assert!(pages[1..].iter().all(|page| !withheld(page)), "{pages:?}");
}

/// A filtered listing whose walk pruned a default-excluded directory names
/// it in `skippedPaths` (outside a git work tree a listed name is pruned by
/// name) and, having entries, offers no call; the same listing with the
/// prune lifted reaches what it hid.
#[test]
fn a_filtered_listing_names_pruned_directories_and_offers_no_call() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/coverage")).expect("dir");
    std::fs::write(root.0.join("src/coverage/pass.rs"), "x\n").expect("pruned");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("visible");
    for operation in ["tree", "files"] {
        let query = json!({"operation":operation,"path":root.0,"extensions":["rs"],"maxDepth":10});
        let out = run(&root.0, query.clone()).expect("listing");
        assert!(
            !out["files"].to_string().contains("pass.rs"),
            "{operation}: {out}"
        );
        assert_eq!(
            skipped_names(&out, "default-excluded"),
            ["coverage"],
            "{operation}: {out}"
        );
        assert_listing_guides_nothing(&out);
        let all = run(&root.0, lifting(query, json!({"defaultExcludes": false}))).expect("lifted");
        assert!(
            all["files"].to_string().contains("pass.rs"),
            "{operation}: the lifted listing reaches the pruned file: {all}"
        );
    }
}

/// `files` shares every local walk's hidden default: dot entries stay out
/// unless `includeHidden`, and the listing counts what it hid.
#[test]
fn files_hide_dot_entries_unless_include_hidden() {
    let root = Fixture::new();
    root.write(".cfg/a.ts", "x\n");
    root.write("b.ts", "x\n");
    let query = |hidden: bool| {
        let mut query = json!({"operation":"files","path":root.0,"extensions":["ts"]});
        if hidden {
            query["includeHidden"] = json!(true);
        }
        run(&root.0, query).expect("files")
    };
    let default = query(false);
    let paths = listed(&default)
        .iter()
        .filter_map(|entry| entry["path"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(paths.len() == 1 && paths[0].ends_with("b.ts"), "{default}");
    assert_eq!(skipped_count(&default, "hidden"), 1, "{default}");
    assert_eq!(listed(&query(true)).len(), 2);
}
/// In a git work tree the default prune skips only directories git ignores,
/// so an ignored `target/` is behind both rules: `skippedPaths` names it
/// once with both causes, and lifting both in one step (the one call an
/// empty listing offers; a listing with entries offers none) reaches the
/// files there and leaves no second widening (no loop).
#[test]
fn an_ignored_default_pruned_directory_is_named_and_reached_in_one_step() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("target/debug")).expect("target");
    std::fs::write(root.0.join("target/debug/gen.rs"), "x\n").expect("built");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    for extensions in [json_fixture(r#"["rs"]"#), json_fixture(r#"["rlib"]"#)] {
        if extensions == json_fixture(r#"["rlib"]"#) {
            std::fs::write(root.0.join("target/debug/a.rlib"), "x\n").expect("rlib");
        }
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"extensions":extensions.clone()}),
        )
        .expect("listing");
        assert_eq!(
            skipped_names(&out, "ignored-default-excluded"),
            ["target"],
            "{out}"
        );
        assert!(skipped_names(&out, "ignored").is_empty(), "{out}");
        assert!(skipped_names(&out, "default-excluded").is_empty(), "{out}");
        let query = json!({"operation":"files","path":root.0,"extensions":extensions});
        let lead = if out["status"] == "empty" {
            skipped_recovery(&out)
        } else {
            assert_listing_guides_nothing(&out);
            lifting(
                query,
                json!({"includeIgnored": true, "defaultExcludes": false}),
            )
        };
        assert_eq!(lead["includeIgnored"], true, "{out}");
        assert_eq!(lead["defaultExcludes"], false, "{out}");
        let all = run(&root.0, lead).expect("lead");
        assert!(
            listed(&all).iter().any(|row| row["path"]
                .as_str()
                .is_some_and(|p| p.contains("target/debug"))),
            "{all}"
        );
        assert_eq!(skipped_count(&all, "default-excluded"), 0, "{all}");
        assert_eq!(skipped_count(&all, "ignored"), 0, "{all}");
        // The lifted listing has entries: it offers no widening again.
        assert_listing_guides_nothing(&all);
    }
}

/// A gitignored `node_modules/` is behind both rules, and its one entry
/// states the rule that left it out, as a default-excluded entry does: the
/// ignore rule names no file here, so the default prune's (`node_modules/`).
#[test]
fn an_ignored_default_pruned_directory_states_its_rule() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "pkg/node_modules/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("pkg/node_modules/lib")).expect("modules");
    std::fs::write(root.0.join("pkg/node_modules/lib/index.js"), "x\n").expect("module");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/main.js"), "x\n").expect("main");
    let out = run(&root.0, json!({"operation":"files","path":root.0})).expect("listing");
    assert_eq!(
        skipped_reasons(&out, "ignored-default-excluded"),
        [("node_modules/".to_owned(), 1)],
        "{out}"
    );
}

/// A directory git ignores that the default prune also names is skipped for
/// both: `skippedPaths` lists it once, `ignored-default-excluded`, never once
/// per cause.
#[test]
fn a_directory_skipped_for_two_causes_is_listed_once_with_both() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "build/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("runtime/build/inner")).expect("build");
    std::fs::write(root.0.join("runtime/build/b.rs"), "x\n").expect("built");
    std::fs::create_dir_all(root.0.join("runtime/src/x")).expect("src");
    std::fs::write(root.0.join("runtime/src/x/a.rs"), "x\n").expect("source");
    for query in [
        json!({"operation":"tree","path":root.0,"maxDepth":3,"entryType":"directory"}),
        json!({"operation":"tree","path":root.0,"maxDepth":3}),
        json!({"operation":"files","path":root.0,"extensions":["rs"]}),
    ] {
        let out = run(&root.0, query).expect("listing");
        let named = out["skippedPaths"]["entries"]
            .as_array()
            .expect("skipped paths")
            .iter()
            .filter(|entry| {
                entry["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("runtime/build"))
            })
            .map(|entry| entry["cause"].clone())
            .collect::<Vec<_>>();
        assert_eq!(named, [json!("ignored-default-excluded")], "{out}");
    }
}
