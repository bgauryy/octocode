//! Q5 outline strings: one string per declaration, members after their
//! parent, and every field reads back.
use super::*;
use serde_json::json;

fn outline() -> Vec<Value> {
    vec![
        json!({"symbolName": "drop_answered_leads", "kind": "function", "line": 15, "endLine": 67,
            "docStartLine": 8, "exported": true}),
        json!({"symbolName": "Pager", "kind": "struct", "line": 70, "endLine": 120, "exportedAs": ["Pager", "default"],
        "members": [
            {"symbolName": "new", "kind": "method", "line": 74, "endLine": 80, "startLine": 73,
                "members": [{"symbolName": "inner", "kind": "function", "line": 76, "column": 9}]},
            {"symbolName": "reads_twice", "kind": "function", "line": 90, "testIdentity": {"framework": "rust"}},
        ]}),
        json!({"symbolName": "impl Display for (A, B)", "kind": "impl", "line": 130, "endLine": 140}),
        json!({"symbolName": "orphan", "kind": "method", "line": 150, "parent": "Hidden", "parentLine": 149}),
        json!({"symbolName": "loose", "line": 160, "parent": "Gone"}),
        json!({"symbolName": "case", "kind": "function", "line": 170,
            "testIdentity": {"framework": "jest", "name": "renders, then (updates)", "parameterized": true},
            "occurrenceId": "o-1", "sourceDigest": "abc"}),
    ]
}

/// The form bh-code measured: the line first, then the kind and name, and
/// the declaration's facts in one tag group.
#[test]
fn each_declaration_is_one_line_first_string_with_its_members_after_it() {
    let lines = outline_lines(&outline());
    assert_eq!(
        lines[0],
        "15\tfunction drop_answered_leads (end 67, doc 8, exported)"
    );
    assert_eq!(lines[1], "70\tstruct Pager (end 120, as Pager|default)");
    assert_eq!(lines[2], "74\t  method new (end 80, start 73)");
    assert_eq!(lines[3], "76\t    function inner (col 9)");
    assert_eq!(lines[4], "90\t  function reads_twice (test rust)");
    assert_eq!(lines[5], "130\timpl impl Display for (A, B) (end 140)");
    assert_eq!(lines[6], "150\tmethod orphan (parent Hidden at 149)");
    assert_eq!(lines[7], "160\t- loose (parent Gone)");
    assert!(
        lines[8].starts_with("170\tfunction case (more {"),
        "{}",
        lines[8]
    );
    assert_eq!(lines.len(), 9, "one string per declaration");
}

/// Every field of every declaration, members at any depth, reads back from
/// the strings: the form loses nothing.
#[test]
fn every_declaration_reads_back_from_its_string() {
    let rows = outline();
    let lines = outline_lines(&rows)
        .into_iter()
        .map(Value::from)
        .collect::<Vec<_>>();
    assert_eq!(rows_of_lines(&lines), Some(rows));
    // The strings are shorter than the objects they state.
    let size = |value: &[Value]| serde_json::to_string(value).expect("json").len();
    assert!(size(&lines) < size(&outline()));
}

/// A string a reader cannot read as an outline line is refused, not guessed.
#[test]
fn a_string_that_is_not_an_outline_line_is_refused() {
    for text in [
        "no tab",
        "x\tfunction f",
        "5\t function odd indent",
        "5\tfunction f (end x)",
    ] {
        assert_eq!(outline_row(text), None, "{text}");
    }
    assert_eq!(
        rows_of_lines(&[json!("5\t  function orphan member")]),
        None,
        "a member needs a parent before it"
    );
    // A name holding ` (` that is not a tag group stays the name.
    assert_eq!(
        outline_row("9\tfunction f (x)"),
        Some((
            0,
            json!({"symbolName": "f (x)", "kind": "function", "line": 9})
        ))
    );
}
