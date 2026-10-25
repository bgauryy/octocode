use super::super::{internal, issue};
use super::{ContractValidationError, validate_read_only};
use serde_json::Value;

fn matches(schema: &Value, value: &mut Value) -> Result<bool, ContractValidationError> {
    super::probe(validate_read_only(schema, schema, value, &mut Vec::new()))
}

pub(super) fn validate(
    schema: &Value,
    value: &mut Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let Some(checks) = schema.get("x-checks") else {
        return Ok(());
    };
    let checks = checks
        .as_array()
        .ok_or_else(|| internal("x-checks must be an array".into()))?;
    let mut issues = Vec::new();
    for check in checks {
        let assertion = check
            .get("assert")
            .ok_or_else(|| internal("x-checks requires assert".into()))?;
        let id = check["id"]
            .as_str()
            .ok_or_else(|| internal("x-checks requires id".into()))?;
        let message = check["message"]
            .as_str()
            .ok_or_else(|| internal("x-checks requires message".into()))?;
        if matches(assertion, value)? {
            continue;
        }
        let mut apply =
            |candidate: &mut Value, at: Vec<String>| -> Result<(), ContractValidationError> {
                if let Some(when) = check.get("when")
                    && !matches(when, candidate)?
                {
                    return Ok(());
                }
                let mut at = at;
                if let Some(parts) = check["path"].as_array() {
                    at.extend(parts.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                issues.extend(issue(id, at, message).issues);
                Ok(())
            };
        if let Some(each) = check.get("each") {
            let each = each
                .as_str()
                .ok_or_else(|| internal("x-checks each must be a field name".into()))?;
            if let Some(entries) = value.get_mut(each).and_then(Value::as_object_mut) {
                for (key, candidate) in entries {
                    let mut at = path.to_vec();
                    at.extend([each.to_owned(), key.clone()]);
                    apply(candidate, at)?;
                }
            }
        } else {
            apply(value, path.to_vec())?;
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

#[cfg(test)]
mod tests;
