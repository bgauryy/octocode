//! The rule of lenient input, held for every tool: a request is read in
//! another form only where its schema alone gives one typed reading, each
//! reading is said once, and nothing else is read. The requests are core's
//! own examples, one per form of each tool's rows, so a field or tool added
//! to the contract is covered without a line here.
use super::*;
use crate::contracts::validate::validate_batch;
use crate::contracts::{format_input_error, prepare_rows};
use octocode_test_support::json_fixture;
use serde_json::json;

mod beside_cursor;
mod readings;
mod strictness;

/// Core's example row of each form of each tool's rows.
fn examples() -> Vec<(ToolId, Value)> {
    let presentation = crate::contracts::cli_presentation().expect("presentation");
    let rows: Vec<(ToolId, Value)> = presentation["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .flat_map(|entry| {
            let id = ToolId::from_name(entry["name"].as_str().expect("name")).expect("tool");
            entry["views"]["variants"]["variants"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |variant| Some((id, variant.get("example")?.clone())))
        })
        .collect();
    for tool in ToolId::ALL {
        assert!(
            rows.iter().any(|(id, _)| *id == tool),
            "{tool:?} has examples"
        );
    }
    rows
}

/// The request holding `row` alone, as the contract declares it.
fn request(row: &Value) -> Value {
    json!({ "queries": [row] })
}

/// The issues of the one row of `request`, and of its envelope.
fn issues(tool: ToolId, request: &Value) -> Vec<ValidationIssue> {
    let (_, batch) = validate_batch(tool.as_str(), request.clone(), false).expect("contract");
    batch
        .rows
        .into_iter()
        .flatten()
        .chain(batch.envelope)
        .collect()
}

/// What `tool` answers a refused `request` with; `None` when it runs.
fn refusal(tool: ToolId, request: &Value) -> Option<String> {
    let error = match prepare_rows(tool.as_str(), request.clone()) {
        Err(error) => error,
        Ok(rows) => rows.into_iter().find_map(Result::err)?,
    };
    let shown = format_input_error(tool.as_str(), &error, request);
    Some(shown["error"].as_str().expect("an error line").to_owned())
}

/// `row` with `field` set to `value`.
fn with(row: &Value, field: &str, value: Value) -> Value {
    let mut row = row.clone();
    row[field] = value;
    row
}

/// Every example is valid as sent, and a valid call is untouched and says
/// nothing.
#[test]
fn a_valid_call_is_untouched() {
    for (tool, row) in examples() {
        let sent = request(&row);
        assert_eq!(refusal(tool, &sent), None, "{tool:?} {row}");
        assert_eq!(
            normalized_readings(tool.as_str(), sent.clone()),
            (sent, Vec::new()),
            "{tool:?}"
        );
    }
}

/// A list or scalar in place of a row is reported as what was sent.
#[test]
fn a_non_object_row_is_reported_as_sent() {
    for sent in [
        json_fixture(r#"{"queries": [["a", "b"]]}"#),
        json_fixture(r#"{"queries": ["a"]}"#),
    ] {
        assert_eq!(
            normalized_readings("localSearch", sent.clone()),
            (sent, Vec::new())
        );
    }
}

/// A page size beside a cursor has one reading: the cursor keeps the size it
/// was issued with. The field is dropped, whatever its value, and the call
/// says so; any other field beside a cursor has no reading and stays for
/// validation to refuse.
#[test]
fn a_page_size_beside_a_cursor_is_dropped_and_said() {
    for size in [json!(60_000), json!("60000"), json!(null)] {
        let sent = json!({"cursor":"issued","responseChars":size});
        let (input, notes) = normalized_readings("localSearch", sent.clone());
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
        let (input, notes) = normalized_readings("localSearch", sent.clone());
        assert!(notes.is_empty(), "{sent}: {notes:?}");
        assert_eq!(input, sent);
        crate::contracts::validate("localSearch", input).expect_err("refused");
    }
}
