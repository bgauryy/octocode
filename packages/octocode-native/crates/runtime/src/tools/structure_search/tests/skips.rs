//! Skipped entries: dot entries, default-pruned and ignored directories,
//! withheld entries, walk warnings, and the leads that lift each.

use super::*;

/// A tree without `hidden` skips dot entries, but says how many it skipped
/// and, on a filtered listing, offers the one retry that lifts every cause it
/// skipped by (`includeSkipped`); `.git`, which the path policy withholds,
/// stays out of the count.
#[test]
fn tree_discloses_skipped_dot_entries_and_offers_include_hidden() {
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
    // Every counted dot entry is reachable: the lead lifts includeHidden.
    assert_eq!(skipped_count(&out, "hidden"), 3, "{out}");
    let lead = &related(&out, "structureSearch")["input"]["queries"][0];
    assert_eq!(lead["includeHidden"], true, "{out}");
    assert!(lead.get("defaultExcludes").is_none(), "{out}");
    let filtered = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":2,"extensions":["rs"]}),
    )
    .expect("filtered tree");
    // The lead is the fix: the summary keeps the count, not the flag.
    assert!(skipped_count(&filtered, "hidden") > 0, "{filtered}");
    let retry = &related(&filtered, "structureSearch");
    assert_eq!(retry["tool"], "structureSearch", "{out}");
    assert_eq!(retry["input"]["queries"][0]["includeHidden"], true, "{out}");
    assert!(
        retry["input"]["queries"][0]
            .get("defaultExcludes")
            .is_none()
            && retry["input"]["queries"][0].get("includeIgnored").is_none(),
        "only the cause it hit: {out}"
    );
    assert!(
        retry["input"]["queries"][0].get("snapshot").is_none(),
        "{out}"
    );
    let all = run(&root.0, retry["input"]["queries"][0].clone()).expect("retry");
    assert_eq!(paths(&all), ["./src/lib.rs"], "{all}");
    assert!(related(&all, "structureSearch").is_null(), "{all}");
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
    // One entry for the cause, its first directory, counting both.
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
        files["hints"][0]
            .as_str()
            .is_some_and(|h| h.contains("build/")),
        "{files}"
    );
    let retry = &related(&files, "structureSearch")["input"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], false, "{files}");
    assert!(retry.get("snapshot").is_none(), "{files}");
    let found = run(&root.0, retry.clone()).expect("retry");
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
    crate::tools::test_support::checks::assert_absent(&out, "withheld");
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
/// in one hop: the retry lifts both the ignore rules and the prune, and the
/// hint names the directory.
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
    let retry = related(&out, "structureSearch")["input"]["queries"][0].clone();
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert_eq!(retry["includeIgnored"], true, "{out}");
    assert!(
        out["hints"][0]
            .as_str()
            .is_some_and(|h| h.contains("target/")),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out["warnings"], "target/");
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

/// A filtered listing whose walk pruned a default-excluded directory leads
/// to the same listing with the prune lifted (outside a git work tree a
/// listed name is pruned by name), and the lead reaches what it hid.
#[test]
fn a_filtered_listing_leads_past_pruned_directories() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/coverage")).expect("dir");
    std::fs::write(root.0.join("src/coverage/pass.rs"), "x\n").expect("pruned");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("visible");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"extensions":["rs"],"maxDepth":10}),
        )
        .expect("listing");
        assert!(
            !out["files"].to_string().contains("pass.rs"),
            "{operation}: {out}"
        );
        let lead = related(&out, "structureSearch")["input"]["queries"][0].clone();
        assert_eq!(lead["defaultExcludes"], false, "{operation}: {out}");
        let all = run(&root.0, lead).expect("lead");
        assert!(
            all["files"].to_string().contains("pass.rs"),
            "{operation}: the lead reaches the pruned file: {all}"
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
/// so an ignored `target/` is behind both rules: the summary names it among
/// the pruned directories, and one lead lifts both causes. Following it
/// reaches the files there and leaves no second widening (no loop).
#[test]
fn an_ignored_default_pruned_directory_is_named_and_reached_in_one_step() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("target/debug")).expect("target");
    std::fs::write(root.0.join("target/debug/gen.rs"), "x\n").expect("built");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    for extensions in [json!(["rs"]), json!(["rlib"])] {
        if extensions == json!(["rlib"]) {
            std::fs::write(root.0.join("target/debug/a.rlib"), "x\n").expect("rlib");
        }
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"extensions":extensions}),
        )
        .expect("listing");
        assert_eq!(skipped_names(&out, "default-excluded"), ["target"], "{out}");
        let lead = related(&out, "structureSearch")["input"]["queries"][0].clone();
        assert_eq!(lead["includeIgnored"], true, "{out}");
        assert_eq!(lead["defaultExcludes"], false, "{out}");
        let all = run(&root.0, lead.clone()).expect("lead");
        assert!(
            listed(&all).iter().any(|row| row["path"]
                .as_str()
                .is_some_and(|p| p.contains("target/debug"))),
            "{all}"
        );
        assert_eq!(skipped_count(&all, "default-excluded"), 0, "{all}");
        assert_eq!(skipped_count(&all, "ignored"), 0, "{all}");
        // What is left is a cause the first step did not lift (the hidden
        // `.gitignore`), never the same widening again.
        if let Some(again) = related(&all, "structureSearch")["input"]["queries"][0].as_object() {
            let mut again = again.clone();
            assert_eq!(again.remove("includeHidden"), Some(json!(true)), "{all}");
            let mut first = lead.as_object().cloned().expect("lead");
            first.remove("includeHidden");
            assert_eq!(again, first, "{all}");
        }
    }
}
