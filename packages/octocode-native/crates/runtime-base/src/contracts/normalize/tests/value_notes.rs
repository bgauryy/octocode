//! A repaired value is said with the form to send: the field with the value
//! it ran with, never the bare name of a field the caller already sent. A
//! field repaired more than once on a row is said once, by its whole value.
use super::*;
use octocode_test_support::json_fixture;

/// The one note `tool` says of `sent`, and the request it ran as.
fn one_note(tool: &str, sent: &str) -> (Value, String) {
    let sent = json_fixture(sent);
    let ran = accepted(tool, sent.clone());
    let notes = normalized_input(tool, sent.clone()).1;
    let [note] = notes.as_slice() else {
        panic!("{tool} {sent}: one note, got {notes:?}");
    };
    (ran, note.clone())
}

/// Every kind of value repair (a value in its field's shape, a sort given
/// its direction, a switch read as `on`, a literal in another case, a value
/// spelt as the registry's host, a request field) says the form it ran,
/// and advises that same form.
#[test]
fn every_value_repair_names_the_form_to_send() {
    for (tool, sent, at, form) in [
        (
            "localFetch",
            r#"{"queries": [{"path": "a.ts", "limit": "5"}]}"#,
            "/queries/0/limit",
            "limit",
        ),
        (
            "structureSearch",
            r#"{"queries": [{"path": ".", "operation": "files", "sort": "bytes"}]}"#,
            "/queries/0/sort",
            "sort",
        ),
        (
            "localSearch",
            r#"{"queries": [{"path": ".", "matchString": ["x"], "multiline": true}]}"#,
            "/queries/0/multiline",
            "multiline",
        ),
        (
            "localSearch",
            r#"{"queries": [{"path": ".", "matchString": "x"}]}"#,
            "/queries/0/matchString",
            "matchString",
        ),
        (
            "securityCheck",
            r#"{"queries": [{"operation": "package", "ecosystem": "pypi", "packageName": "requests"}]}"#,
            "/queries/0/ecosystem",
            "ecosystem",
        ),
        (
            "securityCheck",
            r#"{"queries": [{"operation": "package", "ecosystem": "crates", "packageName": "serde"}]}"#,
            "/queries/0/ecosystem",
            "ecosystem",
        ),
        (
            "localSearch",
            r#"{"queries": [{"path": ".", "matchString": ["x"]}], "responseChars": "900"}"#,
            "/responseChars",
            "responseChars",
        ),
    ] {
        let (ran, note) = one_note(tool, sent);
        let runs = format!("`{form}: {}`", sketch(ran.pointer(at).expect("runs")));
        assert!(
            note.ends_with(&format!(" as {runs}; send {runs} next time.")),
            "{tool} {sent}: {note}"
        );
        assert!(!note.contains(&format!("send `{form}` next")), "{note}");
    }
}

/// A field repaired twice on a row is said once, by its whole value: a sort
/// given its direction whose key was also spelt another way (or in another
/// case), and a renamed sort whose key a spelling then read. Two names
/// read as one field are said in one note naming both.
#[test]
fn one_note_per_repaired_field() {
    for (tool, sent, at, read) in [
        (
            "structureSearch",
            r#"{"queries": [{"path": ".", "operation": "files", "sort": {"by": "size"}}]}"#,
            "/queries/0/sort",
            r#"read `sort: {"by":"size"}` as `sort: "#,
        ),
        (
            "structureSearch",
            r#"{"queries": [{"path": ".", "operation": "files", "sort": {"by": "Bytes"}}]}"#,
            "/queries/0/sort",
            r#"read `sort: {"by":"Bytes"}` as `sort: "#,
        ),
        (
            "structureSearch",
            r#"{"queries": [{"path": ".", "operation": "files", "sortBy": {"by": "size", "order": "desc"}}]}"#,
            "/queries/0/sort",
            "read `sortBy` as `sort: ",
        ),
        (
            "localSearch",
            r#"{"queries": [{"path": ".", "matchString": ["x"], "countOnly": true, "count": true}]}"#,
            "/queries/0/view",
            "read `countOnly`, `count` as `view: ",
        ),
    ] {
        let (ran, note) = one_note(tool, sent);
        let field = at.rsplit('/').next().expect("field");
        let runs = format!("`{field}: {}`", sketch(ran.pointer(at).expect("runs")));
        assert!(
            note.starts_with(&format!("queries[0]: {read}"))
                && note.ends_with(&format!(" as {runs}; send {runs} next time.")),
            "{tool} {sent}: {note}"
        );
    }
    // Two fields read into one row field's object: one note names both.
    let (ran, note) = one_note(
        "ghSearchHistory",
        r#"{"queries": [{"operation": "pullRequest", "owner": "o", "repo": "r", "state": "open", "author": "x"}]}"#,
    );
    let runs = format!("`filters: {}`", sketch(&ran["queries"][0]["filters"]));
    assert!(
        note.contains("`state`")
            && note.contains("`author`")
            && note.ends_with(&format!(" as {runs}; send {runs} next time.")),
        "{note}"
    );
}
