//! The input's shape before any row is validated: an envelope of object
//! rows or a saved-response read. A shape the contract does not take is rejected
//! with an issue whose rule the repair reads (`validate/repair.rs`), never
//! reshaped here: a nested envelope's fields would be lost silently.

use serde_json::{Value, json};

use super::ValidationIssue;

/// The rejection for input with neither fresh queries nor a saved-response token.
const ENVELOPE_REQUIRED: &str = "Fresh calls require {\"queries\":[...]}; saved pages require responseSnapshot without queries.";

/// Rule of a bare array of rows: the repair wraps it as `queries`.
pub(crate) const ENVELOPE_ARRAY: &str = "envelope.array";
/// Rule of a row that is itself an envelope: the repair lifts its rows.
pub(crate) const ENVELOPE_NESTED: &str = "envelope.nested";
/// Rule of any other shape the envelope does not take (no repair).
const ENVELOPE_SHAPE: &str = "envelope.shape";

/// The shape issues of `input` for `tool_name`; empty when it is the
/// envelope of object rows or a saved-response read for canonical validation.
pub(super) fn envelope_issues(tool_name: &str, input: &Value) -> Vec<ValidationIssue> {
    let issue =
        |rule: &str, path: Vec<String>, message: String, schema: Option<Value>| ValidationIssue {
            rule_id: rule.to_owned(),
            path,
            message,
            schema,
            received: None,
        };
    let Some(rows) = input.as_object().and_then(|object| object.get("queries")) else {
        if input
            .as_object()
            .is_some_and(|object| object.contains_key("responseSnapshot"))
        {
            return Vec::new();
        }
        let rule = if input.as_array().is_some_and(|rows| !rows.is_empty()) {
            ENVELOPE_ARRAY
        } else {
            ENVELOPE_SHAPE
        };
        return vec![issue(rule, Vec::new(), ENVELOPE_REQUIRED.to_owned(), None)];
    };
    let Some(rows) = rows.as_array() else {
        return vec![ValidationIssue {
            rule_id: "schema.type".into(),
            path: vec!["queries".to_owned()],
            message: "queries must be an array of query objects.".into(),
            schema: super::tool_contract_named(tool_name)
                .and_then(Result::ok)
                .and_then(|contract| {
                    let input = &contract["inputSchema"];
                    super::schema_facts::property_schemas(input, &input["$defs"], "queries")
                        .into_iter()
                        .find(|schema| schema.get("items").is_some())
                        .cloned()
                }),
            received: Some(rows.clone()),
        }];
    };
    let envelope_fields = || {
        let fields = super::tool_contract_named(tool_name)
            .and_then(Result::ok)
            .map(|tool| {
                let input = &tool["inputSchema"];
                super::schema_facts::property_names(input, &input["$defs"])
                    .into_iter()
                    .filter(|key| *key != "queries")
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        json!({ "envelopeFields": fields })
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let nested = match row {
                Value::Object(row) if row.contains_key("queries") => true,
                Value::Object(_) => return None,
                Value::Array(_) => true,
                _ => false,
            };
            let path = vec!["queries".to_owned(), index.to_string()];
            Some(if nested {
                let mut path = path;
                path.push("queries".into());
                issue(
                    ENVELOPE_NESTED,
                    path,
                    "A row holds its own queries: put those rows directly in queries[] and its other fields (responseLength, …) beside queries. Nothing ran.".to_owned(),
                    Some(envelope_fields()),
                )
            } else {
                issue(
                    ENVELOPE_SHAPE,
                    path,
                    format!("queries[{index}] must be a query object."),
                    None,
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ENVELOPE_ARRAY, ENVELOPE_NESTED, ENVELOPE_SHAPE, envelope_issues};
    use serde_json::json;

    /// Artifact coordinates are trimmed once, by the contract's own
    /// normalization rule, on every validated path.
    #[test]
    fn artifact_coordinates_are_trimmed_by_the_contract() {
        let validated = crate::contracts::prepare_many_and_validate(
            "artifactSearch",
            json!({"queries": [
                {"ecosystem": "npm", "packageName": "  express \n"},
                {"ecosystem": "npm", "keywords": [" http ", "server"]}
            ]}),
        )
        .expect("valid artifact queries");
        assert_eq!(validated[0]["packageName"], "express");
        assert_eq!(validated[1]["keywords"], json!(["http", "server"]));
    }

    #[test]
    fn pure_clasify_preparation_preserves_exactly_the_supplied_values() {
        let query = json!({"id":"decision","mainGoal": "test", "reasoning":"Decide the next evidence read.","state":{"text":"one"}, "questions":{"relevant":{
            "type": "noul", "instructions": "Assess supplied state"
        }}});
        let prepared =
            crate::contracts::prepare_many_and_validate("clasify", json!({"queries": [query]}))
                .expect("pure input");
        assert_eq!(prepared, [query]);
    }

    /// Fresh rows and saved reads pass the shape preflight; other shapes
    /// produce issues the repair can read, never a silent reshape.
    #[test]
    fn fresh_and_saved_envelopes_have_no_shape_issue() {
        let rules = |input| {
            envelope_issues("localFetch", &input)
                .into_iter()
                .map(|issue| (issue.rule_id, issue.path.join(".")))
                .collect::<Vec<_>>()
        };
        assert!(rules(json!({"queries":[{"path":"/a"},{"path":"/b"}]})).is_empty());
        assert!(rules(json!({"responseSnapshot":"saved","responseOffset":1})).is_empty());
        assert_eq!(
            rules(json!([{"path":"/a"}])),
            [(ENVELOPE_ARRAY.to_owned(), String::new())]
        );
        for empty in [json!([]), json!("x"), json!({"responseLength": 9})] {
            assert_eq!(rules(empty), [(ENVELOPE_SHAPE.to_owned(), String::new())]);
        }
        assert_eq!(
            rules(
                json!({"queries":[{"path":"/a"}, 1, {"queries":[{"path":"/b"}]}, [{"path":"/c"}]]})
            ),
            [
                (ENVELOPE_SHAPE.to_owned(), "queries.1".to_owned()),
                (ENVELOPE_NESTED.to_owned(), "queries.2.queries".to_owned()),
                (ENVELOPE_NESTED.to_owned(), "queries.3.queries".to_owned()),
            ]
        );
    }

    #[test]
    fn saved_response_envelopes_validate_without_query_rows() {
        let input = json!({
            "responseSnapshot":"saved",
            "responseOffset":1,
            "responseLength":1000,
            "responseScope":"rows"
        });
        let rows = crate::contracts::prepare_many_and_validate("localFetch", input.clone())
            .expect("valid saved response envelope");
        assert!(
            rows.is_empty(),
            "saved reads must not prepare fresh queries"
        );
        assert_eq!(
            crate::contracts::validate("localFetch", input.clone())
                .expect("saved response fields remain canonical"),
            input
        );
    }

    #[test]
    fn saved_response_envelopes_keep_canonical_validation() {
        for input in [
            json!({"responseSnapshot":"saved","queries":[{"path":"a.rs"}]}),
            json!({"responseSnapshot":null}),
            json!({"responseSnapshot":123}),
            json!({"responseSnapshot":""}),
            json!({"responseSnapshot":"saved","responseOffset":-1}),
            json!({"responseSnapshot":"saved","responseScope":"content.text"}),
            json!({"responseSnapshot":"saved","unexpected":true}),
        ] {
            let expected = crate::contracts::validate("localFetch", input.clone())
                .expect_err("canonical saved response rejection");
            let actual = crate::contracts::prepare_many_and_validate("localFetch", input.clone())
                .expect_err("preparation must preserve canonical rejection");
            assert_eq!(actual, expected, "{input}");
        }
    }

    /// A nested envelope is rejected whatever it carries: its sibling
    /// fields are never dropped silently, and the message names no
    /// internal function.
    #[test]
    fn a_nested_envelope_is_rejected_not_unwrapped() {
        for inner in [
            json!([{"path":"/a","matchString":"fn "}]),
            json!([{"path":"/a","matchString":"fn "},{"path":"/b","matchString":"fn "}]),
        ] {
            let input = json!({"queries":[{"bogus":1,"responseLength":3000,"queries":inner}]});
            let error = crate::contracts::prepare_many_and_validate("localSearch", input)
                .expect_err("nested envelope");
            assert_eq!(error.issues.len(), 1, "{error:?}");
            assert_eq!(error.issues[0].rule_id, ENVELOPE_NESTED);
            assert!(!error.issues[0].message.contains("prepare"), "{error:?}");
            let fields = &error.issues[0].schema.as_ref().expect("schema")["envelopeFields"];
            assert!(
                fields
                    .as_array()
                    .is_some_and(|f| f.contains(&json!("responseLength"))),
                "{fields}"
            );
        }
    }
}
