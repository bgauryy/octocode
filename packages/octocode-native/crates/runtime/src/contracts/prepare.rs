use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInputError {
    message: String,
}

impl ContractInputError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ContractInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ContractInputError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareOptions<'a> {
    pub source_label: &'a str,
}

impl Default for PrepareOptions<'_> {
    fn default() -> Self {
        Self {
            source_label: "direct tool execution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedQuery {
    pub query: Map<String, Value>,
}

/// Applies canonical meta-field defaults. Shape and relation validation is a
/// later stage and must never rewrite tool fields or delegate to Node.
/// Singular preparation helper for continuations and internal callers.
/// Public bulk requests must use `prepare_many_and_validate`; singleton array
/// and envelope forms remain accepted here for cursor compatibility.
pub fn prepare(
    tool_name: &str,
    input: Value,
    options: PrepareOptions<'_>,
) -> Result<PreparedQuery, ContractInputError> {
    let mut object = match input {
        Value::Array(mut values) => {
            if values.len() != 1 {
                return Err(ContractInputError::new(
                    "singular preparation received multiple queries; use prepare_many_and_validate",
                ));
            }
            match values.remove(0) {
                Value::Object(query) => query,
                _ => return Err(ContractInputError::new("query must be an object")),
            }
        }
        Value::Object(mut object) => match object.remove("queries") {
            Some(Value::Array(mut values)) => {
                if values.len() != 1 {
                    return Err(ContractInputError::new(
                        "singular preparation received multiple queries; use prepare_many_and_validate",
                    ));
                }
                match values.remove(0) {
                    Value::Object(q) => q,
                    _ => return Err(ContractInputError::new("query must be an object")),
                }
            }
            Some(_) => return Err(ContractInputError::new("queries must be an array")),
            None => object,
        },
        _ => {
            return Err(ContractInputError::new("tool input must be an object"));
        }
    };
    if tool_name != "jev" {
        default_blank(
            &mut object,
            "goal",
            format!("Execute {tool_name} via {}", options.source_label),
        );
        object
            .entry("debug".to_owned())
            .or_insert(Value::Bool(false));
    }
    if tool_name == "artifactSearch" {
        trim_string(&mut object, "packageName");
        if let Some(Value::Array(keywords)) = object.get_mut("keywords") {
            for keyword in keywords {
                if let Value::String(value) = keyword {
                    *value = value.trim().to_owned();
                }
            }
        }
    }
    Ok(PreparedQuery { query: object })
}

fn trim_string(object: &mut Map<String, Value>, field: &str) {
    if let Some(Value::String(value)) = object.get_mut(field) {
        *value = value.trim().to_owned();
    }
}

fn default_blank(object: &mut Map<String, Value>, field: &str, default: String) {
    let blank = match object.get(field) {
        None => true,
        Some(Value::String(value)) => value.trim().is_empty(),
        Some(_) => false,
    };
    if blank {
        object.insert(field.to_owned(), Value::String(default));
    }
}

#[cfg(test)]
mod tests {
    use super::{PrepareOptions, prepare};
    use serde_json::json;

    #[test]
    fn pure_jev_preparation_preserves_exactly_the_supplied_values() {
        let query = json!({"reasoning":"Decide the next evidence read.","context": {"value": {"goal": "source data", "debug": true}}, "question": {
            "type": "noul", "instructions": "Assess supplied state"
        }});
        for input in [
            query.clone(),
            json!([query.clone()]),
            json!({"queries": [query.clone()]}),
        ] {
            let prepared = prepare("jev", input, PrepareOptions::default()).expect("pure input");
            assert_eq!(serde_json::Value::Object(prepared.query), query);
        }
    }

    #[test]
    fn defaults_goal_and_debug_but_never_invents_reasoning() {
        let prepared = prepare(
            "localFetch",
            json!({"path":"/tmp/a", "goal":" "}),
            PrepareOptions {
                source_label: "native CLI",
            },
        )
        .expect("valid input");
        assert_eq!(prepared.query["goal"], "Execute localFetch via native CLI");
        assert_eq!(prepared.query["debug"], false);
        assert!(prepared.query.get("reasoning").is_none());
    }

    #[test]
    fn rejects_empty_invalid_and_multiple_query_arrays() {
        assert!(prepare("localFetch", json!([]), PrepareOptions::default()).is_err());
        assert!(prepare("localFetch", json!([1]), PrepareOptions::default()).is_err());
        assert!(
            prepare(
                "localFetch",
                json!({"queries":[{"path":"/a"},{"path":"/b"}]}),
                PrepareOptions::default()
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_single_element_arrays_for_cli_and_continuation_compat() {
        for input in [
            json!([{"path":"/tmp/a"}]),
            json!({"queries":[{"path":"/tmp/a"}]}),
        ] {
            let prepared = prepare("localFetch", input, PrepareOptions::default())
                .expect("single-element query array");
            assert_eq!(prepared.query["path"], "/tmp/a");
        }
    }

    #[test]
    fn does_not_rewrite_tool_fields() {
        let prepared = prepare(
            "astSearch",
            json!({"operation":"syntax","path":"/tmp/lib.rs"}),
            PrepareOptions::default(),
        )
        .expect("envelope only");
        assert_eq!(prepared.query["operation"], "syntax");
        assert!(prepared.query.get("treeKind").is_none());
    }
}
