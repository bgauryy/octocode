//! Audit the embedded schema once, including branches input validation can skip.
use super::super::{ContractValidationError, internal};
use super::{compiled_pattern, known_type, relationships::strings};
use serde_json::Value;

pub(in crate::contracts::validate) fn validate(
    root: &Value,
) -> Result<(), ContractValidationError> {
    visit(root, root)
}

fn require(valid: bool, message: &str) -> Result<(), ContractValidationError> {
    if valid {
        Ok(())
    } else {
        Err(internal(message.into()))
    }
}

fn visit(root: &Value, schema: &Value) -> Result<(), ContractValidationError> {
    if schema.is_boolean() {
        return Ok(());
    }
    let object = schema
        .as_object()
        .ok_or_else(|| internal("Schema must be an object or boolean".into()))?;
    for (key, value) in object {
        match key.as_str() {
            "$schema" | "title" | "description" | "format" => {
                require(value.is_string(), "Schema annotation must be a string")?;
            }
            "$ref" => require(
                value
                    .as_str()
                    .filter(|reference| reference.starts_with("#/$defs/"))
                    .and_then(|reference| root.pointer(&reference[1..]))
                    .is_some(),
                "Unresolved schema reference",
            )?,
            "type" => require(
                value.as_str().is_some_and(known_type)
                    || value.as_array().is_some_and(|types| {
                        !types.is_empty()
                            && types
                                .iter()
                                .all(|kind| kind.as_str().is_some_and(known_type))
                    }),
                "Unsupported schema type",
            )?,
            "properties" | "$defs" => {
                for child in value
                    .as_object()
                    .ok_or_else(|| internal("Schema map must be an object".into()))?
                    .values()
                {
                    visit(root, child)?;
                }
            }
            "items" | "propertyNames" | "additionalProperties" | "not" => visit(root, value)?,
            "contains" => visit(super::predicate_root(root, value), value)?,
            "allOf" | "anyOf" | "oneOf" => {
                let branches = value
                    .as_array()
                    .ok_or_else(|| internal("Schema composition must be an array".into()))?;
                require(!branches.is_empty(), "Schema composition must not be empty")?;
                for child in branches {
                    visit(root, child)?;
                }
            }
            "required" => {
                strings(value)?;
            }
            "enum" => require(
                value.as_array().is_some_and(|values| !values.is_empty()),
                "enum must be a nonempty array",
            )?,
            "pattern" => {
                compiled_pattern(
                    value
                        .as_str()
                        .ok_or_else(|| internal("pattern must be a string".into()))?,
                )
                .map_err(internal)?;
            }
            "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
            | "maxProperties" => require(
                value.as_u64().is_some(),
                "Schema size must be a nonnegative integer",
            )?,
            "minimum" | "maximum" => require(value.is_number(), "Schema bound must be numeric")?,
            "x-checks" => checks(value)?,
            "x-order" => order(value)?,
            "x-sum" => {
                diagnostic(value)?;
                require(
                    value["total"].is_string() && !strings(&value["terms"])?.is_empty(),
                    "x-sum requires total and terms",
                )?;
            }
            "x-numericTokens" => {
                diagnostic(value)?;
                for field in ["ignore", "pattern"] {
                    compiled_pattern(
                        value[field]
                            .as_str()
                            .ok_or_else(|| internal("x-numericTokens requires patterns".into()))?,
                    )
                    .map_err(internal)?;
                }
                require(
                    value["minimum"].is_number() && value["maximum"].is_number(),
                    "x-numericTokens requires numeric bounds",
                )?;
            }
            "x-tree" => {
                diagnostic(value)?;
                require(
                    value["maxDepth"].as_u64().is_some(),
                    "x-tree requires maxDepth",
                )?;
                for path in value["children"]
                    .as_array()
                    .ok_or_else(|| internal("x-tree requires child paths".into()))?
                {
                    require(
                        !strings(path)?.is_empty(),
                        "x-tree child path must not be empty",
                    )?;
                }
            }
            "x-url" => {
                diagnostic(value)?;
                if let Some(protocols) = value.get("protocols") {
                    strings(protocols)?;
                }
                for field in ["credentials", "query", "fragment"] {
                    if let Some(flag) = value.get(field) {
                        require(*flag == false, "x-url component constraints must be false")?;
                    }
                }
            }
            "const" | "default" | "x-versionSupport" | "x-requiredMessage" | "x-listing"
            | "x-cursor" => {}
            _ => return Err(internal(format!("Unsupported schema keyword: {key}"))),
        }
    }
    Ok(())
}

fn diagnostic(annotation: &Value) -> Result<(), ContractValidationError> {
    require(
        annotation["id"].is_string() && annotation["message"].is_string(),
        "Constraint requires id and message",
    )?;
    if let Some(path) = annotation.get("path") {
        strings(path)?;
    }
    Ok(())
}

fn checks(value: &Value) -> Result<(), ContractValidationError> {
    for check in value
        .as_array()
        .ok_or_else(|| internal("x-checks must be an array".into()))?
    {
        diagnostic(check)?;
        let assertion = check
            .get("assert")
            .ok_or_else(|| internal("x-checks requires assert".into()))?;
        visit(assertion, assertion)?;
        if let Some(condition) = check.get("when") {
            visit(condition, condition)?;
        }
        if let Some(each) = check.get("each") {
            require(each.is_string(), "x-checks each must be a field name")?;
        }
    }

    Ok(())
}

fn order(value: &Value) -> Result<(), ContractValidationError> {
    diagnostic(value)?;
    require(
        value["lower"].is_string() && value["upper"].is_string(),
        "x-order requires lower and upper",
    )?;
    match value["kind"].as_str() {
        Some("number" | "timestamp") => {}
        Some("lexicographic") => require(
            !strings(&value["keys"])?.is_empty(),
            "x-order requires comparison keys",
        )?,
        _ => return Err(internal("Unsupported x-order kind".into())),
    }
    if let Some(exclusive) = value.get("exclusive") {
        require(exclusive.is_boolean(), "x-order exclusive must be boolean")?;
    }

    Ok(())
}
