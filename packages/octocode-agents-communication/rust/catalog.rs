use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::OnceLock};

pub const SKILL: &str = include_str!("../skills/octocode-agents-communication/SKILL.md");
/// Select only host-specific setup paragraphs; all shared workflow remains canonical.
/// The full skill remains the default for installation and unknown-host discovery.
pub fn skill_instructions(vendor: Option<&str>) -> String {
    let Some(vendor) = vendor else {
        return SKILL.to_owned();
    };
    let host_lines = [
        ("**Claude:**", &["claude"][..]),
        ("**Codex:**", &["codex"][..]),
        ("**Grok:**", &["grok"][..]),
        ("**OpenCode:**", &["opencode"][..]),
        ("**Pi:**", &["pi"][..]),
        ("**Cursor/Grok hooks:**", &["cursor", "grok"][..]),
    ];
    SKILL
        .lines()
        .filter(|line| {
            host_lines
                .iter()
                .all(|(prefix, hosts)| !line.starts_with(prefix) || hosts.contains(&vendor))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
pub fn skill() -> Value {
    json!({"name":"octocode-agents-communication", "url":"https://github.com/bgauryy/octocode/blob/main/packages/octocode-agents-communication/skills/octocode-agents-communication/SKILL.md", "command":"scripts/agents-communication skill"})
}
pub fn catalog() -> Result<Value> {
    Ok(cached_catalog()?.clone())
}
/// Keep one immutable discovery surface for the lifetime of each worker/server.
pub fn selected_tools(selection: Option<&str>) -> Result<Vec<Value>> {
    let tools = cached_catalog()?["tools"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid tool catalog"))?;
    let Some(selection) = selection else {
        return Ok(tools.clone());
    };
    let mut names = std::collections::HashSet::new();
    for name in selection.split(',') {
        if !tools.iter().any(|tool| tool["name"] == name) {
            bail!("Unknown selected tool: {name}; use schema to inspect tools");
        }
        if !names.insert(name) {
            bail!("Duplicate selected tool: {name}");
        }
    }
    Ok(tools
        .iter()
        .filter(|tool| names.contains(tool["name"].as_str().unwrap_or("")))
        .cloned()
        .collect())
}
fn cached_catalog() -> Result<&'static Value> {
    static CATALOG: OnceLock<Result<Value, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| build_catalog().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))
}
fn build_catalog() -> Result<Value> {
    let mut value: Value = serde_json::from_str(include_str!("catalog.json"))?;
    let commands = value["commands"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid command catalog"))?
        .clone();
    for tool in value["tools"]
        .as_array_mut()
        .ok_or_else(|| anyhow!("Invalid tool catalog"))?
    {
        let command = commands
            .iter()
            .find(|command| command["name"] == tool["name"])
            .ok_or_else(|| anyhow!("Tool has no command definition: {}", tool["name"]))?;
        tool["description"] = command["description"].clone();
        tool["inputSchema"] = command["inputSchema"].clone();
    }
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
    let all = cached_catalog()?;
    let commands: Vec<&Value> = all["commands"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid command catalog"))?
        .iter()
        .map(|command| &command["name"])
        .collect();
    Ok(json!({
        "package":"@octocodeai/octocode-agents-communication", "implementation":"Rust", "skill":skill(),
        "usage":"scripts/agents-communication <command> [json] --workspace <path> [--database <file>] [--session <id>]",
        "workflow":["Use supplied identity/tools; otherwise join → attach → listen, or heartbeat every 15s; leave when done.", "peers → coordinate with reasoning → handle new deliveries once → ack; inbox is recovery.", "lock/lock_many with reasoning → edit only on ok:true → renew while progressing → unlock.", "On conflict release held leases, ask the owner once, then retry after handoff/expiry.", "Share large context with share_document; read only needed byte pages."],
        "commands":commands,
        "discover":["<command> --help", "schema <command>", "schema entities", "schema entity <name>", "db info", "db protocol", "skill"],
        "run":all["commands"].as_array().and_then(|items| items.iter().find(|c| c["name"] == "run")).map(|c| &c["usage"])
    }))
}
pub fn definition(name: &str) -> Result<Value> {
    cached_catalog()?["commands"]
        .as_array()
        .and_then(|items| items.iter().find(|c| c["name"] == name))
        .cloned()
        .ok_or_else(|| anyhow!("Unknown command: {name}; use --help"))
}
pub fn command(name: &str, input: &Value) -> Result<()> {
    // Lazily compile only used commands. The bounded key set is embedded, never caller supplied.
    type CommandValidator = OnceLock<Result<Option<jsonschema::Validator>, String>>;
    static VALIDATORS: OnceLock<Result<HashMap<String, CommandValidator>, String>> =
        OnceLock::new();
    let validators = VALIDATORS
        .get_or_init(|| {
            let build = || -> Result<HashMap<String, CommandValidator>> {
                cached_catalog()?["commands"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Invalid commands"))?
                    .iter()
                    .map(|command| {
                        let name = command["name"]
                            .as_str()
                            .ok_or_else(|| anyhow!("Invalid command name"))?;
                        Ok((name.to_owned(), OnceLock::new()))
                    })
                    .collect()
            };
            build().map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))?;
    let validator = validators
        .get(name)
        .ok_or_else(|| anyhow!("Unknown command: {name}; use --help"))?
        .get_or_init(|| {
            let build = || -> Result<Option<jsonschema::Validator>> {
                Ok(definition(name)?
                    .get("inputSchema")
                    .map(jsonschema::validator_for)
                    .transpose()?)
            };
            build().map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))?;
    if let Some(validator) = validator
        && let Err(error) = validator.validate(input)
    {
        bail!("Invalid input: {error}");
    }
    Ok(())
}
pub fn entity(name: &str) -> Result<Value> {
    cached_catalog()?["entities"]
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
        "reasoning" => 512,
        "prompt" => usize::MAX,
        _ => 256,
    };
    if value.trim().is_empty()
        || value.encode_utf16().count() > max
        || (key == "reasoning" && value.len() > 512)
    {
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
