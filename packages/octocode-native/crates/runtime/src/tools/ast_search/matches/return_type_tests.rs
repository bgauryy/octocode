//! A declaration pattern with no return annotation: when it matches nothing
//! and its annotated spelling does, the row holds those matches, says once
//! which pattern ran, and every page keeps running that pattern.

use super::scan::DIRECTORY_SCANS;
use crate::tools::ast_search::test_rows::{ast_chain, ast_rows, run};
use crate::tools::test_support::{Fixture, checks};
use serde_json::json;
use std::path::PathBuf;

const CODE: &str = "structural.query.rewritten";
const REQUESTED: &str = "export function $NAME($$$) { $$$ }";
const REWRITTEN: &str = "export function $NAME($$$): $R { $$$ }";

fn rewrite_notes(out: &serde_json::Value) -> Vec<&serde_json::Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == CODE)
        .collect()
}

fn pattern_row(path: &std::path::Path, pattern: &str) -> serde_json::Value {
    json!({"operation":"match","path":path,"rule":{"pattern":pattern}})
}

/// Every source line of the two files below that declares an annotated
/// function, each file with several so a tiny page cuts inside one.
fn annotated_tree(root: &Fixture) -> PathBuf {
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    for (stem, count) in [("a", 3), ("b", 2)] {
        let body = (0..count)
            .map(|n| format!("export function {stem}{n}(x: number): number {{ return x; }}\n"))
            .collect::<String>();
        std::fs::write(dir.join(format!("{stem}.ts")), body).expect("source");
    }
    std::fs::write(dir.join("c.ts"), "const unrelated = 1;\n").expect("source");
    dir
}

/// The defect: the natural pattern used to return nothing on annotated code.
/// Now the row holds the matches, and one diagnostic names both patterns and
/// the explicit repeat.
#[test]
fn an_annotated_declaration_answers_a_pattern_without_a_return_type() {
    let root = Fixture::new();
    let dir = annotated_tree(&root);
    let out = run(&root.0, pattern_row(&dir, REQUESTED)).expect("match");
    assert_eq!(out["matchCount"], 5, "{out}");
    let notes = rewrite_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains(REWRITTEN) && message.contains(REQUESTED),
        "{message}"
    );
    assert!(
        notes[0]["recovery"]
            .as_str()
            .is_some_and(|recovery| recovery.contains(REWRITTEN)),
        "{out}"
    );
    // The same answer the corrected pattern gives by itself, which is no
    // rewrite and so says nothing.
    let direct = run(&root.0, pattern_row(&dir, REWRITTEN)).expect("direct");
    assert_eq!(out["files"], direct["files"], "{out}");
    assert!(rewrite_notes(&direct).is_empty(), "{direct}");
}

/// Hits from the rewrite page losslessly: a tiny limit walks every row once,
/// each continuation keeps the rewritten pattern, and only the first page
/// discloses it.
#[test]
fn every_page_of_a_rewrite_keeps_the_rewritten_pattern_and_says_so_once() {
    let root = Fixture::new();
    let dir = annotated_tree(&root);
    let mut row = pattern_row(&dir, REQUESTED);
    row["limit"] = json!(1);
    let pages = ast_chain(&root.0, row);
    assert!(pages.len() >= 5, "{pages:?}");
    let rows = ast_rows(&pages);
    assert_eq!(rows.len(), 5, "{rows:?}");
    let distinct = rows.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(distinct.len(), 5, "{rows:?}");
    let mut direct = pattern_row(&dir, REWRITTEN);
    direct["limit"] = json!(1);
    assert_eq!(rows, ast_rows(&ast_chain(&root.0, direct)), "{pages:?}");
    assert_eq!(rewrite_notes(&pages[0]).len(), 1, "{:?}", pages[0]);
    for page in &pages[1..] {
        assert!(rewrite_notes(page).is_empty(), "{page}");
    }
    for page in &pages[..pages.len() - 1] {
        let next = &page["next"]["nextPage"]["input"]["queries"][0];
        assert_eq!(next["rule"]["pattern"], REWRITTEN, "{page}");
    }
}

