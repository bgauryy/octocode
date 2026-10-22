//! A scan that stops at the walk ceiling names every scope it never read as a
//! `narrowScope` call, and the calls together reach every unread file. The
//! ceiling is lowered so a small tree reaches it.
use super::{AstSearchQuery, execute_ast, unwalked};
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const WINDOW: usize = 65_536;

/// Every row of `out` the leads of a terminal page name.
fn leads(out: &Value) -> Vec<(String, Value)> {
    out["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| key.starts_with("narrowScope"))
        .map(|(key, lead)| (key.clone(), lead.clone()))
        .collect()
}

fn run(root: &std::path::Path, row: &Value) -> Value {
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(row.clone()).expect("typed row");
    execute_ast(&query, &paths, &security, &NeverCancel, Some(WINDOW))
        .unwrap_or_else(|error| panic!("{row} failed: {error:?}"))
}

/// The file names a match or symbols page lists, one per row.
fn names(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["path"].as_str())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect()
}

fn listed(out: &Value) -> BTreeSet<String> {
    names(out).into_iter().collect()
}

/// The files `row` reaches, then every file the astSearch leads of its
/// terminal pages reach, recursively, once per call that lists them (sorted);
/// the calls it made, in order.
fn reached(root: &std::path::Path, row: &Value, calls: &mut Vec<Value>) -> Vec<String> {
    let mut files = reached_unsorted(root, row, calls);
    files.sort();
    files
}

fn reached_unsorted(root: &std::path::Path, row: &Value, calls: &mut Vec<Value>) -> Vec<String> {
    calls.push(row.clone());
    let out = run(root, row);
    let mut files = names(&out);
    for (key, lead) in leads(&out) {
        if lead["tool"] == "astSearch" {
            let next = &lead["query"]["queries"][0];
            assert!(next.get("snapshot").is_none(), "{key}: {lead}");
            files.extend(reached_unsorted(root, next, calls));
        }
    }
    files
}

/// `root` holds eleven candidate files (and an ignored and a pruned one):
///
/// ```text
/// a/a1 a/a2  b/b1 b/deep/d1 b/deep/d2  c/c1 c/c2 c/c3  e/e1  root1 root2
/// ```
fn tree(root: &Fixture) {
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "ignored/\nnode_modules/\n");
    for (index, path) in [
        "a/a1.ts",
        "a/a2.ts",
        "b/b1.ts",
        "b/deep/d1.ts",
        "b/deep/d2.ts",
        "c/c1.ts",
        "c/c2.ts",
        "c/c3.ts",
        "e/e1.ts",
        "root1.ts",
        "root2.ts",
        "ignored/x.ts",
        "node_modules/n.ts",
    ]
    .into_iter()
    .enumerate()
    {
        root.write(path, format!("export const value{index} = {index};\n"));
    }
}

const EVERY_FILE: [&str; 11] = [
    "a1.ts", "a2.ts", "b1.ts", "d1.ts", "d2.ts", "c1.ts", "c2.ts", "c3.ts", "e1.ts", "root1.ts",
    "root2.ts",
];

/// `files` sorted, each name once and only once.
fn sorted<const N: usize>(files: [&str; N]) -> Vec<String> {
    let mut files = files.map(str::to_owned).to_vec();
    files.sort();
    files
}

fn every_file() -> Vec<String> {
    sorted(EVERY_FILE)
}

fn match_row(root: &std::path::Path, extra: Value) -> Value {
    let mut row = json!({
        "operation":"match",
        "path":root.to_string_lossy(),
        "pattern":"export const $A = $B",
    });
    row.as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    row
}

fn symbols_row(root: &std::path::Path, extra: Value) -> Value {
    let mut row = json!({"operation":"symbols","path":root.to_string_lossy()});
    row.as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    row
}

/// The path of a lead's query, relative to `root`.
fn lead_path(root: &std::path::Path, lead: &Value) -> String {
    let path = lead["query"]["queries"][0]["path"].as_str().expect("path");
    std::path::Path::new(path)
        .strip_prefix(root.canonicalize().expect("root"))
        .map_or_else(
            |_| path.to_owned(),
            |relative| relative.to_string_lossy().into_owned(),
        )
}

