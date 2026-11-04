//! A declaration pattern over real pages: what it leaves unsaid costs no second
//! scan and no note, and every hit is reachable on the first answer, including
//! when the first page is not empty.

use super::scan::DIRECTORY_SCANS;
use crate::tools::ast_search::test_rows::{ast_chain, ast_rows, run};
use crate::tools::test_support::{Fixture, checks};
use serde_json::json;
use std::path::PathBuf;

const PATTERN: &str = "export function $NAME($$$) { $$$ }";

fn pattern_row(path: &std::path::Path, pattern: &str) -> serde_json::Value {
    json!({"operation":"match","path":path,"rule":{"pattern":pattern}})
}

fn exact_notes(out: &serde_json::Value) -> Vec<serde_json::Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["code"] == "structural.pattern.exact")
        .cloned()
        .collect()
}

/// `count` annotated exports across files, so a tiny page cuts inside one.
fn annotated_tree(root: &Fixture, count: usize) -> PathBuf {
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    let mut left = count;
    let mut file = 0;
    while left > 0 {
        let here = left.min(3);
        let body = (0..here)
            .map(|n| format!("export function f{file}_{n}(x: number): number {{ return x; }}\n"))
            .collect::<String>();
        std::fs::write(dir.join(format!("f{file}.ts")), body).expect("source");
        left -= here;
        file += 1;
    }
    std::fs::write(dir.join("other.ts"), "const unrelated = 1;\n").expect("source");
    dir
}

/// The trap of an exact matcher: one unannotated function among many used to
/// be the whole non-empty answer. Every function is a hit, in one scan.
#[test]
fn one_unannotated_function_among_many_does_not_hide_the_rest() {
    let root = Fixture::new();
    let dir = annotated_tree(&root, 100);
    std::fs::write(
        dir.join("plain.ts"),
        "export function plain(x) { return x; }\n",
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
    let before = scans();
    let out = run(&root.0, pattern_row(&dir, PATTERN)).expect("match");
    assert_eq!(out["matchCount"], 101, "{out}");
    assert_eq!(scans(), before + 1, "one scan: {out}");
    assert!(exact_notes(&out).is_empty(), "{out}");
    // Generics, predicates and generators are hits too.
    std::fs::write(
        dir.join("typed.ts"),
        "export function a<T>(x: T): T { return x; }\nexport function b(x): x is string { return true; }\nexport function* c() {}\nexport async function d(): Promise<void> {}\n",
    )
    .expect("source");
    let out = run(&root.0, pattern_row(&dir, PATTERN)).expect("match");
    assert_eq!(out["matchCount"], 105, "{out}");
}

/// A tiny page walks every hit once, each continuation keeps the pattern as
/// the caller wrote it, and the walk equals the pattern spelled in full.
#[test]
fn a_tiny_page_walk_has_no_gaps_or_duplicates() {
    let root = Fixture::new();
    let dir = annotated_tree(&root, 7);
    let mut row = pattern_row(&dir, PATTERN);
    row["limit"] = json!(1);
    let pages = ast_chain(&root.0, row);
    let rows = ast_rows(&pages);
    assert_eq!(rows.len(), 7, "{rows:?}");
    assert_eq!(
        rows.iter().collect::<std::collections::BTreeSet<_>>().len(),
        7,
        "{rows:?}"
    );
    let mut spelled = pattern_row(&dir, "export function $NAME($$$): $R { $$$ }");
    spelled["limit"] = json!(1);
    assert_eq!(rows, ast_rows(&ast_chain(&root.0, spelled)), "{pages:?}");
    for page in &pages[..pages.len() - 1] {
        let next = &page["next"]["nextPage"]["input"]["queries"][0];
        assert_eq!(next["rule"]["pattern"], PATTERN, "{page}");
    }
    let budgeted = ast_chain(&root.0, pattern_row(&dir, PATTERN));
    assert_eq!(ast_rows(&budgeted).len(), 7, "{budgeted:?}");
}

/// A scan its budget cut reads the pattern the same way in every window.
#[test]
fn every_scan_window_reads_the_pattern_the_same() {
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
    let rows = ast_rows(&ast_chain(&root.0, pattern_row(&dir, PATTERN)));
    assert_eq!(rows.len(), 2, "{rows:?}");
}

/// Grammars that spell a return type differently share one pattern text.
#[test]
fn a_directory_of_grammars_reads_one_pattern() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "function a(): void {}\n").expect("ts");
    std::fs::write(root.0.join("b.js"), "function b() {}\n").expect("js");
    let out = run(&root.0, pattern_row(&root.0, "function $N($$$) { $$$ }")).expect("mixed");
    assert_eq!(out["matchCount"], 2, "{out}");
}

