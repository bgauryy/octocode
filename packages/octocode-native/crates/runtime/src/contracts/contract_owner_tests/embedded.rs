use super::*;

/// The flags and help parts the build cuts from the CLI presentation
/// are exactly its tools' `cli`, `selection.cli` and `help`.
#[test]
fn cli_tool_parts_match_the_presentation() {
    let presentation = super::super::cli_presentation().expect("presentation");
    let tools = presentation["tools"].as_array().expect("tools");
    let flags = super::super::cli_tool_flags().expect("flags");
    assert_eq!(flags.as_array().map(Vec::len), Some(tools.len()));
    for (tool, entry) in tools.iter().zip(flags.as_array().into_iter().flatten()) {
        let name = tool["name"].as_str().expect("name");
        assert_eq!(entry["name"], name);
        assert_eq!(entry["cli"], tool["cli"], "{name} cli");
        assert_eq!(
            entry["selection"]["cli"], tool["selection"]["cli"],
            "{name} selection.cli"
        );
        assert_eq!(
            super::super::cli_tool_help(name),
            tool["help"].as_str(),
            "{name} help"
        );
    }
    assert_eq!(super::super::cli_tool_help("nope"), None);
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
        for forbidden in ["title", "description", "examples", "annotations"] {
            assert!(!object.contains_key(forbidden), "{name}.{forbidden}");
        }
        assert!(
            object.contains_key("shortDescription")
                && object.contains_key("querySchema")
                && object.contains_key("inputSchema")
                && object.contains_key("outputSchema")
                && object.contains_key("rules"),
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

#[test]
fn public_response_and_tree_limits_are_pinned_against_silent_drift() {
    // Provenance records the core revision and generated fingerprint, but
    // both move with regeneration. It does not pin intended numeric bounds:
    // a core change that relaxes a public limit passes provenance checks.
    // This pins the two limits that also surface in the live MCP schema so any
    // change is a visible, reviewed test diff.
    fn collect(value: &serde_json::Value, response: &mut Vec<u64>, tree_depth: &mut Vec<u64>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(max) = map
                    .get("responseLength")
                    .and_then(|schema| schema.get("maximum"))
                    .and_then(serde_json::Value::as_u64)
                {
                    response.push(max);
                }
                if let Some(max) = map
                    .get("maxDepth")
                    .and_then(|schema| schema.get("maximum"))
                    .and_then(serde_json::Value::as_u64)
                {
                    tree_depth.push(max);
                }
                for child in map.values() {
                    collect(child, response, tree_depth);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    collect(child, response, tree_depth);
                }
            }
            _ => {}
        }
    }

    let contract: serde_json::Value = serde_json::from_str(contract_json()).expect("contract JSON");
    let mut response_maxima = Vec::new();
    let mut tree_depth_maxima = Vec::new();
    collect(&contract, &mut response_maxima, &mut tree_depth_maxima);

    assert!(
        !response_maxima.is_empty(),
        "expected at least one responseLength bound in the contract"
    );
    assert!(
        response_maxima.iter().all(|max| *max == 50_000),
        "responseLength.maximum drifted from 50000: {response_maxima:?}"
    );
    assert!(
        !tree_depth_maxima.is_empty(),
        "expected the tree-recursion maxDepth bound in the contract"
    );
    assert!(
        tree_depth_maxima.iter().all(|max| *max == 20),
        "public maxDepth bounds drifted from 20: {tree_depth_maxima:?}"
    );
}

/// Schema single-source guard: assert that no `.rs` source file defines
/// inline JSON Schema vocabulary (`"$schema"`, `"inputSchema"` as an object
/// key inside a `json!()` macro call, or `"properties"` as an *lvalue* in a
/// JSON literal). Schemas are embedded from `@octocodeai/config`'s
/// generated `contract/`; this scan catches accidental copy-paste of schema
/// fragments into tool runners.
///
/// Patterns checked (as substrings in non-comment, non-test lines):
///   - `json!({"$schema":` — top-level JSON Schema declaration
///   - `json!({"inputSchema":` — MCP tool registration schema inline
///   - `"inputSchema": {` — same, written as a field expression
#[test]
fn no_inline_schema_literals_outside_generated_contracts() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    // Patterns that indicate inline (hand-authored) JSON Schema definition.
    // We do not check for "properties" broadly because it appears in
    // schema-reading code (e.g. contracts/validate.rs). Instead we only
    // flag the two patterns that would only ever appear as schema authors:
    let forbidden: &[&str] = &[
        "json!({\"$schema\":",
        "json!({\"inputSchema\":",
        "\"inputSchema\": {",
    ];

    let mut violations: Vec<String> = Vec::new();
    scan_for_schema_literals(&src_dir, forbidden, &mut violations);

    assert!(
        violations.is_empty(),
        "Hand-authored JSON Schema literals found in native sources \
             — move them to octocode-core and run `yarn contracts:regen`:\n{}",
        violations.join("\n")
    );
}

fn scan_for_schema_literals(
    dir: &std::path::Path,
    patterns: &[&str],
    violations: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_for_schema_literals(&path, patterns, violations);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                // Skip comment lines.
                if trimmed.starts_with("//") || trimmed.starts_with('*') {
                    continue;
                }
                for pattern in patterns {
                    if trimmed.contains(pattern) {
                        violations.push(format!(
                            "{}:{}: suspicious inline schema pattern `{}`",
                            path.display(),
                            line_no + 1,
                            pattern
                        ));
                    }
                }
            }
        }
    }
}
