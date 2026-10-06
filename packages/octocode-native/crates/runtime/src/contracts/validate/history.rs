//! Interpreters for the `history_keyword_scope` and
//! `history_repository_scope` opcodes.
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_history_keyword_scope(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let has_keywords = query
            .get("keywords")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty());
        if !has_keywords {
            continue;
        }
        if query.get("operation").and_then(Value::as_str) != Some("commit") {
            continue;
        }
        for field in ["path", "ref", "base", "head"] {
            if query.get(field).is_some() {
                return Err(issue(
                    "history.keyword-scope",
                    vec!["queries".into(), index.to_string(), field.into()],
                    format!(
                        "Commit-message keywords cannot be combined with {field}; search covers the default branch. Use history without keywords for path/ref filters and ghGetHistoryItem for diffs."
                    ),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_history_repository_scope(
    input: &Value,
) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        if query.get("repo").is_some() && query.get("owner").and_then(Value::as_str).is_none() {
            return Err(issue(
                "history.repository-scope",
                vec!["queries".into(), index.to_string(), "owner".into()],
                "repo requires owner; omit both to search all GitHub.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::contracts::validate::validate;
    use serde_json::json;

    #[test]
    fn commit_keywords_reject_path_and_ref_scopes() {
        // Commit-message keywords never silently drop path
        // or branch; the combination is a validation error.
        for field in ["path", "ref"] {
            let mut query = json!({
                "operation":"commit",
                "owner":"octocat",
                "repo":"Hello-World",
                "keywords":["hello"],
                "mainGoal": "test", "reasoning":"Reject a keyword search that would ignore its scope."
            });
            query[field] = json!("somewhere");
            let error = validate("ghSearchHistory", json!({"queries":[query]}))
                .expect_err("keywords plus scope is rejected");
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| issue.rule_id == "history.keyword-scope"
                        && issue.path.last().is_some_and(|last| last == field)),
                "{field}: {error:?}"
            );
        }
    }
}
