//! Tool contract embedding: fingerprint and provenance guards, per-tool byte
//! spans, and the `tool_contract.rs` include of the exact bytes read.

use super::tool_gen::{render_listing_roles, render_query_limits, render_tool_ids};
use super::{config_path, integer, invalid, string};
use serde_json::Value;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Lines of the generated `tool_types.rs` header that hold its fingerprint.
const TOOL_TYPES_HEADER_LINES: usize = 32;

/// Byte span of every `tools[]` element inside the embedded contract text, in
/// contract (= `ToolId::ALL`) order. The runtime parses only the tool a call
/// touches from its span instead of the whole multi-megabyte contract, the
/// dominant cost of a fresh CLI process. Every span is verified to parse back
/// to exactly its element, so a scanner mistake fails the build.
fn tool_spans(text: &str, tools: &[Value]) -> Result<Vec<(usize, usize)>, Box<dyn Error>> {
    let bytes = text.as_bytes();
    let (mut depth, mut index) = (0usize, 0usize);
    let (mut key, mut in_tools, mut start) = ("", false, 0usize);
    let mut spans = Vec::new();
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let open = index + 1;
                index = open;
                while bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                if depth == 1 {
                    key = &text[open..index];
                }
            }
            b'{' | b'[' => {
                if in_tools && depth == 2 && bytes[index] == b'{' {
                    start = index;
                }
                if depth == 1 && bytes[index] == b'[' && key == "tools" {
                    in_tools = true;
                }
                depth += 1;
            }
            b'}' | b']' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid("tool contract JSON is unbalanced"))?;
                if in_tools && depth == 2 && bytes[index] == b'}' {
                    spans.push((start, index + 1));
                }
                if in_tools && depth == 1 {
                    in_tools = false;
                }
            }
            _ => {}
        }
        index += 1;
    }
    if spans.len() != tools.len() {
        return Err(invalid(format!(
            "tool contract span scan found {} tools, contract has {}",
            spans.len(),
            tools.len()
        )));
    }
    for (index, ((start, end), tool)) in spans.iter().zip(tools).enumerate() {
        if serde_json::from_str::<Value>(&text[*start..*end])? != *tool {
            return Err(invalid(format!(
                "tool contract span {index} does not parse to tools[{index}]"
            )));
        }
    }
    Ok(spans)
}

