use super::*;

#[test]
fn cli_help_is_the_embedded_core_presentation() {
    let presentation = super::super::cli_presentation().expect("presentation");
    for tool in presentation["tools"].as_array().expect("tools") {
        assert_eq!(
            super::super::cli_tool_help(tool["name"].as_str().unwrap()),
            tool["help"].as_str()
        );
    }
}

/// Every classification notice native renders is authored in the
/// embedded contract.
#[test]
fn every_classification_notice_comes_from_the_contract() {
    for key in ["failedWithRead", "failed", "imageInspection"] {
        assert!(
            !super::super::classification_notice(key).is_empty(),
            "classificationNotices.{key}"
        );
    }
}

#[test]
fn syntax_operation_is_rejected_without_core_alias() {
    assert!(
        prepare_and_validate(
            "astSearch",
            json!({"operation":"syntax","path":"/tmp/lib.rs"}),
        )
        .is_err()
    );
}

#[test]
fn each_tool_span_parses_to_its_contract_entry() {
    // `tool_contract` parses one tool from its span when the full contract
    // is not yet parsed; every span must yield exactly `tools[i]`.
    let contract = super::super::parsed_contract().expect("generated contract");
    let tools = contract["tools"].as_array().expect("tool array");
    assert_eq!(
        super::super::generated::CONTRACT_TOOL_SPANS.len(),
        tools.len()
    );
    for ((start, end), (tool, id)) in super::super::generated::CONTRACT_TOOL_SPANS
        .iter()
        .zip(tools.iter().zip(super::super::ToolId::ALL))
    {
        let parsed: serde_json::Value =
            serde_json::from_str(&contract_json()[*start..*end]).expect("span JSON");
        assert_eq!(&parsed, tool, "{id}");
        assert_eq!(
            super::super::tool_contract(id).expect("tool contract"),
            tool
        );
        assert_eq!(
            super::super::tool_contract_named(id.as_str())
                .expect("known tool")
                .expect("tool contract"),
            tool
        );
    }
    assert!(super::super::tool_contract_named("nope").is_none());
}

#[test]
fn embedded_contract_is_compact_json() {
    // Every artifact embeds the contract, so the build re-serializes it
    // compactly (the source is pretty-printed at 3.4x the size).
    let text = contract_json();
    assert!(!text.contains('\n'), "embedded contract is pretty-printed");
    // Canonical Zod constraints and per-candidate failure receipts: 2,572,176 bytes.
    assert!(
        text.len() < 2_573_000,
        "embedded contract is {} bytes",
        text.len()
    );
}

#[test]
fn generated_contract_is_enforcement_only() {
    // Presentation and MCP instructions are core-delivered by the JS
    // layers; the embed carries machine-enforced surfaces exclusively.
    let contract = super::super::parsed_contract().expect("generated contract");
    let top = contract.as_object().expect("contract object");
    for forbidden in ["mcpInstructions", "mcpInstructionTable"] {
        assert!(!top.contains_key(forbidden), "top-level {forbidden}");
    }
    for tool in contract["tools"].as_array().expect("tool array") {
        let name = tool["name"].as_str().expect("tool name");
        let object = tool.as_object().expect("tool object");
        for forbidden in ["description", "examples", "annotations"] {
            assert!(!object.contains_key(forbidden), "{name}.{forbidden}");
        }
        assert!(
            object.contains_key("title")
                && object.contains_key("querySchema")
                && object.contains_key("inputSchema")
                && object.contains_key("outputSchema")
                && !object.contains_key("rules"),
            "{name} enforcement fields"
        );
    }
}

#[test]
fn embedded_contract_is_the_unmodified_config_output() {
    // octocode-config records the digest of the contract it generated; a
    // hand edit to the embedded bytes breaks the match. No native-side pin
    // needs updating on regeneration.
    // The embed is compact, so the digest is taken over the generated source
    // bytes; the embedded text must be the same JSON document.
    use sha2::{Digest, Sha256};
    const SOURCE: &str =
        include_str!("../../../../../../octocode-config/contract/tool-contract.json");
    let provenance: serde_json::Value =
        serde_json::from_str(contract_provenance_json()).expect("provenance JSON");
    assert_eq!(provenance["sourcePackage"], "@octocodeai/octocode-core");
    assert_eq!(
        provenance["contractFingerprint"],
        super::super::contract_fingerprint()
    );
    assert_eq!(
        provenance["contractSha256"],
        hex::encode(Sha256::digest(SOURCE.as_bytes())),
        "generated source differs from what octocode-config recorded"
    );
    let source: serde_json::Value = serde_json::from_str(SOURCE).expect("source JSON");
    let embedded: serde_json::Value = serde_json::from_str(contract_json()).expect("embedded JSON");
    assert_eq!(
        embedded, source,
        "embedded contract differs from its source"
    );
}
