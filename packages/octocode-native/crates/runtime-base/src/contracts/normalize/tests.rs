use super::*;
use crate::contracts::prepare_rows;
use octocode_test_support::json_fixture;
use serde_json::json;

mod beside_cursor;
mod failed_row_notes;
mod guessed_fields;
mod request_shapes;
mod value_notes;

fn accepted(tool: &str, sent: Value) -> Value {
    let input = normalized_input(tool, sent.clone()).0;
    let rows = prepare_rows(tool, input.clone()).unwrap_or_else(|error| {
        panic!("{sent} should have one reading: {:?}", error.issues);
    });
    assert!(rows.iter().all(Result::is_ok), "{sent}: {rows:?}");
    input
}

/// A field another form declares, sent at the default every such form
/// gives it, says what omitting it says: the row runs without it. Any other
/// value stays a refusal naming the form that takes it.
#[test]
fn a_field_at_its_default_beside_another_form_runs_without_it() {
    let sent = json_fixture(
        r#"{
            "queries": [
                {
                    "path": ".",
                    "matchString": ["x"],
                    "view": "matchOnly",
                    "contextLines": {"before": 0, "after": 0}
                }
            ]
        }"#,
    );
    let row = accepted("localSearch", sent.clone())["queries"][0].clone();
    assert!(row.get("contextLines").is_none(), "{row}");
    assert_eq!(row["view"], "matchOnly", "{row}");
    let notes = normalized_input("localSearch", sent).1;
    assert!(
        matches!(notes.as_slice(), [one] if one.starts_with("queries[0]: left out `contextLines: ")
            && one.contains("leave `contextLines` out next time")),
        "{notes:?}"
    );
    let wider = json_fixture(
        r#"{
            "queries": [
                {
                    "path": ".",
                    "matchString": ["x"],
                    "view": "matchOnly",
                    "contextLines": {"before": 1, "after": 0}
                }
            ]
        }"#,
    );
    let (input, _) = normalized_input("localSearch", wider);
    assert_eq!(
        input["queries"][0]["contextLines"],
        json_fixture(r#"{"before": 1, "after": 0}"#)
    );
    let refused = match prepare_rows("localSearch", input) {
        Err(error) => error,
        Ok(mut rows) => rows.remove(0).expect_err("refused"),
    };
    let shown = crate::contracts::format_input_error("localSearch", &refused, &[], &Value::Null)
        .to_string();
    assert!(
        shown.contains("contextLines") && shown.contains("content"),
        "{shown}"
    );
}

