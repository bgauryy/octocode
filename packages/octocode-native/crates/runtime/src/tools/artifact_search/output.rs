//! artifactSearch's answers to the shared response stages.
use crate::response::output::ToolOutput;
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        if crate::response::output::has_value(query, "packageName") {
            "Check the exact packageName spelling and ecosystem, or search by keywords."
        } else {
            "Use fewer or broader keywords, an exact packageName, or another ecosystem."
        }
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn settle_repaired_row(&self, row: &mut Value) {
        super::settle_repaired_row(row);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn registry_rows_are_provider_evidence_with_a_lookup_tip() {
        let output = crate::id::ToolId::ArtifactSearch.output();
        assert_eq!(output.evidence_kind(&json!({}), &json!({})), "provider");
        assert!(output.fallback_hint(&json!({})).contains("packageName"));
    }
}
