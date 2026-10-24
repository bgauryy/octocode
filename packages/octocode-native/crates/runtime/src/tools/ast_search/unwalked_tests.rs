//! A scan that stops at the walk ceiling names every scope it never read as a
//! `narrowScope` call, and the calls together reach every unread file. The
//! ceiling is lowered so a small tree reaches it.
use super::{AstSearchQuery, execute_ast, unwalked};
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod frontier;
mod reach;

const WINDOW: usize = 65_536;

/// Every row of `out` the leads of a terminal page name.
fn leads(out: &Value) -> Vec<(String, Value)> {
    out["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, call)| call["kind"] == "continue" && call["tool"] == "astSearch")
        .map(|(name, call)| (name.clone(), call.clone()))
        .chain(
            out["related"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|call| call["tool"] == "structureSearch")
                .enumerate()
                .map(|(index, call)| (index.to_string(), call.clone())),
        )
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
            let next = &lead["input"]["queries"][0];
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
    let mut row = json!({"operation":"match","path":root.to_string_lossy(),"rule":{"pattern":"export const $A = $B"}});
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
    let path = lead["input"]["queries"][0]["path"].as_str().expect("path");
    std::path::Path::new(path)
        .strip_prefix(root.canonicalize().expect("root"))
        .map_or_else(
            |_| path.to_owned(),
            |relative| relative.to_string_lossy().into_owned(),
        )
}

/// One operation's row: `match` or `symbols` over a scope, with `extra`.
type Build = fn(&std::path::Path, Value) -> Value;

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
