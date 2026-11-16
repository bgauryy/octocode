use super::*;
use crate::runtime::RuntimeError;
use serde_json::json;
mod outcomes;
mod rows;
mod spelling;

/// One (exit code, isError) pairing: a call flagged as a tool error never
/// exits as success or empty, and a failed or rejected outcome whose rows
/// all failed is flagged.
#[test]
fn the_error_flag_and_the_exit_class_agree() {
    let answered = json!({"results":[{"data":{}}]});
    let failed =
        json!({"results":[{"index":0,"status":"error","data":{"errorCode":"invalidInput"}}]});
    let mixed = json!({"results":[{"data":{}},{"index":1,"status":"error","data":{"errorCode":"invalidInput"}}]});
    for (value, all_failed, flagged) in [
        (answered, false, false),
        (failed, true, true),
        (mixed, false, false),
    ] {
        let call = outcome(value.clone(), None, all_failed);
        assert_eq!(call.is_error(), flagged, "{value}");
        if call.is_error() {
            assert!(
                !matches!(call.exit_class(), ExitClass::Success | ExitClass::Empty),
                "{value}"
            );
        }
    }
}

fn all_rows_invalid_input(value: &Value) -> bool {
    value["results"].as_array().is_some_and(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| {
                row["data"]["errorCode"]
                    .as_str()
                    .is_some_and(|code| crate::id::error_class(code) == ErrorClass::CorrectInput)
            })
    })
}

pub(super) fn outcome(
    structured_content: Value,
    failure: Option<ErrorClass>,
    all_failed: bool,
) -> ToolOutcome {
    ToolOutcome {
        structured_content: crate::security::Sanitized::sealed(structured_content),
        failure,
        all_failed,
        continued: None,
    }
}
