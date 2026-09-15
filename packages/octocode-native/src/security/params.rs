use std::collections::HashSet;

use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResearchFields {
    pub goal: Option<String>,
    pub reasoning: Option<String>,
}

pub fn extract_research_fields(params: &Map<String, Value>) -> ResearchFields {
    let objects = query_objects(params);
    ResearchFields {
        goal: collect_strings(&objects, "goal"),
        reasoning: collect_strings(&objects, "reasoning"),
    }
}

pub fn extract_repo_owner_from_params(params: &Map<String, Value>) -> Vec<String> {
    let objects = query_objects(params);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for object in objects {
        let repository = string_field(object, "repository").filter(|value| value.contains('/'));
        let owner = string_field(object, "owner");
        let repo = string_field(object, "repo");
        let value = repository
            .map(str::to_owned)
            .or_else(|| {
                owner
                    .zip(repo)
                    .map(|(owner, repo)| format!("{owner}/{repo}"))
            })
            .or_else(|| owner.map(str::to_owned));
        if let Some(value) = value
            && seen.insert(value.clone())
        {
            result.push(value);
        }
    }
    result
}

fn query_objects(params: &Map<String, Value>) -> Vec<&Map<String, Value>> {
    match params.get("queries") {
        Some(Value::Array(queries)) if !queries.is_empty() => {
            queries.iter().filter_map(Value::as_object).collect()
        }
        _ => vec![params],
    }
}

fn collect_strings(objects: &[&Map<String, Value>], key: &str) -> Option<String> {
    let mut seen = HashSet::new();
    let values = objects
        .iter()
        .filter_map(|object| string_field(object, key))
        .filter(|value| seen.insert((*value).to_owned()))
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join("; "))
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_frozen_bulk_research_and_repo_fields() {
        let value = serde_json::json!({"queries": [
            {"goal":"sub 1", "reasoning":"r1", "repository":"facebook/react"},
            {"goal":"sub 2", "reasoning":"r1", "owner":"facebook", "repo":"react"},
            {"owner":"vercel", "repo":"next.js"}
        ]});
        let object = value.as_object().expect("fixture object");
        assert_eq!(
            extract_research_fields(object),
            ResearchFields {
                goal: Some("sub 1; sub 2".to_owned()),
                reasoning: Some("r1".to_owned()),
            }
        );
        assert_eq!(
            extract_repo_owner_from_params(object),
            ["facebook/react", "vercel/next.js"]
        );
    }

    #[test]
    fn empty_queries_fall_back_to_flat_fields() {
        let value = serde_json::json!({"queries": [], "goal":"flat", "owner":"facebook"});
        let object = value.as_object().expect("fixture object");
        assert_eq!(
            extract_research_fields(object).goal.as_deref(),
            Some("flat")
        );
        assert_eq!(extract_repo_owner_from_params(object), ["facebook"]);
    }
}
