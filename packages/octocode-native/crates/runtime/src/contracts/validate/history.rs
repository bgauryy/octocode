//! Interpreters for the `history_content_selection` and
//! `history_keyword_scope` opcodes.
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_history_content_selection(
    input: &Value,
) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let Some(patches) = query.pointer("/content/patches").and_then(Value::as_object) else {
            continue;
        };
        let selected = patches.get("mode").and_then(Value::as_str) == Some("selected");
        let nonempty = |field: &str| {
            patches
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
        };
        let has_selection = nonempty("files") || nonempty("ranges");
        if selected && !has_selection {
            return Err(issue(
                "history.content-selection",
                vec![
                    "queries".into(),
                    index.to_string(),
                    "content".into(),
                    "patches".into(),
                    "files".into(),
                ],
                "selected patch mode requires non-empty files or ranges",
            ));
        }
        if !selected && has_selection {
            return Err(issue(
                "history.content-selection",
                vec![
                    "queries".into(),
                    index.to_string(),
                    "content".into(),
                    "patches".into(),
                    "mode".into(),
                ],
                "patch files and ranges require selected mode",
            ));
        }
    }
    Ok(())
}

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
        for field in ["path", "branch", "base", "head", "includeDiff"] {
            if query.get(field).is_some_and(|v| v != &Value::Bool(false)) {
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

#[cfg(test)]
mod tests {
    use crate::contracts::validate::validate;
    use serde_json::json;

    #[test]
    fn commit_keywords_reject_path_and_branch_scopes() {
        // Commit-message keywords never silently drop path
        // or branch; the combination is a validation error.
        for field in ["path", "branch"] {
            let mut query = json!({
                "operation":"commit",
                "owner":"octocat",
                "repo":"Hello-World",
                "keywords":["hello"],
                "goal": "test", "reasoning":"Reject a keyword search that would ignore its scope."
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
