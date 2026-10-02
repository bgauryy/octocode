//! Interpreter for the `github_code_search_runnable` and
//! `github_repo_search_runnable` opcodes.
use super::{ContractValidationError, issue};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum GithubSearchKind {
    Code,
    Repositories,
}

pub(super) fn validate_github_search_queries(
    input: &Value,
    kind: GithubSearchKind,
) -> Result<(), ContractValidationError> {
    let Some(queries) = input["queries"].as_array() else {
        return Ok(());
    };
    for (index, query) in queries.iter().enumerate() {
        let has_text = |field: &str| {
            query
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        };
        let has_terms = |field: &str| {
            query
                .get(field)
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
                })
        };
        let (runnable, message) = match kind {
            GithubSearchKind::Code => (
                has_terms("keywords")
                    || ["path", "extension", "filename", "language"]
                        .iter()
                        .any(|field| has_text(field)),
                "ghSearchCode needs keywords or a path, extension, filename, or language filter",
            ),
            GithubSearchKind::Repositories => (
                has_terms("keywords")
                    || has_terms("topics")
                    || [
                        "owner",
                        "language",
                        "stars",
                        "forks",
                        "goodFirstIssues",
                        "updated",
                        "created",
                        "size",
                        "visibility",
                        "license",
                        "qualifiers",
                    ]
                    .iter()
                    .any(|field| has_text(field))
                    || query.get("archived").is_some_and(Value::is_boolean),
                "ghSearchRepo needs keywords, topics, owner, or a filter",
            ),
        };
        if !runnable {
            return Err(issue(
                "gh-search.runnable-constraint",
                vec!["queries".into(), index.to_string(), "keywords".into()],
                message.to_owned(),
            ));
        }
        // Code search must be scoped to an owner: the public contract states code
        // "cannot wildcard repositories", so an unscoped code query (which the
        // provider would run across all of GitHub) is rejected here rather than
        // silently returning global noise.
        if matches!(kind, GithubSearchKind::Code) && !has_text("owner") {
            return Err(issue(
                "gh-search.code-scope",
                vec!["queries".into(), index.to_string(), "owner".into()],
                "code search requires an owner (optionally with repo); it cannot wildcard across all of GitHub".to_owned(),
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
    fn rejects_repo_scoped_code_wildcards_before_provider_io() {
        let error = validate(
            "ghSearchCode",
            json!({"queries":[{
                "owner":"octocode",
                "repo":"octocode",
                "goal": "test", "reasoning":"Reject a repo-wide wildcard."
            }]}),
        )
        .expect_err("owner/repo alone is not a runnable code search");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.rule_id == "gh-search.runnable-constraint")
        );

        validate(
            "ghSearchCode",
            json!({"queries":[{
                "owner":"octocode",
                "repo":"octocode",
                "path":"src",
                "goal": "test", "reasoning":"Run a path-bounded code search."
            }]}),
        )
        .expect("path is an explicit code-search narrowing filter");
    }

    #[test]
    fn rejects_unscoped_code_search_that_would_wildcard_all_of_github() {
        let error = validate(
            "ghSearchCode",
            json!({"queries":[{
                "keywords":["isEmptyArray"],
                "goal": "test", "reasoning":"A keyword-only code search must not run globally."
            }]}),
        )
        .expect_err("code search without an owner is a global wildcard");
        // The schema requires owner; the code-scope rule backs it up.
        assert!(error.issues.iter().any(|issue| {
            issue.path.last().is_some_and(|field| field == "owner")
                && matches!(
                    issue.rule_id.as_str(),
                    "schema.required" | "gh-search.code-scope"
                )
        }));
        // owner alone (no repo) is a legitimate org-wide code search.
        validate(
            "ghSearchCode",
            json!({"queries":[{
                "owner":"sindresorhus",
                "keywords":["isEmptyArray"],
                "goal": "test", "reasoning":"Owner-scoped code search is allowed."
            }]}),
        )
        .expect("owner-scoped code search is runnable");
    }
}