/// A rule's nested patterns read the same: a containment rule finds what its
/// parts mean, first time.
#[test]
fn a_containment_rule_reads_its_patterns_the_same() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a.ts"),
        "class A<T> extends B {\n  private x = 1;\n  public async run(y: T): Promise<void> { await g(y); }\n}\nclass C {\n  other() { h(); }\n}\n",
    )
    .expect("ts");
    let has = json!({
        "operation": "match",
        "path": root.0,
        "rule": {
            "pattern": "class $C { $$$ }",
            "has": {"pattern": {"context": "class W { $M($$$) { $$$ } }", "selector": "method_definition"}, "stopBy": "end"}
        }
    });
    assert_eq!(run(&root.0, has).expect("has")["matchCount"], 2);
    let inside = json!({
        "operation": "match",
        "path": root.0,
        "rule": {"pattern": "g($$$)", "inside": {"pattern": "$M($$$) { $$$ }", "stopBy": "end"}}
    });
    assert_eq!(run(&root.0, inside).expect("inside")["matchCount"], 1);
}

/// What a pattern spells and the source lacks is a near miss, counted with the
/// call that lists them: an arrow too, though it has no name.
#[test]
fn a_spelled_part_that_differs_is_a_counted_near_miss() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(
        &file,
        "const a = (x) => x;\nconst b = (y, z) => y;\nfunction c(x): x is string { return true; }\n",
    )
    .expect("ts");
    let arrow = run(&root.0, pattern_row(&file, "($X, $Y, $Z) => $B")).expect("arrow");
    assert_eq!(arrow["matchCount"], 0, "{arrow}");
    let notes = exact_notes(&arrow);
    assert_eq!(notes.len(), 1, "{arrow}");
    assert!(
        notes[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("2 arrow_function node(s)")),
        "{arrow}"
    );
    let lead = &checks::related_call(&arrow, "astSearch", |input| {
        input["queries"][0]["rule"].get("kind").is_some()
    })["input"]["queries"][0];
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
    assert_eq!(run(&root.0, lead.clone()).expect("lead")["matchCount"], 2);
    // A return type it spells that the function does not have.
    let predicate = run(
        &root.0,
        pattern_row(&file, "function $N($$$): string { $$$ }"),
    )
    .expect("fn");
    assert_eq!(predicate["matchCount"], 0, "{predicate}");
    assert_eq!(exact_notes(&predicate).len(), 1, "{predicate}");
}

/// A method spelled without a type is a constructor in C#: the row says how it
/// read, and leads to what the file declares.
#[test]
fn a_reading_the_scope_holds_none_of_says_so() {
    let root = Fixture::new();
    let file = root.0.join("a.cs");
    std::fs::write(&file, "class A { public void M() {} }\n").expect("cs");
    let out = run(&root.0, pattern_row(&file, "$M($$$) { $$$ }")).expect("cs");
    assert_eq!(out["matchCount"], 0, "{out}");
    let notes = exact_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        notes[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("constructor_declaration")),
        "{out}"
    );
    let lead = &checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["operation"] == "symbols"
    })["input"]["queries"][0];
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
}

/// A pattern the grammar cannot read as one node is an error, never an empty
/// answer.
#[test]
fn a_pattern_with_no_single_node_is_an_error_not_an_absence() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "f();\n").expect("ts");
    for pattern in ["if ($A) { $$$ } ]", "f() g() {", "$A $B { $C } $D"] {
        let out = run(&root.0, pattern_row(&file, pattern));
        assert!(out.is_err(), "{pattern}: {out:?}");
    }
}

/// A relation with no `stopBy` reads the immediate neighbor. An empty rule that
/// holds matches with `stopBy:"end"` says so and gives that call; a rule that
/// already reads to the edge, or that holds none there either, adds nothing.
#[test]
fn an_empty_relation_says_what_stop_by_end_holds() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(
        &file,
        "class A { run() { if (x) { return 1; } } }\nclass B { other() {} }\n",
    )
    .expect("ts");
    let notes = |out: &serde_json::Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["code"] == "structural.rule.neighbor")
            .count()
    };
    let row = |rule: serde_json::Value| json!({"operation":"match","path":file,"rule":rule});
    let near = run(
        &root.0,
        row(json!({"kind":"class_declaration","has":{"kind":"return_statement"}})),
    )
    .expect("neighbor");
    assert_eq!(near["matchCount"], 0, "{near}");
    assert_eq!(notes(&near), 1, "{near}");
    let lead = &checks::related_call(&near, "astSearch", |input| {
        input["queries"][0]["rule"]["has"]["stopBy"] == "end"
    })["input"]["queries"][0];
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
    assert_eq!(run(&root.0, lead.clone()).expect("lead")["matchCount"], 1);

    // Inside `all` too, and an inner relation of a relation.
    let nested = run(
        &root.0,
        row(json!({"all":[{"kind":"return_statement"},{"inside":{"kind":"class_declaration"}}]})),
    )
    .expect("nested");
    assert_eq!(notes(&nested), 1, "{nested}");

    for quiet in [
        json!({"kind":"class_declaration","has":{"kind":"return_statement","stopBy":"end","regex":"zzz"}}),
        json!({"kind":"class_declaration","has":{"kind":"while_statement"}}),
        json!({"kind":"while_statement"}),
    ] {
        let out = run(&root.0, row(quiet.clone())).expect("quiet");
        assert_eq!(out["matchCount"], 0, "{quiet}: {out}");
        assert_eq!(notes(&out), 0, "{quiet}: {out}");
    }
}
