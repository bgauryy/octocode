use serde_json::Value;

use super::{ContractValidationError, internal, issue, query_values};

pub(super) fn validate(
    opcode: &str,
    args: &Value,
    input: &Value,
) -> Result<(), ContractValidationError> {
    let field = |name: &str| {
        args[name]
            .as_str()
            .ok_or_else(|| internal(format!("{opcode}.{name} must be a string")))
    };
    let mut issues = Vec::new();
    for (index, row) in query_values(input) {
        let fail = |path: Vec<String>, message: String| {
            let mut at = vec!["queries".into(), index.to_string()];
            at.extend(path);
            issue(opcode, at, message)
        };
        let error = match opcode {
            "requires_field" => {
                let (trigger, required) = (field("field")?, field("requires")?);
                (row.get(trigger).is_some() && row.get(required).is_none()).then(|| {
                    fail(
                        vec![required.into()],
                        format!("{trigger} requires {required}."),
                    )
                })
            }
            "requires_any" => {
                if args
                    .get("operation")
                    .is_some_and(|operation| row.get("operation") != Some(operation))
                {
                    continue;
                }
                let fields = args["fields"]
                    .as_array()
                    .ok_or_else(|| internal("requires_any.fields must be an array".into()))?;
                (!fields
                    .iter()
                    .any(|name| name.as_str().is_some_and(|name| row.get(name).is_some())))
                .then(|| {
                    fail(
                        vec![],
                        format!(
                            "Supply at least one of {}.",
                            fields
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                })
            }
            "requires_array_member" => {
                let (trigger, array) = (field("field")?, field("array")?);
                (row.get(trigger).is_some()
                    && !row[array]
                        .as_array()
                        .is_some_and(|items| items.contains(&args["member"])))
                .then(|| {
                    fail(
                        vec![array.into()],
                        format!("{trigger} requires {} in {array}.", args["member"]),
                    )
                })
            }
            "reject_value_combination" => {
                let values = args["values"].as_array().ok_or_else(|| {
                    internal("reject_value_combination.values must be an array".into())
                })?;
                let matches = values.iter().all(|condition| {
                    let actual = condition["path"].as_array().and_then(|path| {
                        path.iter()
                            .try_fold(row, |value, key| value.get(key.as_str()?))
                    });
                    actual.is_some_and(|value| value == &condition["value"])
                });
                matches.then(|| fail(vec![], "These field values cannot be combined.".into()))
            }
            _ => return Err(internal(format!("Unknown relation opcode: {opcode}"))),
        };
        if let Some(error) = error {
            issues.extend(error.issues);
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relations_apply_only_to_the_matching_rows_and_preserve_row_identity() {
        for (opcode, args, accepted, rejected) in [
            (
                "requires_field",
                json!({"field":"ref","requires":"repo"}),
                json!({"ref":"main","repo":"r"}),
                json!({"ref":"main"}),
            ),
            (
                "requires_any",
                json!({"operation":"search","fields":["owner","keywords"]}),
                json!({"operation":"list"}),
                json!({"operation":"search"}),
            ),
            (
                "requires_array_member",
                json!({"field":"classification","array":"matchIn","member":"content"}),
                json!({"classification":{},"matchIn":["content"]}),
                json!({"classification":{},"matchIn":["path"]}),
            ),
            (
                "reject_value_combination",
                json!({"values":[{"path":["filters","state"],"value":"open"},{"path":["filters","isMerged"],"value":true}]}),
                json!({"filters":{"state":"open"}}),
                json!({"filters":{"state":"open","isMerged":true}}),
            ),
        ] {
            validate(opcode, &args, &json!({"queries":[accepted.clone()]})).unwrap();
            let error =
                validate(opcode, &args, &json!({"queries":[accepted,rejected]})).unwrap_err();
            assert!(
                error
                    .issues
                    .iter()
                    .all(|issue| issue.path.starts_with(&["queries".into(), "1".into()]))
            );
        }
    }
}
