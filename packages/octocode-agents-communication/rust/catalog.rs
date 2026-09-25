use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

pub const SKILL: &str = include_str!("../skills/octocode-agents-communication/SKILL.md");
pub fn skill() -> Value {
    json!({"name":"octocode-agents-communication", "url":"https://github.com/bgauryy/octocode/blob/main/packages/octocode-agents-communication/skills/octocode-agents-communication/SKILL.md", "command":"scripts/agents-communication skill"})
}
pub fn catalog() -> Result<Value> {
    let mut value: Value = serde_json::from_str(include_str!("catalog.json"))?;
    value["database"]["sql"] = json!(crate::database::SQL);
    value["database"]["schemaSha256"] = json!(crate::database::expected()?);
    value["pagination"] = json!({"maxItems":100,"targetBytes":262144,"continuation":"Pass next as after with the same filters. A single oversized row is returned to ensure progress."});
    value["database"]["leasePathComparison"] = json!({"algorithm":"Unicode canonical caseless per component (NFD, full casefold, NFD)","unicodeVersion":caseless::UNICODE_VERSION,"normalizationUnicodeVersion":unicode_normalization::UNICODE_VERSION,"policy":"Case and normalization aliases conflict on every filesystem; access paths and workspace containment stay case-preserving."});
    Ok(value)
}
pub fn validate(schema: &Value, input: &Value) -> Result<()> {
    let validator = jsonschema::validator_for(schema)?;
    if let Err(error) = validator.validate(input) {
        bail!("Invalid input: {error}");
    }
    Ok(())
}
pub fn help() -> Result<Value> {
    let all = catalog()?;
    let commands: Vec<&Value> = all["commands"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid command catalog"))?
        .iter()
        .map(|command| &command["name"])
        .collect();
    Ok(json!({
        "package":"@octocodeai/octocode-agents-communication", "implementation":"Rust", "skill":skill(),
        "usage":"scripts/agents-communication <command> [json] --workspace <path> [--database <file>] [--session <id>]",
        "workflow":["Use the host identity, or join → attach raw/native → listen for presence.", "peers → send_message → hook/native delivery → handle → ack; inbox is recovery.", "lock → edit → renew before expiry → unlock", "Without listen: heartbeat every 15 seconds; leave when finished."],
        "commands":commands,
        "discover":["<command> --help", "schema <command>", "schema entities", "schema entity <name>", "db info", "db protocol", "skill"],
        "run":all["commands"].as_array().and_then(|items| items.iter().find(|c| c["name"] == "run")).map(|c| &c["usage"])
    }))
}
pub fn definition(name: &str) -> Result<Value> {
    catalog()?["commands"]
        .as_array()
        .and_then(|items| items.iter().find(|c| c["name"] == name))
        .cloned()
        .ok_or_else(|| anyhow!("Unknown command: {name}; use --help"))
}
pub fn command(name: &str, input: &Value) -> Result<()> {
    let definition = definition(name)?;
    if let Some(schema) = definition.get("inputSchema") {
        validate(schema, input)?;
    }
    Ok(())
}
pub fn entity(name: &str) -> Result<Value> {
    catalog()?["entities"]
        .as_array()
        .and_then(|items| items.iter().find(|c| c["name"] == name))
        .cloned()
        .ok_or_else(|| anyhow!("Unknown entity: {name}"))
}
pub fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    let value = input[key]
        .as_str()
        .ok_or_else(|| anyhow!("Missing string: {key}"))?;
    let max = match key {
        "body" => 16384,
        "path" => 4096,
        "prompt" => usize::MAX,
        _ => 256,
    };
    if value.trim().is_empty() || value.encode_utf16().count() > max {
        bail!("Invalid {key}");
    }
    Ok(value)
}
pub fn ttl(input: &Value, default: i64) -> Result<i64> {
    let value = match input.get("ttlMs") {
        Some(v) => v.as_i64().ok_or_else(|| anyhow!("Invalid TTL"))?,
        None => default,
    };
    if !(1000..=86_400_000).contains(&value) {
        bail!("Invalid TTL");
    }
    Ok(value)
}
