//! A file whose matches span match pages parses once for all of them: the
//! later pages cut from its kept rows, byte-identical to rebuilt ones, for as
//! long as the stored scan's proof holds.
use super::page_rows::rebuilds;
use super::*;
use crate::security::ContentSecurity;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};

fn source() -> String {
    (0..40)
        .map(|index| format!("target(\"value number {index}\", {index});\n"))
        .collect()
}

fn run(row: &Value, paths: &PathPolicy, security: &ContentSecurity) -> Value {
    let query = serde_json::from_value(row.clone()).expect("canonical match");
    super::super::execute_ast(
        &query,
        paths,
        security,
        &crate::execution::cancel::NeverCancel,
        Some(1),
    )
    .expect("page")
}

fn first_row(root: &std::path::Path, extra: Value) -> Value {
    let mut row = json!({
        "operation":"match","path":root,"rule":{"pattern":"target($$$A)"},"parser":"typescript"
    });
    row.as_object_mut()
        .expect("row")
        .extend(extra.as_object().cloned().unwrap_or_default());
    row
}

fn next_of(out: &Value) -> Option<Value> {
    out["next"]["nextPage"]["input"]["queries"].get(0).cloned()
}

/// Every page of `row`'s result, each scanned afresh when `fresh`.
fn walk(row: &Value, paths: &PathPolicy, security: &ContentSecurity, fresh: bool) -> Vec<String> {
    let (mut pages, mut call) = (Vec::new(), Some(row.clone()));
    while let Some(row) = call {
        if fresh && let Some(snapshot) = row["snapshot"].as_str() {
            SCANS.evict(snapshot);
        }
        let out = run(&row, paths, security);
        call = next_of(&out);
        pages.push(serde_json::to_string(&out).expect("page"));
        assert!(pages.len() < 200, "the walk ends");
    }
    pages
}

/// One parse for the rows of a whole walk, and the bytes of a rebuild on every
/// page, in each row shape.
#[test]
fn match_pages_of_one_file_cut_from_its_kept_rows() {
    for extra in [
        json!({}),
        json!({"includeCaptures":true}),
        json!({"includeProvenance":true}),
    ] {
        let root = Fixture::new();
        let file = root.0.join("source.ts");
        std::fs::write(&file, source()).expect("source");
        let (paths, security) = simple_policy(&root.0);
        octocode_engine::stamp::settle_written_so_far();
        let row = first_row(&root.0, extra.clone());
        let file = file.canonicalize().expect("canonical");
        let kept = walk(&row, &paths, &security, false);
        assert!(kept.len() > 2, "{extra}: {} pages", kept.len());
        let scanned = rebuilds(&file);
        assert_eq!(scanned, 1, "{extra}: {} pages", kept.len());
        let fresh = walk(&row, &paths, &security, true);
        assert_eq!(rebuilds(&file), scanned + kept.len(), "{extra}");
        assert_eq!(kept, fresh, "{extra}");
    }
}

/// An edit between match pages restarts the read, also one that restores the
/// modification time: the stored scan's proof is what the rows stand on.
#[test]
fn an_edit_between_match_pages_restarts() {
    for restore in [false, true] {
        let root = Fixture::new();
        let file = root.0.join("source.ts");
        std::fs::write(&file, source()).expect("source");
        let (paths, security) = simple_policy(&root.0);
        octocode_engine::stamp::settle_written_so_far();
        let first = run(&first_row(&root.0, json!({})), &paths, &security);
        let next = next_of(&first).expect("a second page");
        let second = run(&next, &paths, &security);
        let third = next_of(&second).expect("a third page");
        let modified = std::fs::metadata(&file)
            .expect("metadata")
            .modified()
            .expect("mtime");
        std::fs::write(&file, source().replace("value number 3", "VALUE number 3")).expect("edit");
        if restore {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file)
                .expect("open")
                .set_times(std::fs::FileTimes::new().set_modified(modified))
                .expect("restore mtime");
        }
        let page = run(&third, &paths, &security);
        assert_eq!(
            page["errorCode"], "staleSnapshot",
            "restore {restore}: {page}"
        );
    }
}

/// The scan's proof watches what a test-code scope's verdicts read, so such a
/// file parses once for all its pages too.
#[test]
fn a_test_code_scope_keeps_its_rows_too() {
    let root = Fixture::new();
    let file = root.0.join("source.ts");
    std::fs::write(&file, source()).expect("source");
    let (paths, security) = simple_policy(&root.0);
    octocode_engine::stamp::settle_written_so_far();
    let row = first_row(&root.0, json!({"testCode":"exclude"}));
    let pages = walk(&row, &paths, &security, false);
    assert!(pages.len() > 2, "{} pages", pages.len());
    assert_eq!(rebuilds(&file.canonicalize().expect("canonical")), 1);
}

/// A file's key blocks and whole-file secret lines are judged over its whole
/// text on every page ([`crate::tools::ast_search::judged_rows`]), and a page
/// shows what judging it afresh shows.
#[test]
fn match_pages_of_one_file_judge_its_text_once() {
    let body = "Qx7Zk2Vb9Ls4Tm6Yp1Wn8Rd3Hc5Gj0Fa7Eu2Si9Oq4Lk6Xv1Bz3Nm8Pt5Dy";
    let root = Fixture::new();
    let file = root.0.join("source.ts");
    let key = format!(
        "target(\"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"-----END RSA PRIVATE KEY-----\");\n"
    );
    std::fs::write(&file, source() + &key).expect("source");
    let (paths, security) = simple_policy(&root.0);
    octocode_engine::stamp::settle_written_so_far();
    let row = first_row(&root.0, json!({}));
    let kept = walk(&row, &paths, &security, false);
    assert!(kept.len() > 2, "{} pages", kept.len());
    let shown = kept.concat();
    assert!(!shown.contains(body), "{shown}");
    assert!(shown.contains("REDACTED"), "{shown}");
    assert_eq!(walk(&row, &paths, &security, true), kept);
}
