//! artifactSearch's answers to the shared response stages.
use crate::tools::clasify::{items, resource::ResourceSource};
use crate::tools::id::ToolId;
use crate::tools::output::ToolOutput;
use serde_json::{Value, json};

pub(crate) struct Output;

impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Check packageName, or broaden keywords."
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn clasify_items(&self, source: &ResourceSource, state: &Value) -> Option<Vec<items::Item>> {
        // An exact lookup is already one item; discovery lists packages.
        let ResourceSource::ArtifactSearch(query) = source else {
            return None;
        };
        if query.package_name().is_some() {
            return None;
        }
        let kind = query.artifact_type().as_str();
        let found = items::page_data(state)?
            .get("artifacts")?
            .as_array()?
            .iter()
            .filter_map(|artifact| {
                let kind = artifact.get("type").and_then(Value::as_str).unwrap_or(kind);
                let name = artifact.get("name")?.as_str()?;
                let mut fetch = serde_json::Map::new();
                fetch.insert("type".into(), json!(kind));
                fetch.insert("packageName".into(), json!(name));
                Some(items::Item {
                    state: items::narrowed(
                        state,
                        "/results/0/data/artifacts",
                        vec![artifact.clone()],
                    ),
                    read: Some(items::read(ToolId::ArtifactSearch, fetch)),
                    path: None,
                    item: Some(format!("{kind}:{name}")),
                })
            })
            .collect();
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use crate::tools::id::ToolId;
    use serde_json::json;

    #[test]
    fn registry_rows_are_provider_evidence_with_a_lookup_tip() {
        let output = ToolId::ArtifactSearch.output();
        assert_eq!(output.evidence_kind(&json!({}), &json!({})), "provider");
        assert!(output.fallback_hint(&json!({})).contains("packageName"));
    }
}
