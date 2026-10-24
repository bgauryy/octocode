use super::schema::{validate_read_only, validate_schema};
use super::{ContractValidationError, contract_tool};
use serde_json::Value;

/// Validates a completed structured response, exactly as a client receives
/// it, against the canonical generated output contract. Validation uses a
/// clone because schema defaults, if ever introduced by the contract owner,
/// must not mutate an already produced result.
pub fn validate_output(tool_name: &str, output: &Value) -> Result<(), ContractValidationError> {
    let (_, tool) = contract_tool(tool_name)?;
    let schema = &tool["outputSchema"];
    let mut candidate = output.clone();
    if declares_defaults(tool_name, schema) {
        validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
    } else {
        validate_read_only(schema, schema, &mut candidate, &mut Vec::new())?;
    }
    #[cfg(test)]
    undeclared_check(schema, &candidate)?;
    Ok(())
}

/// Tests also reject a key the core output schema never declares: the
/// published schemas stay open, so plain validation would accept it.
#[cfg(test)]
fn undeclared_check(schema: &Value, output: &Value) -> Result<(), ContractValidationError> {
    let paths = super::super::undeclared::undeclared_paths(schema, output);
    if paths.is_empty() {
        return Ok(());
    }
    Err(ContractValidationError {
        issues: paths
            .into_iter()
            .map(|path| super::ValidationIssue {
                rule_id: "undeclaredOutputField".into(),
                path: path
                    .trim_start_matches('.')
                    .split('.')
                    .map(|segment| segment.trim_end_matches("[]").to_owned())
                    .collect(),
                message: format!("{path} is not declared by the core output schema"),
                schema: None,
                received: None,
            })
            .collect(),
    })
}

/// [`validate_output`] without the copy when validation cannot change the
/// output: the tool's output schema declares no defaults to insert, so the
/// read-only mode writes nothing (`not` probes and union branches check the
/// response itself) and it stays byte-identical, walked once.
pub(crate) fn validate_output_in_place(
    tool_name: &str,
    output: &mut Value,
) -> Result<(), ContractValidationError> {
    let (_, tool) = contract_tool(tool_name)?;
    let schema = &tool["outputSchema"];
    if declares_defaults(tool_name, schema) {
        return validate_output(tool_name, output);
    }
    validate_read_only(schema, schema, output, &mut Vec::new())?;
    #[cfg(test)]
    undeclared_check(schema, output)?;
    Ok(())
}

/// Whether a tool's output schema has a `default` keyword anywhere (cached:
/// the embedded contract never changes).
fn declares_defaults(tool_name: &str, schema: &Value) -> bool {
    fn any_default(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.contains_key("default") || map.values().any(any_default),
            Value::Array(items) => items.iter().any(any_default),
            _ => false,
        }
    }
    static DECLARED: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, bool>>,
    > = std::sync::OnceLock::new();
    let mut declared = DECLARED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&known) = declared.get(tool_name) {
        return known;
    }
    let found = any_default(schema);
    declared.insert(tool_name.to_owned(), found);
    found
}
