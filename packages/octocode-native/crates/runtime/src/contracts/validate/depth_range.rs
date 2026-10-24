//! Interpreter for the `depth_range` opcode: a row's `minDepth` cannot
//! exceed its `maxDepth` (core's `refineNativeDepthRange`). Which bound is
//! wrong only the caller knows, so the issue carries no fix.
use super::{ContractValidationError, ValidationIssue, issue, query_values};

pub(super) fn validate(input: &serde_json::Value) -> Result<(), ContractValidationError> {
    let issues = query_values(input)
        .filter_map(|(index, query)| {
            let min = query.get("minDepth")?.as_u64()?;
            let max = query.get("maxDepth")?.as_u64()?;
            (min > max).then(|| {
                issue(
                    "structure.depth-range",
                    vec!["queries".into(), index.to_string(), "minDepth".into()],
                    format!(
                        "minDepth must be less than or equal to maxDepth (minDepth {min}, maxDepth {max}): lower minDepth or raise maxDepth."
                    ),
                )
                .issues
            })
        })
        .flatten()
        .collect::<Vec<ValidationIssue>>();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn a_min_depth_past_the_max_depth_is_named_at_min_depth() {
        let rules = json!([{"phase":"validate","opcode":"depth_range"}]);
        let input = json!({"queries":[
            {"path":".","minDepth":2,"maxDepth":2},
            {"path":".","minDepth":3,"maxDepth":1},
            {"path":".","minDepth":3}
        ]});
        let error = super::super::apply_validation_rules(&rules, &input).expect_err("3 > 1");
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, ["queries", "1", "minDepth"]);
        assert!(error.issues[0].message.contains("minDepth 3, maxDepth 1"));
    }
}
