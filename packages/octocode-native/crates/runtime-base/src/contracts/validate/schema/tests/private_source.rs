//! A clasify `source.query` is the tool's public row, whoever sends it: the
//! tool's private progress is never part of it, and only an issued cursor
//! carries that progress, beside it, as `sourceProgress`.
use serde_json::{Value, json};

fn questions() -> Value {
    json!({"hook": {"type": "noul", "instructions": "Calls a React hook."}})
}

/// A fresh row carrying a tool's private progress inside `source.query` is
/// refused at admission, the field named where the caller wrote it.
#[test]
fn a_fresh_row_with_private_progress_in_source_query_is_refused() {
    let row = json!({"operation":"evaluate",
        "source":{"tool":"ghSearchCode","query":{"keywords":["useState"],"owner":"vercel","repo":"next.js","page":2}},
        "questions":questions()});
    let error = crate::contracts::validate::validate_query("clasify", row)
        .expect_err("private progress in source.query");
    assert!(
        error.issues.iter().any(|issue| issue.path.ends_with(&[
            "source".to_owned(),
            "query".to_owned(),
            "page".to_owned()
        ])),
        "{:?}",
        error.issues
    );
}

/// The same progress beside the query is the clasify row's own private
/// progress: a caller cannot send it, and the row a cursor restores holds it.
#[test]
fn source_progress_is_admitted_only_from_an_issued_cursor() {
    let row = json!({"operation":"evaluate",
        "source":{"tool":"ghSearchCode","query":{"keywords":["useState"],"owner":"vercel","repo":"next.js"}},
        "sourceProgress":{"page":2,"listed":"0123456789abcdef"},
        "questions":questions()});
    let fresh = crate::contracts::validate::validate_query("clasify", row.clone())
        .expect_err("a caller cannot send private progress");
    assert!(
        fresh
            .issues
            .iter()
            .any(|issue| issue.rule_id == "input.private-state"),
        "{:?}",
        fresh.issues
    );
    crate::contracts::validate::validate_saved_query("clasify", row)
        .expect("a restored row holds its progress");
}