/// Embeds the tool contract that `@octocodeai/config` generates (its
/// `generate:tool-contract` script is the only generator). Native keeps no copy:
/// regenerating in config is the whole change, and cargo rebuilds from it.
pub(super) fn embed_tool_contract() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let dir = fs::canonicalize(config_path(&manifest_dir, "contract"))?;
    let file = |name: &str| dir.join(name);
    for name in [
        "tool-contract.json",
        "contract-fixtures.json",
        "provenance.json",
        "tool_types.rs",
        "cli-presentation.json",
    ] {
        println!("cargo:rerun-if-changed={}", file(name).display());
    }
    // Re-serialize compactly: the source is pretty-printed, and every artifact
    // embeds the full contract. Key order is kept (`preserve_order`), so the
    // spans below are computed on the exact bytes that are embedded.
    let contract: Value = serde_json::from_str(&fs::read_to_string(file("tool-contract.json"))?)?;
    let text = serde_json::to_string(&contract)?;
    let spans = tool_spans(
        &text,
        contract
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("tool contract has no tools[]"))?,
    )?;
    let fingerprint = string(
        contract
            .get("fingerprint")
            .ok_or_else(|| invalid("tool contract has no fingerprint"))?,
        "tool contract fingerprint",
    )?;
    // Required field; native reads nothing else from it.
    integer(
        contract
            .get("contractFormatVersion")
            .ok_or_else(|| invalid("tool contract has no contractFormatVersion"))?,
        "contractFormatVersion",
    )?;
    // All artifacts come from one generator run; a mismatch means a partial or
    // hand edit in octocode-config, so fail the build instead of shipping it.
    let provenance_text = fs::read_to_string(file("provenance.json"))?;
    let provenance: Value = serde_json::from_str(&provenance_text)?;
    if provenance
        .get("contractFingerprint")
        .and_then(Value::as_str)
        != Some(fingerprint)
    {
        return Err(invalid(
            "octocode-config contract/provenance.json does not match tool-contract.json; run `yarn workspace @octocodeai/config generate:tool-contract`",
        ));
    }
    // The generator writes the fingerprint in the file's header; only the
    // header is read, never the multi-megabyte type definitions.
    let fingerprint_line =
        format!("pub const TOOL_TYPES_CONTRACT_FINGERPRINT: &str = \"{fingerprint}\";");
    let header = BufReader::new(fs::File::open(file("tool_types.rs"))?)
        .lines()
        .take(TOOL_TYPES_HEADER_LINES)
        .collect::<Result<Vec<_>, _>>()?;
    if !header.contains(&fingerprint_line) {
        return Err(invalid(
            "octocode-config contract/tool_types.rs was generated from a different contract; run `yarn workspace @octocodeai/config generate:tool-contract`",
        ));
    }
    // CLI discovery content (`schema`, `<tool> --help`) is projected from the
    // same core contract; a different fingerprint means a partial regeneration.
    let presentation_text = fs::read_to_string(file("cli-presentation.json"))?;
    let presentation: Value = serde_json::from_str(&presentation_text)?;
    if presentation.get("fingerprint").and_then(Value::as_str) != Some(fingerprint) {
        return Err(invalid(
            "octocode-config contract/cli-presentation.json does not match tool-contract.json; run `yarn workspace @octocodeai/config generate:tool-contract`",
        ));
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out_dir.join("tool_ids.rs"), render_tool_ids(&contract)?)?;
    fs::write(
        out_dir.join("query_limits.rs"),
        render_query_limits(&contract)?,
    )?;
    fs::write(
        out_dir.join("listing_roles.rs"),
        render_listing_roles(&contract)?,
    )?;
    // Embed the exact bytes read above: spans and fingerprint come from this
    // read, so a regeneration racing the build cannot pair them with newer
    // text that `include_str!` of the source file would pick up.
    fs::write(out_dir.join("tool-contract.json"), &text)?;
    fs::write(out_dir.join("provenance.json"), &provenance_text)?;
    fs::write(out_dir.join("cli-presentation.json"), &presentation_text)?;
    fs::copy(
        file("contract-fixtures.json"),
        out_dir.join("contract-fixtures.json"),
    )?;
    let include = |name: &str| {
        format!(
            "include_str!({:?})",
            out_dir.join(name).display().to_string()
        )
    };
    let output = format!(
        "// @generated by crates/runtime/build.rs from packages/octocode-config/contract/.\n\
         pub const CONTRACT_JSON: &str = {};\n\
         /// Byte span of each `tools[]` element in `CONTRACT_JSON`, in `ToolId::ALL` order.\n\
         pub const CONTRACT_TOOL_SPANS: [(usize, usize); {}] = [{}];\n\
         /// CLI discovery content projected from the same contract by octocode-config.\n\
         pub const CLI_PRESENTATION_JSON: &str = {};\n\
         // Read only by tests: contract-ownership and fixture-replay checks.\n\
         #[cfg(test)]\n\
         pub const CONTRACT_FINGERPRINT: &str = {fingerprint:?};\n\
         #[cfg(test)]\n\
         pub const CONTRACT_FIXTURES_JSON: &str = {};\n\
         #[cfg(test)]\n\
         pub const CONTRACT_PROVENANCE_JSON: &str = {};\n",
        include("tool-contract.json"),
        spans.len(),
        spans
            .iter()
            .map(|(start, end)| format!("({start}, {end})"))
            .collect::<Vec<_>>()
            .join(", "),
        include("cli-presentation.json"),
        include("contract-fixtures.json"),
        include("provenance.json"),
    );
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("tool_contract.rs"),
        output,
    )?;
    Ok(())
}
