use super::*;
use serde_json::json;
use std::cell::Cell;

/// One computation per scope and input text: a repeat reads it back, equal,
/// and another scope or another input computes its own.
#[test]
fn a_result_is_computed_once_per_scope_and_input() {
    static MEMO: Remembered<Value> = Remembered::new();
    let runs = Cell::new(0);
    let compute = |input: &Value| {
        runs.set(runs.get() + 1);
        json!({"checked": input})
    };
    let scope = "checked-tests-a-result-is-computed-once";
    let input = json!({"queries": [{"path": "a", "n": 1}]});
    let first = MEMO.get_or(scope, &input, || compute(&input), |_| 1);
    let again = MEMO.get_or(scope, &input, || compute(&input), |_| 1);
    assert_eq!(first, again);
    assert_eq!(runs.get(), 1);
    let other_scope = format!("{scope}-other");
    assert_eq!(
        MEMO.get_or(&other_scope, &input, || compute(&input), |_| 1),
        first
    );
    assert_eq!(runs.get(), 2);
    let other = json!({"queries": [{"path": "a", "n": 2}]});
    assert_eq!(
        MEMO.get_or(scope, &other, || compute(&other), |_| 1),
        json!({"checked": other})
    );
    assert_eq!(runs.get(), 3);
}

/// Field order is part of the text: the same fields in another order are
/// another input, since a checked input keeps its order.
#[test]
fn inputs_that_differ_only_in_field_order_are_kept_apart() {
    static MEMO: Remembered<String> = Remembered::new();
    let scope = "checked-tests-field-order";
    let ab: Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
    let ba: Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
    let text = |value: &Value| serde_json::to_string(value).unwrap();
    assert_eq!(
        MEMO.get_or(scope, &ab, || text(&ab), String::len),
        r#"{"a":1,"b":2}"#
    );
    assert_eq!(
        MEMO.get_or(scope, &ba, || text(&ba), String::len),
        r#"{"b":2,"a":1}"#
    );
}

/// Each remembered check answers as the contract check it stands for, on a
/// first call and on a repeat, valid or not.
#[test]
fn remembered_checks_answer_as_the_checks_they_stand_for() {
    let related = json!({"queries": [{"path": "checked-tests-related", "matchString": ["x"]}]});
    let invalid = json!({"queries": [{"path": 7}]});
    for _ in 0..2 {
        for input in [&related, &invalid] {
            assert_eq!(
                checked_input(ToolId::LocalSearch, true, input),
                octocode_runtime_base::response::continuations::checked_input(
                    ToolId::LocalSearch,
                    true,
                    input
                )
            );
            assert_eq!(
                prepared_rows("localSearch", input, false),
                contracts::prepare_rows("localSearch", input.clone())
            );
            assert_eq!(
                normalized_input("localSearch", input.clone()),
                contracts::normalized_input("localSearch", input.clone())
            );
        }
        assert_eq!(
            selection_only_fields("localSearch"),
            contracts::selection_only_fields("localSearch")
        );
    }
}