/// A clasify source's `query` is the row a direct call of its tool
/// takes, so it is repaired as that direct call is, and the note names the
/// field to send under `source.query`.
#[test]
fn a_source_query_is_repaired_as_its_tools_direct_call() {
    let query = json_fixture(r#"{"path": ".", "matchString": ["x"], "contextLines": 4}"#);
    let sent = json!({"queries":[{"operation":"evaluate",
        "source":{"tool":"localSearch","query":query},
        "questions":{"r":{"type":"noul","instructions":"Is it?"}}}]});
    let direct = normalized_input("localSearch", json!({"queries":[query]})).0;
    let input = accepted("clasify", sent.clone());
    assert_eq!(
        input["queries"][0]["source"]["query"], direct["queries"][0],
        "{input}"
    );
    assert_eq!(
        input["queries"][0]["source"]["query"]["contextLines"],
        json_fixture(r#"{"before": 4, "after": 4}"#),
        "{input}"
    );
    let notes = normalized_input("clasify", sent).1;
    assert!(
        notes
            .iter()
            .any(|note| note.starts_with("queries[0]: source.query: ")
                && note.contains("contextLines")),
        "{notes:?}"
    );
}

/// A value is repaired one nesting level per validation round, so the rounds
/// a call takes are its own depth: a rule sent as JSON text nested deeper than
/// any fixed count of rounds still runs, read whole.
#[test]
fn a_value_nested_at_any_depth_runs() {
    let leaf = json_fixture(r#"{"pattern": "eval($X)"}"#);
    let depth = 8;
    let mut rule = leaf.clone();
    for _ in 0..depth {
        rule = json!({"inside": rule.to_string()});
    }
    let input = accepted(
        "astSearch",
        json!({"queries":[{"operation":"match","path":".","rule": rule.to_string()}]}),
    );
    let mut at = &input["queries"][0]["rule"];
    for _ in 0..depth {
        at = &at["inside"];
    }
    assert_eq!(at, &leaf, "{input}");
}

/// Owner decision 2026-11-12: every repair runs and says the form to send.
/// A value read in the shape its field takes is said once per call, of the
/// value as sent and as it runs, which is the form to send, naming the rows
/// (none for a request field); a value nested in JSON text is one value; a
/// field a guess wrote is said by the guess's note alone, with the value it
/// runs with.
#[test]
fn a_value_read_in_its_fields_shape_says_the_form_to_send() {
    for (tool, sent, said) in [
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": "runForwardingSignals"}]
                }"#,
            ),
            r#"queries[0]: read `matchString: "runForwardingSignals"` as `matchString: ["runForwardingSignals"]`; send `matchString: ["runForwardingSignals"]` next time."#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.ts", "limit": "5"}]}"#),
            r#"queries[0]: read `limit: "5"` as `limit: 5`; send `limit: 5` next time."#,
        ),
        (
            "graphBuild",
            json_fixture(r#"{"queries": [{"path": ".", "exclude": "tests/**"}]}"#),
            r#"queries[0]: read `exclude: "tests/**"` as `exclude: ["tests/**"]`; send `exclude: ["tests/**"]` next time."#,
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {"path": ".", "matchString": "x"},
                        {"path": ".", "matchString": "x"}
                    ]
                }"#,
            ),
            r#"queries[0,1]: read `matchString: "x"` as `matchString: ["x"]`; send `matchString: ["x"]` next time."#,
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"]}],
                    "responseChars": "900"
                }"#,
            ),
            r#"read `responseChars: "900"` as `responseChars: 900`; send `responseChars: 900` next time."#,
        ),
        (
            "astSearch",
            json!({"queries":[{"operation":"match","path":".","rule":
                json!({"inside": json_fixture(r#"{"pattern": "eval($X)"}"#).to_string()}).to_string()}]}),
            r#"send `rule: {"inside":{"pattern":"eval($X)"}}` next time."#,
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "pattern": "defects-queue"}]}"#),
            r#"queries[0]: read `pattern` as `matchString: ["defects-queue"]`; send `matchString: ["defects-queue"]` next time."#,
        ),
    ] {
        let input = accepted(tool, sent.clone());
        let notes = normalized_input(tool, sent.clone()).1;
        assert!(
            matches!(notes.as_slice(), [one] if one.ends_with(said)),
            "{tool} {sent} ran as {input}: {notes:?}"
        );
    }
}

/// A note quotes a long value in its own shape, by a text's two ends and a
/// list's first items, so it still says what to send; the notes of a batch
/// come in row order.
#[test]
fn a_long_value_keeps_its_form_and_notes_follow_row_order() {
    let long = format!("start-{}-end!", "m".repeat(60));
    let chars: Vec<char> = long.chars().collect();
    let ends = |kept: usize| -> String {
        chars[..kept]
            .iter()
            .chain(&['…'])
            .chain(&chars[chars.len() - kept..])
            .collect()
    };
    let sent = json!({"queries":[{"path":".","matchString":long}]});
    let notes = normalized_input("localSearch", sent).1;
    let form = format!("`matchString: [\"{}\"]`", ends(NOTE_TEXT_CHARS / 2));
    assert!(
        matches!(notes.as_slice(), [one]
            if one.contains(&format!("as {form}; send {form} next time."))),
        "{notes:?}"
    );
    let many = json_fixture(r#"["a", "b", "c", "d", "e"]"#).to_string();
    let notes = normalized_input(
        "localSearch",
        json!({"queries":[{"path":".","matchString":many}]}),
    )
    .1;
    assert!(
        notes.iter().any(|note| note.contains(
            r#"as `matchString: ["a","b","c",…]`; send `matchString: ["a","b","c",…]` next time"#
        )),
        "{notes:?}"
    );
    let batch = json_fixture(
        r#"{
            "queries": [
                {"path": ".", "matchString": "x"},
                {"path": ".", "matchString": "x"},
                {"path": ".", "pattern": ["y"]}
            ]
        }"#,
    );
    let notes = normalized_input("localSearch", batch).1;
    assert!(
        matches!(notes.as_slice(), [first, second]
            if first.starts_with("queries[0,1]: ") && second.starts_with("queries[2]: ")),
        "{notes:?}"
    );
}

/// A depth sent under another tool's name for it runs as this tool's own,
/// and the call says the field to send.
#[test]
fn a_depth_under_another_tools_name_runs_and_names_the_field() {
    for (tool, sent, field) in [
        (
            "graphQuery",
            json_fixture(
                r#"{
                    "path": ".",
                    "operation": "deps",
                    "target": "a.ts",
                    "maxDepth": 2
                }"#,
            ),
            "depth",
        ),
        (
            "lspSearch",
            json_fixture(
                r#"{
                    "path": "a.ts",
                    "operation": "callers",
                    "symbolName": "run",
                    "line": 1,
                    "maxDepth": 2
                }"#,
            ),
            "depth",
        ),
        (
            "ghCloneRepo",
            json_fixture(r#"{"owner": "o", "repo": "r", "depth": 2}"#),
            "historyDepth",
        ),
    ] {
        let input = accepted(tool, json!({"queries":[sent.clone()]}));
        assert_eq!(input["queries"][0][field], json!(2), "{tool} {input}");
        let notes = normalized_input(tool, json!({"queries":[sent]})).1;
        assert!(
            matches!(notes.as_slice(), [one] if one.ends_with(&format!("send `{field}` next time."))),
            "{tool}: {notes:?}"
        );
    }
}

#[test]
fn a_value_with_one_reading_runs() {
    let row = |input: &Value| input["queries"][0].clone();
    let one = row(&accepted(
        "localSearch",
        json_fixture(r#"{"queries": [{"path": ".", "matchString": "x"}]}"#),
    ));
    assert_eq!(one["matchString"], json_fixture(r#"["x"]"#));
    let context = row(&accepted(
        "localSearch",
        json_fixture(
            r#"{
                "queries": [{"path": ".", "matchString": ["x"], "contextLines": 2}]
            }"#,
        ),
    ));
    assert_eq!(
        context["contextLines"],
        json_fixture(r#"{"before": 2, "after": 2}"#)
    );
    let both = row(&accepted(
        "localSearch",
        json_fixture(
            r#"{
                "queries": [{"path": ".", "matchString": "x", "contextLines": 2}]
            }"#,
        ),
    ));
    assert_eq!(both["matchString"], json_fixture(r#"["x"]"#));
    assert_eq!(
        both["contextLines"],
        json_fixture(r#"{"before": 2, "after": 2}"#)
    );
    let quoted = row(&accepted(
        "localSearch",
        json_fixture(r#"{"queries": [{"path": ".", "matchString": "[\"x\",\"y\"]"}]}"#),
    ));
    assert_eq!(quoted["matchString"], json_fixture(r#"["x", "y"]"#));
    let number = row(&accepted(
        "localFetch",
        json_fixture(r#"{"queries": [{"path": 5}]}"#),
    ));
    assert_eq!(number["path"], json!("5"));
    let single = row(&accepted(
        "localFetch",
        json_fixture(r#"{"queries": [{"path": ["a.ts"]}]}"#),
    ));
    assert_eq!(single["path"], json!("a.ts"));
}

#[test]
fn a_shape_with_one_reading_runs() {
    let bare = accepted(
        "localSearch",
        json_fixture(r#"{"path": ".", "matchString": ["x"]}"#),
    );
    assert_eq!(bare["queries"][0]["path"], json!("."));
    let with_budget = accepted(
        "localSearch",
        json_fixture(r#"{"path": ".", "matchString": ["x"], "responseChars": 900}"#),
    );
    assert_eq!(with_budget["responseChars"], json!(900));
    assert!(with_budget["queries"][0].get("responseChars").is_none());
    let list = accepted("localFetch", json_fixture(r#"[{"path": "a.ts"}]"#));
    assert_eq!(list["queries"][0]["path"], json!("a.ts"));
    let single = accepted(
        "localFetch",
        json_fixture(r#"{"queries": {"path": "a.ts"}}"#),
    );
    assert_eq!(single["queries"][0]["path"], json!("a.ts"));
    let quoted = accepted(
        "localFetch",
        json_fixture(r#"{"queries": "[{\"path\":\"a.ts\"}]"}"#),
    );
    assert_eq!(quoted["queries"][0]["path"], json!("a.ts"));
    let lifted = accepted(
        "localFetch",
        json_fixture(
            r#"{
                "queries": [
                    {"path": "a.ts", "responseChars": 100},
                    {"path": "b.ts", "responseChars": 100}
                ]
            }"#,
        ),
    );
    assert_eq!(lifted["responseChars"], json!(100));
    let spread = accepted(
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
    );
    assert_eq!(spread["queries"][0]["view"], json!("files"));
    assert_eq!(spread["queries"][1]["view"], json!("content"));
}

/// A list or scalar in place of a row is reported as what was sent.
#[test]
fn a_non_object_row_is_reported_as_sent() {
    for sent in [
        json_fixture(r#"{"queries": [["a", "b"]]}"#),
        json_fixture(r#"{"queries": ["a"]}"#),
    ] {
        let input = normalized_input("localSearch", sent.clone()).0;
        assert_eq!(input, sent);
    }
}

/// Fields only one operation takes name it; fields several take do not.
#[test]
fn fields_only_one_operation_takes_name_it() {
    let tree = accepted(
        "ghStructure",
        json_fixture(
            r#"{
                "queries": [{"owner": "o", "repo": "r", "path": "src", "maxDepth": 1}]
            }"#,
        ),
    );
    assert_eq!(tree["queries"][0]["operation"], json!("tree"));
    let search = accepted(
        "ghSearchRepo",
        json_fixture(r#"{"queries": [{"keywords": ["x"], "limit": 3}]}"#),
    );
    assert_eq!(search["queries"][0]["operation"], json!("search"));
    let many = normalized_input(
        "lspSearch",
        json_fixture(
            r#"{
                "queries": [{"path": "a.ts", "symbolName": "a", "line": 1}]
            }"#,
        ),
    )
    .0;
    assert!(many["queries"][0].get("operation").is_none());
}

#[test]
fn a_sort_takes_its_natural_direction() {
    let row = |sort: Value| {
        accepted(
            "structureSearch",
            json!({"queries":[{"path":".","operation":"files","sort":sort}]}),
        )["queries"][0]["sort"]
            .clone()
    };
    assert_eq!(
        row(json!("path")),
        json_fixture(r#"{"by": "path", "order": "asc"}"#)
    );
    assert_eq!(
        row(json_fixture(r#"{"by": "bytes"}"#)),
        json_fixture(r#"{"by": "bytes", "order": "desc"}"#)
    );
    assert_eq!(
        row(json_fixture(r#"{"by": "bytes", "order": "asc"}"#)),
        json_fixture(r#"{"by": "bytes", "order": "asc"}"#),
        "a stated direction stays"
    );
}

/// More than one reading, or a guess: the agent decides, as the error asks.
#[test]
fn what_has_no_single_reading_stays_rejected() {
    for (tool, sent) in [
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": ["a.ts", "b.ts"]}]}"#),
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.ts", "ranges": "1-2"}]}"#),
        ),
        (
            "localFetch",
            json_fixture(r#"{"query": {"path": "README.md"}}"#),
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "view": "file"}]
                }"#,
            ),
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [
                        {"path": "a", "responseChars": 1},
                        {"path": "b", "responseChars": 2}
                    ]
                }"#,
            ),
        ),
        // No declared field says what these mean.
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "path": ".",
                            "matchString": ["x"],
                            "matchContentLength": 200
                        }
                    ]
                }"#,
            ),
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "owner": "o",
                            "repo": "r",
                            "operation": "all",
                            "keywords": ["x"]
                        }
                    ]
                }"#,
            ),
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "multiline": "yes"}]
                }"#,
            ),
        ),
    ] {
        let input = normalized_input(tool, sent.clone()).0;
        let failed = prepare_rows(tool, input).map_or(true, |rows| rows.iter().any(Result::is_err));
        assert!(failed, "{sent} must stay rejected");
    }
}

/// An error names only what repair could not settle, never a field it repaired.
#[test]
fn an_error_names_only_what_repair_left() {
    let sent = json_fixture(
        r#"{
            "queries": [
                {"path": ".", "matchString": "x", "serchText": "typo"},
                {"path": ".", "matchString": "y"}
            ]
        }"#,
    );
    let input = normalized_input("localSearch", sent).0;
    assert_eq!(input["queries"][0]["serchText"], json!("typo"));
    assert_eq!(input["queries"][0]["matchString"], json_fixture(r#"["x"]"#));
    assert!(input["queries"][0].get("include").is_none());
    assert_eq!(input["queries"][1]["matchString"], json_fixture(r#"["y"]"#));
    let rows = prepare_rows("localSearch", input).expect("rows");
    let error = rows[0].as_ref().expect_err("the typo stays");
    let shown = format!("{:?}", error.issues);
    assert!(shown.contains("serchText"), "{shown}");
    assert!(!shown.contains("Expected a JSON array"), "{shown}");
}

#[test]
fn a_valid_call_is_untouched() {
    let sent = json_fixture(
        r#"{
            "queries": [{"path": ".", "matchString": ["x"]}],
            "responseChars": 500
        }"#,
    );
    assert_eq!(normalized_input("localSearch", sent.clone()).0, sent);
}

/// A page size beside a cursor has one reading: the cursor keeps the size it
/// was issued with. The field is dropped, whatever its value, and the call
/// says so; any other field beside a cursor has no reading and stays for
/// validation to refuse.
#[test]
fn a_page_size_beside_a_cursor_is_dropped_and_said() {
    for size in [json!(60_000), json!("60000"), json!(null)] {
        let sent = json!({"cursor":"issued","responseChars":size});
        let (input, notes) = normalized_input("localSearch", sent.clone());
        assert_eq!(input, json_fixture(r#"{"cursor": "issued"}"#), "{sent}");
        assert_eq!(notes.len(), 1, "{sent}: {notes:?}");
        assert!(
            notes[0].contains("`responseChars`")
                && notes[0].contains("page size it was issued with"),
            "{notes:?}"
        );
        crate::contracts::validate("localSearch", input).expect("the cursor alone is valid");
    }
    for sent in [
        json_fixture(r#"{"cursor": "issued", "path": "."}"#),
        json_fixture(r#"{"cursor": "issued", "responseChars": 900, "path": "."}"#),
        json_fixture(
            r#"{
                "cursor": "issued",
                "queries": [{"path": ".", "matchString": ["x"]}],
                "responseChars": 900
            }"#,
        ),
    ] {
        let (input, notes) = normalized_input("localSearch", sent.clone());
        assert!(notes.is_empty(), "{sent}: {notes:?}");
        assert_eq!(input, sent);
        crate::contracts::validate("localSearch", input).expect_err("refused");
    }
}