/// What each operation reaches past the ceiling: the page that stops there
/// is terminal and honest about it, one `narrowScope` per unread directory
/// plus one for the unread files of a directory, and the calls together
/// list every candidate (an unread directory that exceeds the ceiling again
/// leads on in the same way).
type Build = fn(&std::path::Path, Value) -> Value;

fn leads_reach_every_unread_file(build: Build) {
    let root = Fixture::new();
    tree(&root);
    let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
    let first = run(&root.0, &build(&root.0, json!({})));
    // The walk read five of the eleven candidates, in name order.
    assert_eq!(
        listed(&first),
        ["a1.ts", "a2.ts", "b1.ts", "d1.ts", "d2.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "{first}"
    );
    assert_eq!(first["terminalLimit"], json!(true), "{first}");
    assert_eq!(first["partialReasons"], json!(["scanBudget"]), "{first}");
    assert!(first["next"].get("expandScan").is_none(), "{first}");
    let found = leads(&first);
    // c's own files, the subdirectory e, and the root's own files.
    let paths: Vec<_> = found
        .iter()
        .map(|(_, lead)| lead_path(&root.0, lead))
        .collect();
    assert_eq!(paths, ["c", "e", ""], "{first}");
    let keys: Vec<_> = found.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["narrowScope", "narrowScope2", "narrowScope3"]);
    // A scope of a directory's own files leaves its subdirectories out and
    // reads only what the scan did not.
    let files_only = |index: usize| -> BTreeSet<String> {
        let row = found[index].1["query"]["queries"][0].clone();
        assert_eq!(row["exclude"], json!(["/*/"]), "{row}");
        listed(&run(&root.0, &row))
    };
    assert_eq!(
        files_only(2),
        ["root1.ts", "root2.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    let mut calls = Vec::new();
    let all = reached(&root.0, &build(&root.0, json!({})), &mut calls);
    assert_eq!(all, every_file(), "each file once: {calls:?}");
}

#[test]
fn match_leads_reach_every_file_the_walk_ceiling_left_unread() {
    leads_reach_every_unread_file(match_row);
}

#[test]
fn symbols_leads_reach_every_file_the_walk_ceiling_left_unread() {
    leads_reach_every_unread_file(symbols_row);
}

/// A directory with more candidates than the ceiling has no narrower path:
/// the page says so with a typed reason and names the directory, and offers
/// no call that could not reach its files.
#[test]
fn a_directory_over_the_ceiling_is_a_typed_terminal_limit() {
    let builds: [Build; 2] = [match_row, symbols_row];
    for build in builds {
        let root = Fixture::new();
        for index in 0..7 {
            root.write(&format!("flat/f{index}.ts"), "export const v = 1;\n");
        }
        let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
        let out = run(&root.0, &build(&root.0, json!({})));
        assert_eq!(out["terminalLimit"], json!(true), "{out}");
        assert_eq!(
            out["partialReasons"],
            json!(["scanBudget", "flatDirectory"]),
            "{out}"
        );
        let warnings = out["warnings"].to_string();
        assert!(
            warnings.contains("flat") && warnings.contains("7 files"),
            "{out}"
        );
        assert!(leads(&out).is_empty(), "{out}");
    }
}

/// A glob that matches by position names no scope: another `path` would
/// change what it matches, so the page says why it offers none.
#[test]
fn an_anchored_glob_gets_no_narrowed_scope() {
    let cases: [(Build, &str); 2] = [(match_row, "include"), (symbols_row, "languageGlobs")];
    for (build, glob) in cases {
        let root = Fixture::new();
        tree(&root);
        let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
        let extra = if glob == "include" {
            json!({"include":["/a/**", "/b/**", "/c/**", "/e/**", "/*.ts"]})
        } else {
            json!({"languageGlobs":{"typescript":["/**/*.ts"]}})
        };
        let out = run(&root.0, &build(&root.0, extra));
        assert_eq!(out["terminalLimit"], json!(true), "{out}");
        assert!(leads(&out).is_empty(), "{out}");
        assert!(out["warnings"].to_string().contains("by position"), "{out}");
    }
}

/// Past the lead cap the remaining scopes are counted and one
/// structureSearch lead lists them: nothing is dropped silently.
#[test]
fn scopes_past_the_lead_cap_are_counted_and_listed() {
    let root = Fixture::new();
    for index in 0..12 {
        root.write(&format!("d{index:02}/f.ts"), "export const v = 1;\n");
    }
    let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(1);
    let out = run(&root.0, &match_row(&root.0, json!({})));
    let found = leads(&out);
    let cap = unwalked::MAX_LEADS;
    assert_eq!(found.len(), cap + 1, "{out}");
    assert_eq!(found[cap].1["tool"], "structureSearch", "{out}");
    assert!(
        out["warnings"].to_string().contains("3 more unread scopes"),
        "{out}"
    );
}

/// `maxDepth` counts from the scope's own root, and a scope the depth never
/// reaches is not a lead.
#[test]
fn a_narrowed_scope_rebases_max_depth() {
    let query = json!({"operation":"match","path":"/r","pattern":"x","maxDepth":3});
    let reach = |depth, files_only| unwalked::Reach {
        dir: format!("/r/{depth}"),
        depth,
        files_only,
        skip: 0,
    };
    let depth_of = |reach: &unwalked::Reach| {
        unwalked::narrowed(&query, reach)
            .map(|lead| lead["query"]["queries"][0]["maxDepth"].clone())
    };
    assert_eq!(depth_of(&reach(0, true)), Some(json!(3)));
    assert_eq!(depth_of(&reach(2, false)), Some(json!(1)));
    assert_eq!(depth_of(&reach(3, false)), None);
}

/// Every call of `build`'s walk past a ceiling of `ceiling` files, and the
/// files they list, each as often as a call lists it (sorted).
fn walk_all(
    root: &Fixture,
    build: Build,
    extra: Value,
    ceiling: usize,
) -> (Value, Vec<Value>, Vec<String>) {
    let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(ceiling);
    let row = build(&root.0, extra);
    let first = run(&root.0, &row);
    let mut calls = Vec::new();
    let files = reached(&root.0, &row, &mut calls);
    (first, calls, files)
}

/// The directories (relative to `root`) the narrowed calls of `first` read,
/// in lead order.
fn lead_dirs(root: &Fixture, first: &Value) -> Vec<String> {
    leads(first)
        .iter()
        .map(|(_, lead)| lead_path(&root.0, lead))
        .collect()
}

/// Files that sort before the child the walk stopped in were read by the cut
/// scan, those after it were not: a directory's own-files scope skips the
/// read ones, so every candidate is listed by exactly one call, at every
/// level of a nested tree.
fn leads_skip_what_the_cut_scan_read(build: Build) {
    let root = Fixture::new();
    for path in [
        "0first.ts",
        "a/a1.ts",
        "a/a2.ts",
        "b/a.txt",
        "b/b0.ts",
        "b/deep/d1.ts",
        "b/deep/d2.ts",
        "b/notes.txt",
        "b/z.ts",
        "c/c1.ts",
        "c/c2.ts",
        "root1.ts",
    ] {
        root.write(path, "export const value = 1;\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 5);
    assert_eq!(
        listed(&first),
        ["0first.ts", "a1.ts", "a2.ts", "b0.ts", "d1.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "{first}"
    );
    // The frontier's own directory, the one above it, the unread sibling,
    // and the root.
    assert_eq!(
        lead_dirs(&root, &first),
        ["b/deep", "b", "c", ""],
        "{first}"
    );
    // Each own-files scope skips the candidates the cut scan read before
    // it: one in `b/deep` (d1), one in `b` (b0; a.txt is no candidate), one
    // in the root (0first).
    let skips: Vec<_> = leads(&first)
        .iter()
        .map(|(_, lead)| lead["query"]["queries"][0]["scanOffset"].clone())
        .collect();
    assert_eq!(
        skips,
        [json!(1), json!(1), Value::Null, json!(1)],
        "{first}"
    );
    assert_eq!(
        files,
        sorted([
            "0first.ts",
            "a1.ts",
            "a2.ts",
            "b0.ts",
            "d1.ts",
            "d2.ts",
            "z.ts",
            "c1.ts",
            "c2.ts",
            "root1.ts"
        ]),
        "each file once: {calls:?}"
    );
}

#[test]
fn match_leads_list_each_file_once_when_files_sort_before_the_frontier() {
    leads_skip_what_the_cut_scan_read(match_row);
}

#[test]
fn symbols_leads_list_each_file_once_when_files_sort_before_the_frontier() {
    leads_skip_what_the_cut_scan_read(symbols_row);
}

/// A file the scan cannot parse is no candidate: it neither costs a lead
/// nor counts toward a directory's size. With `languageGlobs`, a file the
/// map names is one.
fn unparseable_files_are_not_candidates(build: Build) {
    let root = Fixture::new();
    for path in ["x/x1.ts", "x/x2.ts", "y/y1.ts", "y/y2.ts", "zz.txt"] {
        root.write(path, "export const value = 1;\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 2);
    // `zz.txt` is the only unread file of the root, and no scan parses it.
    assert_eq!(lead_dirs(&root, &first), ["y"], "{first}");
    assert_eq!(
        files,
        sorted(["x1.ts", "x2.ts", "y1.ts", "y2.ts"]),
        "{calls:?}"
    );

    // A function too: the outline reads a mapped file's functions.
    root.write(
        "zz.foo",
        "export const value = 2;\nexport function mapped() {}\n",
    );
    let mapped = json!({"languageGlobs":{"typescript":["*.foo"]}});
    let (first, calls, files) = walk_all(&root, build, mapped, 2);
    assert_eq!(lead_dirs(&root, &first), ["y", ""], "{first}");
    assert_eq!(
        files,
        sorted(["x1.ts", "x2.ts", "y1.ts", "y2.ts", "zz.foo"]),
        "{calls:?}"
    );
}

#[test]
fn match_ignores_unparseable_files_when_it_names_unread_scopes() {
    unparseable_files_are_not_candidates(match_row);
}

#[test]
fn symbols_ignores_unparseable_files_when_it_names_unread_scopes() {
    unparseable_files_are_not_candidates(symbols_row);
}

/// Only candidates make a directory too big for the walk: many unparseable
/// files beside a few candidates leave a scope that reads them.
fn unparseable_files_do_not_make_a_directory_flat(build: Build) {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(&format!("a/a{index}.ts"), "export const v = 1;\n");
    }
    root.write("b/b1.ts", "export const v = 1;\n");
    for index in 0..8 {
        root.write(&format!("b/data{index}.txt"), "not code\n");
    }
    let (first, calls, files) = walk_all(&root, build, json!({}), 3);
    assert_eq!(first["partialReasons"], json!(["scanBudget"]), "{first}");
    assert_eq!(lead_dirs(&root, &first), ["b"], "{first}");
    assert_eq!(
        files,
        sorted(["a1.ts", "a2.ts", "a3.ts", "b1.ts"]),
        "{calls:?}"
    );
}

#[test]
fn match_counts_only_candidates_toward_a_flat_directory() {
    unparseable_files_do_not_make_a_directory_flat(match_row);
}

#[test]
fn symbols_counts_only_candidates_toward_a_flat_directory() {
    unparseable_files_do_not_make_a_directory_flat(symbols_row);
}

/// A narrowed call walks under the ignore rules the root walk had, ancestor
/// `.gitignore` files included: neither the listing of an unread scope nor
/// the call that reads it names an ignored entry.
fn narrowed_scopes_keep_the_ancestor_ignore_rules(build: Build) {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "skipme/\n*.gen.ts\n");
    for path in [
        "a/a1.ts",
        "a/a2.ts",
        "b/b1.ts",
        "b/b2.ts",
        "b/b3.gen.ts",
        "b/skipme/s1.ts",
        "b/zz/z1.ts",
        "b/zz/hidden.ts",
    ] {
        root.write(path, "export const value = 1;\n");
    }
    // An intermediate ancestor: the rule of `b` reaches the scope `b/zz`.
    root.write("b/.gitignore", "hidden.ts\n");
    let (first, calls, files) = walk_all(&root, build, json!({}), 3);
    assert_eq!(lead_dirs(&root, &first), ["b/zz", "b"], "{first}");
    assert_eq!(
        files,
        sorted(["a1.ts", "a2.ts", "b1.ts", "b2.ts", "z1.ts"]),
        "{calls:?}"
    );
}

#[test]
fn match_narrowed_scopes_keep_the_ancestor_ignore_rules() {
    narrowed_scopes_keep_the_ancestor_ignore_rules(match_row);
}

#[test]
fn symbols_narrowed_scopes_keep_the_ancestor_ignore_rules() {
    narrowed_scopes_keep_the_ancestor_ignore_rules(symbols_row);
}