/// A pattern with matches stays exactly as written and pays no second scan,
/// though the scope also holds annotated declarations; so does a pattern that
/// already spells a return type.
#[test]
fn a_matching_or_annotated_pattern_is_unchanged_and_scans_once() {
    let root = Fixture::new();
    let dir = root.0.join("once");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("a.ts"),
        "export function typed(a: number): number { return a; }\nexport function plain(b) { return b; }\n",
    )
    .expect("source");
    let canonical = dir.canonicalize().expect("canonical");
    let scans = || {
        DIRECTORY_SCANS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|scanned| **scanned == canonical)
            .count()
    };
    for pattern in [REQUESTED, REWRITTEN] {
        let before = scans();
        let out = run(&root.0, pattern_row(&dir, pattern)).expect("match");
        assert_eq!(out["matchCount"], 1, "{pattern}: {out}");
        assert!(rewrite_notes(&out).is_empty(), "{pattern}: {out}");
        assert_eq!(scans(), before + 1, "{pattern}: one scan, {out}");
    }
}

/// Neither spelling matches: the row stays the requested pattern's empty
/// answer with the leads an exact miss already gives.
#[test]
fn an_empty_rewrite_leaves_the_requested_empty_answer() {
    let root = Fixture::new();
    let dir = root.0.join("generic");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("a.rs"),
        "pub(crate) fn a() -> u8 { 1 }\npub fn b() -> u8 { 2 }\n",
    )
    .expect("source");
    let out = run(&root.0, pattern_row(&dir, "fn $N($$$) { $$$ }")).expect("match");
    assert_eq!(out["matchCount"], 0, "{out}");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(rewrite_notes(&out).is_empty(), "{out}");
    assert!(
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["code"] == "structural.pattern.exact"),
        "{out}"
    );
    let lead = &checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["operation"] == "match"
    })["input"]["queries"][0];
    assert_eq!(lead["rule"], json!({"kind":"function_item"}), "{out}");
}

/// A partial answer says so: declarations neither spelling matches (here a
/// visibility, a type predicate and type parameters) are counted beside the
/// hits and listed by the lead, which a pattern that matched them all omits.
#[test]
fn declarations_neither_spelling_matches_are_counted_beside_the_hits() {
    let root = Fixture::new();
    let notes_of = |out: &serde_json::Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["code"] == "structural.pattern.exact")
            .cloned()
            .collect::<Vec<_>>()
    };
    let lead_of = |out: &serde_json::Value| {
        checks::related_call(out, "astSearch", |input| {
            input["queries"][0]["rule"].get("kind").is_some()
        })["input"]["queries"][0]
            .clone()
    };
    let rust = root.0.join("rust");
    std::fs::create_dir_all(&rust).expect("dir");
    std::fs::write(
        rust.join("a.rs"),
        "fn a() -> u8 { 1 }\npub fn b() -> u8 { 2 }\npub(crate) fn c() -> u8 { 3 }\n",
    )
    .expect("source");
    let out = run(&root.0, pattern_row(&rust, "fn $N($$$) { $$$ }")).expect("rust");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert_eq!(rewrite_notes(&out).len(), 1, "{out}");
    let notes = notes_of(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        notes[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("2 more function_item node(s)")),
        "{out}"
    );
    let lead = lead_of(&out);
    assert_eq!(lead["rule"], json!({"kind":"function_item"}), "{out}");
    let listed = run(&root.0, lead).expect("lead runs");
    assert_eq!(listed["matchCount"], 3, "{listed}");

    let typed = root.0.join("typed");
    std::fs::create_dir_all(&typed).expect("dir");
    std::fs::write(
        typed.join("a.ts"),
        "export function a(): void {}\nexport function b(x): x is string { return true; }\nexport function c<T>(x: T): T { return x; }\nfunction local(): void {}\n",
    )
    .expect("source");
    let out = run(&root.0, pattern_row(&typed, REQUESTED)).expect("ts");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = notes_of(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        notes[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("2 more function_declaration node(s)")),
        "{out}"
    );
    let lead = lead_of(&out);
    assert_eq!(
        lead["rule"],
        json!({"kind":"function_declaration","inside":{"kind":"export_statement"}}),
        "{out}"
    );
    let listed = run(&root.0, lead).expect("lead runs");
    assert_eq!(listed["matchCount"], 3, "{listed}");

    // A file view is told the same, and its lead lists the nodes.
    for view in ["files", "countMatches"] {
        let mut row = pattern_row(&typed, REQUESTED);
        row["view"] = json!(view);
        let out = run(&root.0, row).expect("file view");
        assert_eq!(out["matchCount"], 1, "{view}: {out}");
        assert_eq!(notes_of(&out).len(), 1, "{view}: {out}");
        let lead = lead_of(&out);
        crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
        assert_eq!(run(&root.0, lead).expect("lead")["matchCount"], 3, "{view}");
    }

    // Every declaration of the kind is a hit: nothing to add.
    std::fs::write(root.0.join("whole.ts"), "export function a(): void {}\n").expect("source");
    let out = run(&root.0, pattern_row(&root.0.join("whole.ts"), REQUESTED)).expect("whole");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert!(notes_of(&out).is_empty(), "{out}");
}

