//! What a test needs to check that a call one package tool leads to in the
//! other runs there as sent.
use octocode_contract_types::ArtifactType;
use serde_json::Value;

/// Every ecosystem artifactSearch takes, as its published query schema lists it.
pub(crate) fn artifact_ecosystems() -> Vec<ArtifactType> {
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
    let tool = presentation["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "artifactSearch")
        .expect("artifactSearch");
    tool["views"]["query"]["querySchema"]["properties"]["ecosystem"]["enum"]
        .as_array()
        .expect("ecosystem enum")
        .iter()
        .map(|name| serde_json::from_value(name.clone()).expect("an artifact type"))
        .collect()
}

/// `input` runs as sent on `tool`: no repair note, nothing rewritten, valid.
pub(crate) fn runs_as_sent(tool: &str, input: &Value) {
    let (normalized, notes) =
        octocode_runtime_base::contracts::normalized_input(tool, input.clone());
    assert!(notes.is_empty(), "{tool} repairs {input}: {notes:?}");
    assert_eq!(&normalized, input, "{tool} rewrites the call");
    octocode_runtime_base::contracts::validate(tool, input.clone())
        .unwrap_or_else(|error| panic!("{tool} rejects {input}: {error}"));
}
