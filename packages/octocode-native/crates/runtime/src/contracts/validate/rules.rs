use super::{
    ContractValidationError, artifact, clasify, depth_range, internal, issue, lsp, relations,
    structural_rule,
};
use serde_json::Value;

pub(super) fn apply_validation_rules(
    rules: &Value,
    input: &Value,
) -> Result<(), ContractValidationError> {
    let rules = rules
        .as_array()
        .ok_or_else(|| internal("rules must be an array".into()))?;
    let mut issues = Vec::new();
    for rule in rules.iter().filter(|rule| rule["phase"] == "validate") {
        let result = match rule["opcode"].as_str() {
            Some(
                opcode @ ("requires_field"
                | "requires_any"
                | "requires_array_member"
                | "reject_value_combination"),
            ) => relations::validate(opcode, &rule["args"], input),
            Some("artifact_registry_url") => artifact::validate_artifact_registry_url(input),
            Some("lsp_rust_context") => lsp::validate_lsp_queries(input),
            Some("depth_range") => depth_range::validate(input),
            Some("structural_rule") => structural_rule::validate(input),
            Some("clasify_box_image") => clasify::validate_box_image(input),
            Some(opcode) => {
                return Err(issue(
                    "contract.unsupported-validator",
                    vec![],
                    format!("Unsupported validation opcode: {opcode}"),
                ));
            }
            None => return Err(internal("validation rule opcode must be a string".into())),
        };
        if let Err(error) = result {
            issues.extend(error.issues);
        }
    }
    issues.dedup();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

pub(super) fn query_values(input: &Value) -> impl Iterator<Item = (usize, &Value)> {
    input["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
}
