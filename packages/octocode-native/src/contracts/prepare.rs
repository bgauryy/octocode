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
pub struct PreparedBatch {
    pub queries: Vec<Map<String, Value>>,
    #[serde(flatten)]
    pub envelope: Map<String, Value>,
}

/// Applies the canonical envelope and meta-field defaults. Shape and relation
/// validation is a later stage and must never rewrite tool fields or delegate
/// to Node.
pub fn prepare(
    tool_name: &str,
    input: Value,
    options: PrepareOptions<'_>,
) -> Result<PreparedBatch, ContractInputError> {
    let (queries, envelope) = match input {
        Value::Array(values) => (values, Map::new()),
        Value::Object(mut object) => match object.remove("queries") {
            Some(Value::Array(values)) => (values, object),
            Some(_) => return Err(ContractInputError::new("queries must be an array")),
            None => (vec![Value::Object(object)], Map::new()),
        },
        _ => {
            return Err(ContractInputError::new(
                "tool input must be an object or array of objects",
            ));
        }
    };
    if queries.is_empty() {
        return Err(ContractInputError::new("at least one query is required"));
    }
    let mut prepared = Vec::with_capacity(queries.len());
    for query in queries {
        let Value::Object(mut object) = query else {
            return Err(ContractInputError::new("each query must be an object"));
        };
        default_blank(
            &mut object,
            "goal",
            format!("Execute {tool_name} via {}", options.source_label),
        );
        default_blank(
            &mut object,
            "reasoning",
            format!("Executed via {} tool command", options.source_label),
        );
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
        prepared.push(object);
    }
    Ok(PreparedBatch {
        queries: prepared,
        envelope,
    })
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
    fn wraps_one_query_and_defaults_blank_meta_fields() {
        let prepared = prepare(
            "localFetch",
            json!({"path":"/tmp/a", "goal":" "}),
            PrepareOptions {
                source_label: "native CLI",
            },
        )
        .expect("valid input");
        assert_eq!(
            prepared.queries[0]["goal"],
            "Execute localFetch via native CLI"
        );
        assert_eq!(
            prepared.queries[0]["reasoning"],
            "Executed via native CLI tool command"
        );
    }

    #[test]
    fn rejects_empty_or_non_object_queries() {
        assert!(prepare("localFetch", json!([]), PrepareOptions::default()).is_err());
        assert!(prepare("localFetch", json!([1]), PrepareOptions::default()).is_err());
    }

    #[test]
    fn does_not_rewrite_tool_fields() {
        let prepared = prepare(
            "astSearch",
            json!({"operation":"syntax","path":"/tmp/lib.rs"}),
            PrepareOptions::default(),
        )
        .expect("envelope only");
        assert_eq!(prepared.queries[0]["operation"], "syntax");
        assert!(prepared.queries[0].get("treeKind").is_none());
    }
}
