use super::*;
use octocode_test_support::json_fixture;

#[test]
fn repaired_request_shapes_disclose_the_canonical_form() {
    for (tool, sent, phrases) in [
        (
            "localFetch",
            json_fixture(r#"{"path": "a.ts"}"#),
            vec!["queries", "send"],
        ),
        (
            "localFetch",
            json_fixture(r#"[{"path": "a.ts"}]"#),
            vec!["queries", "send"],
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": {"path": "a.ts"}}"#),
            vec!["queries", "send"],
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": "[{\"path\":\"a.ts\"}]"}"#),
            vec!["queries", "send"],
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": "{\"path\":\"a.ts\"}"}"#),
            vec!["queries", "send"],
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "view": "files",
                    "queries": [
                        {"path": "a", "matchString": ["x"]},
                        {"path": "b", "matchString": ["y"], "view": "content"}
                    ]
                }"#,
            ),
            vec!["view", "queries", "send"],
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [
                        {"path": "a.ts", "responseChars": 900},
                        {"path": "b.ts", "responseChars": 900}
                    ]
                }"#,
            ),
            vec!["responseChars", "queries", "send"],
        ),
    ] {
        accepted(tool, sent.clone());
        let notes = normalized_input(tool, sent.clone()).1;
        assert!(
            notes
                .iter()
                .any(|note| phrases.iter().all(|phrase| note.contains(phrase))),
            "{tool} {sent}: repaired shape must state the canonical form: {notes:?}"
        );
    }
}

#[test]
fn a_supplied_incomplete_sort_is_a_disclosed_repair() {
    for sort in [json!("path"), json_fixture(r#"{"by": "bytes"}"#)] {
        let sent = json!({"queries":[{"path":".","operation":"files","sort":sort}]});
        let ran = accepted("structureSearch", sent.clone());
        let notes = normalized_input("structureSearch", sent).1;
        assert!(
            matches!(notes.as_slice(), [one] if one.starts_with("queries[0]: ")
            && one.contains("sort") && one.contains("order") && one.contains("send")),
            "{ran}: {notes:?}"
        );
    }
}

#[test]
fn valid_defaults_and_explicit_values_are_not_repairs() {
    for sent in [
        json_fixture(r#"{"queries": [{"path": ".", "operation": "files"}]}"#),
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": ".",
                        "operation": "files",
                        "sort": {"by": "path", "order": "asc"}
                    }
                ]
            }"#,
        ),
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": ".",
                        "operation": "files",
                        "sort": {"by": "bytes", "order": "asc"}
                    }
                ]
            }"#,
        ),
    ] {
        let (ran, notes) = normalized_input("structureSearch", sent.clone());
        assert_eq!(ran, sent);
        assert!(
            notes.is_empty(),
            "valid defaults are not repairs: {notes:?}"
        );
        accepted("structureSearch", sent);
    }
}

#[test]
fn removed_sort_names_remain_rejected_without_an_alias() {
    let sent = json_fixture(
        r#"{
            "queries": [
                {
                    "path": ".",
                    "operation": "files",
                    "sort": {"by": "lineCount", "order": "desc"}
                }
            ]
        }"#,
    );
    let (ran, notes) = normalized_input("structureSearch", sent.clone());
    assert_eq!(ran, sent);
    assert!(
        notes.is_empty(),
        "a refused former name was not repaired: {notes:?}"
    );
    let error = match prepare_rows("structureSearch", ran) {
        Err(error) => error,
        Ok(mut rows) => rows.remove(0).expect_err("former enum stays rejected"),
    };
    let shown = crate::contracts::format_input_error("structureSearch", &error, &[], &Value::Null)
        .to_string();
    assert!(
        shown.contains("totalLines") && shown.contains("lineCount"),
        "{shown}"
    );
}

#[test]
fn ambiguous_request_shapes_remain_unchanged_without_repair_claims() {
    for sent in [
        json_fixture(
            r#"{
                "queries": [
                    {"path": "a", "responseChars": 1},
                    {"path": "b", "responseChars": 2}
                ]
            }"#,
        ),
        json_fixture(r#"{"queries": "not JSON"}"#),
        json_fixture(r#"{"queries": [["a", "b"]]}"#),
        // A cursor is a request of its own, never a field beside rows.
        json_fixture(r#"{"queries": [{"cursor": "cursor2.x"}]}"#),
    ] {
        let (ran, notes) = normalized_input("localFetch", sent.clone());
        assert_eq!(ran, sent);
        assert!(notes.is_empty(), "unrepaired input: {notes:?}");
        assert!(
            prepare_rows("localFetch", ran).map_or(true, |rows| rows.iter().any(Result::is_err))
        );
    }
}
