use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInputError {
    message: String,
}

impl ContractInputError {
    fn new(message: impl Into<String>) -> Self {
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
/// Accepts a single query object directly, or `{ "queries": [q] }` with
/// exactly one element (kept for backward-compatibility with continuation
/// tokens). Multiple queries in one call are not supported.
pub fn prepare(
    tool_name: &str,
    input: Value,
    options: PrepareOptions<'_>,
) -> Result<PreparedQuery, ContractInputError> {
    let mut object = match input {
        Value::Array(_) => {
            return Err(ContractInputError::new(
                "multiple queries are not supported; send one query object directly",
            ));
        }
        Value::Object(mut object) => match object.remove("queries") {
            Some(Value::Array(mut values)) => {
                if values.len() != 1 {
                    return Err(ContractInputError::new(
                        "multiple queries are not supported; send one query object directly",
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
    default_blank(
        &mut object,
        "goal",
        format!("Execute {tool_name} via {}", options.source_label),
    );
    object
        .entry("debug".to_owned())
        .or_insert(Value::Bool(false));
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
    fn rejects_arrays_and_multiple_queries() {
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
    fn accepts_single_element_queries_array_for_continuation_compat() {
        let prepared = prepare(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a"}]}),
            PrepareOptions::default(),
        )
        .expect("single-element queries array");
        assert_eq!(prepared.query["path"], "/tmp/a");
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
