//! What the example beside a refusal shows, and which coercions warn: the
//! example keeps a valid value where the row's own cannot stand, invents no
//! real-looking value, and a coercion is always said.
use super::repair_tests::{answered, shown_example};
use crate::contracts::normalized_input;
use serde_json::{Value, json};

fn example_line(lines: &[String]) -> Option<&String> {
    lines.iter().find(|line| line.starts_with("Example: "))
}

/// A text outside an enum that no sibling or near spelling explains is shown
/// as the allowed member nearest to it in spelling, never dropped with its
/// field: `merged` for an issue search is nearest to `closed`.
#[test]
fn a_refused_enum_text_is_shown_as_its_nearest_member() {
    let shown = shown_example(
        "ghSearchHistory",
        json!({"operation":"issue","owner":"pallets","repo":"flask","filters":{"state":"merged"}}),
    );
    assert_eq!(shown["filters"]["state"], "closed", "{shown}");
}

/// A required number the row never sent has no placeholder that cannot read
/// as a real one (`1` is a real issue number), so no example is shown and
/// the refusal names the field.
#[test]
fn a_missing_required_number_is_named_not_invented() {
    let sent =
        json!({"queries":[{"operation":"pullRequest","owner":"expressjs","repo":"express"}]});
    let (_, lines) = answered("ghGetHistoryItem", &sent);
    assert!(lines[0].contains("number"), "{lines:?}");
    assert!(example_line(&lines).is_none(), "{lines:?}");
}

/// A scalar sent for an object whose fields it fits equally is shown under
/// the first field that takes it, so the example holds the value sent.
#[test]
fn a_scalar_for_an_object_of_several_fields_gets_an_example() {
    let shown = shown_example(
        "astSearch",
        json!({"operation":"match","path":".","rule":"identifier"}),
    );
    let rule = shown["rule"].as_object().expect("an object rule");
    assert!(rule.values().any(|value| value == "identifier"), "{shown}");
}

/// A number written as an integral float is read as the integer, and said.
#[test]
fn an_integral_float_for_an_integer_warns() {
    let sent = json!({"queries":[{"path":".","matchString":["a"],"limit":3.0}]});
    let (_, warnings) = normalized_input("localSearch", sent);
    assert!(
        warnings.iter().any(|note| note.contains("limit")),
        "{warnings:?}"
    );
}

/// An out-of-range value for a field the contract says how to find is
/// refused with that guidance, as a missing one is.
#[test]
fn an_out_of_range_line_says_how_to_find_it() {
    for line in [json!(0), json!(-3)] {
        let sent = json!({"queries":[{"operation":"definition","path":"a.ts","symbolName":"x","line":line}]});
        let (_, lines) = answered("lspSearch", &sent);
        assert!(
            lines[0].contains("localSearch or astSearch"),
            "{line}: {lines:?}"
        );
        let _: &Value = &line;
    }
}