/// The rule around the pattern, a single file and the file views all keep
/// their shape; only the pattern changes.
#[test]
fn the_rewrite_keeps_the_rest_of_the_query() {
    let root = Fixture::new();
    let dir = annotated_tree(&root);
    let file = dir.join("a.ts");
    let single = run(&root.0, pattern_row(&file, REQUESTED)).expect("file");
    assert_eq!(single["matchCount"], 3, "{single}");
    assert_eq!(rewrite_notes(&single).len(), 1, "{single}");

    let mut listed = pattern_row(&dir, REQUESTED);
    listed["view"] = json!("files");
    let listed = run(&root.0, listed).expect("files view");
    assert_eq!(listed["matchCount"], 5, "{listed}");
    assert_eq!(
        listed["files"].as_array().map(Vec::len),
        Some(2),
        "{listed}"
    );
    assert_eq!(rewrite_notes(&listed).len(), 1, "{listed}");

    let mut counted = pattern_row(&dir, REQUESTED);
    counted["view"] = json!("countMatches");
    let counted = run(&root.0, counted).expect("count view");
    assert_eq!(counted["matchCount"], 5, "{counted}");

    let rule = json!({
        "pattern": REQUESTED,
        "not": {"regex": "a0"}
    });
    let narrowed = run(&root.0, json!({"operation":"match","path":dir,"rule":rule})).expect("rule");
    assert_eq!(narrowed["matchCount"], 4, "{narrowed}");
}

/// A scan its budget cut has not shown the requested pattern is empty, so the
/// rest of the walk keeps both spellings: an unannotated declaration in a
/// later window is still found, every window once.
#[test]
fn a_cut_scan_keeps_both_spellings_across_its_windows() {
    let root = Fixture::new();
    let dir = root.0.join("cut");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("a.ts"),
        "export function typed(x: number): number { return x; }\n",
    )
    .expect("source");
    std::fs::write(dir.join("b.ts"), "export function plain(x) { return x; }\n").expect("source");
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let pages = ast_chain(&root.0, pattern_row(&dir, REQUESTED));
    let rows = ast_rows(&pages);
    assert_eq!(rows.len(), 2, "{pages:?}");
    assert!(
        rows.iter().any(|row| row.contains("typed"))
            && rows.iter().any(|row| row.contains("plain")),
        "{rows:?}"
    );
    let notes = pages.iter().flat_map(rewrite_notes).count();
    assert_eq!(notes, 1, "{pages:?}");
}

/// A grammar with no optional return annotation has nothing to rewrite, and
/// an expression pattern is not a declaration.
#[test]
fn nothing_to_annotate_means_no_rewrite() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.js"), "export function f(a) { return a; }\n").expect("js");
    std::fs::write(root.0.join("b.ts"), "call(1);\n").expect("ts");
    let js = run(
        &root.0,
        pattern_row(
            &root.0.join("a.js"),
            "export function $N($$$) { return 1; }",
        ),
    )
    .expect("js");
    assert_eq!(js["matchCount"], 0, "{js}");
    assert!(rewrite_notes(&js).is_empty(), "{js}");
    let call = run(&root.0, pattern_row(&root.0.join("b.ts"), "absent($X)")).expect("call");
    assert_eq!(call["matchCount"], 0, "{call}");
    assert!(rewrite_notes(&call).is_empty(), "{call}");
}
