use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::OnceLock};

pub const SKILL: &str = include_str!("../skills/octocode-agents-communication/SKILL.md");
/// Lines a managed `run` worker never needs: the host owns identity, presence and delivery.
const WORKER_OMITS: [&str; 2] = ["Use bound tools, else", "   Without a host"];
fn body() -> impl Iterator<Item = &'static str> {
    SKILL.lines().skip_while(|line| !line.starts_with("# "))
}
fn join_lines<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    lines.map(|line| format!("{line}\n")).collect()
}
/// Plain `skill` returns the canonical file; a host profile drops the install frontmatter.
pub fn skill_instructions(vendor: Option<&str>) -> String {
    if vendor.is_none() {
        return SKILL.to_owned();
    }
    join_lines(body())
}
/// Workflow rules for a managed worker, without setup it must not perform.
pub fn worker_skill() -> String {
    join_lines(
        body()
            .take_while(|line| !line.starts_with("**"))
            .filter(|line| !WORKER_OMITS.iter().any(|prefix| line.starts_with(prefix))),
    )
}
pub fn catalog() -> Result<Value> {
    Ok(cached_catalog()?.clone())
}
/// A deferred delivery batch must fit its public receipt command.
pub fn delivery_batch_limit() -> Result<usize> {
    cached_catalog()?["commands"]
        .as_array()
        .and_then(|commands| commands.iter().find(|c| c["name"] == "confirm_delivery"))
        .and_then(|c| c["inputSchema"]["properties"]["items"]["maxItems"].as_u64())
        .and_then(|limit| usize::try_from(limit).ok())
        .filter(|limit| *limit > 0)
        .ok_or_else(|| anyhow!("Invalid confirmation batch limit in command catalog"))
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
    stored_lengths(input)?;
    jsonschema::validator_for(schema)?
        .validate(input)
        .map_err(|error| invalid(&error))
}
/// Report where and which rule failed, never the offending value: a rejected
/// 1 MiB document must not come back as a 1 MiB error.
fn invalid(error: &jsonschema::ValidationError) -> anyhow::Error {
    let at = error.instance_path().to_string();
    let message = error
        .masked_with(if at.is_empty() { "input" } else { &at })
        .to_string();
    match message.char_indices().nth(160) {
        Some((end, _)) => anyhow!("Invalid input: {}…", &message[..end]),
        None => anyhow!("Invalid input: {message}"),
    }
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
        "package":"@octocodeai/octocode-agents-communication", "implementation":"Rust",
        "usage":"scripts/agents-communication <command> [json|-] --workspace <path> [--database <file>] [--session <id>]",
        "commands":commands,
        "discover":["skill", "<command> --help", "schema entity <name>", "db info"],
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
    stored_lengths(input)?;
    match validator {
        Some(validator) => validator.validate(input).map_err(|error| invalid(&error)),
        None => Ok(()),
    }
}
pub fn entity(name: &str) -> Result<Value> {
    cached_catalog()?["entities"]
        .as_array()
        .and_then(|items| items.iter().find(|c| c["name"] == name))
        .cloned()
        .ok_or_else(|| anyhow!("Unknown entity: {name}"))
}
/// The one text-length authority. Units match storage: `reasoning` is capped in
/// UTF-8 bytes by the DB trigger and `content` by the document store; other text
/// uses UTF-16 units like `sqlite_agent.py`. Schema `maxLength` counts code points,
/// so the same number there is never the stricter check.
fn stored_limit(key: &str) -> Option<(usize, bool)> {
    match key {
        "reasoning" => Some((512, true)),
        "content" => Some((1024 * 1024, true)),
        "body" => Some((16384, false)),
        "path" => Some((4096, false)),
        "prompt" => None,
        _ => Some((256, false)),
    }
}
fn check_length(key: &str, value: &str) -> Result<()> {
    if let Some((max, bytes)) = stored_limit(key) {
        let (used, unit) = if bytes {
            (value.len(), "UTF-8 bytes")
        } else {
            (value.encode_utf16().count(), "UTF-16 units")
        };
        if used > max {
            let mib = 1024 * 1024;
            if max % mib == 0 {
                bail!("Invalid {key}: {used} {unit} exceeds {} MiB", max / mib);
            }
            bail!("Invalid {key}: {used} {unit} exceeds {max}");
        }
    }
    Ok(())
}
/// Apply storage units before schema validation so an over-limit field fails
/// with its name, unit and limit.
fn stored_lengths(input: &Value) -> Result<()> {
    match input {
        Value::Object(fields) => fields.iter().try_for_each(|(key, value)| match value {
            Value::String(text)
                if matches!(key.as_str(), "reasoning" | "content" | "body" | "path") =>
            {
                check_length(key, text)
            }
            other => stored_lengths(other),
        }),
        Value::Array(items) => items.iter().try_for_each(stored_lengths),
        _ => Ok(()),
    }
}
pub fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    let value = input[key]
        .as_str()
        .ok_or_else(|| anyhow!("Missing string: {key}"))?;
    if value.trim().is_empty() {
        bail!("Invalid {key}: blank");
    }
    check_length(key, value)?;
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
#[cfg(test)]
mod tests {
    use super::*;

    fn error(name: &str, input: &Value) -> String {
        command(name, input)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn invalid_input_reports_location_and_rule_never_the_value() {
        let mut extra = serde_json::Map::new();
        extra.insert("k".repeat(100_000), json!(1));
        extra.insert("name".into(), json!("x.md"));
        extra.insert("content".into(), json!(""));
        extra.insert("reasoning".into(), json!("r"));
        for (name, input) in [
            (
                "share_document",
                json!({"name":"x.md","content":"a".repeat(1024 * 1024 + 1),"reasoning":"r"}),
            ),
            (
                "share_document",
                json!({"name":"B".repeat(100_000),"content":"","reasoning":"r"}),
            ),
            ("share_document", Value::Object(extra)),
            (
                "send_message",
                json!({"body":"b","reasoning":"r","ttlMs":"x".repeat(100_000)}),
            ),
        ] {
            let message = error(name, &input);
            assert!(message.starts_with("Invalid"), "{message}");
            assert!(message.len() < 300, "{} bytes", message.len());
        }
        let message = error(
            "send_message",
            &json!({"body":"b","reasoning":"r","wake":"x".repeat(1000)}),
        );
        assert!(message.contains("/wake"), "{message}");
    }

    #[test]
    fn one_length_authority_names_field_unit_and_limit() -> Result<()> {
        let reasoning = error(
            "send_message",
            &json!({"body":"b","reasoning":"é".repeat(300)}),
        );
        assert_eq!(reasoning, "Invalid reasoning: 600 UTF-8 bytes exceeds 512");
        let body = error(
            "send_message",
            &json!({"body":"😀".repeat(9000),"reasoning":"r"}),
        );
        assert_eq!(body, "Invalid body: 18000 UTF-16 units exceeds 16384");
        let nested = error(
            "lock_many",
            &json!({"paths":[{"path":"😀".repeat(2049)}],"reasoning":"r"}),
        );
        assert!(
            nested.starts_with("Invalid path: 4098 UTF-16 units"),
            "{nested}"
        );
        let input = json!({"body":"😀".repeat(8192),"reasoning":"é".repeat(256)});
        command("send_message", &input)?;
        assert_eq!(text(&input, "body")?.chars().count(), 8192);
        let blank = text(&json!({"body":" \n"}), "body")
            .err()
            .map(|e| e.to_string());
        assert_eq!(blank.as_deref(), Some("Invalid body: blank"));
        Ok(())
    }

    #[test]
    fn discovery_surfaces_stay_within_token_budgets() -> Result<()> {
        let tools = serde_json::to_string(&selected_tools(None)?)?;
        assert_eq!(selected_tools(None)?.len(), 14);
        assert!(!tools.contains("$schema"));
        assert!(tools.len() <= 12_000, "tools/list {} bytes", tools.len());
        let mut total = 0;
        for command in cached_catalog()?["commands"]
            .as_array()
            .ok_or_else(|| anyhow!("commands"))?
        {
            let size = serde_json::to_string(command)?.len();
            assert!(size <= 1_600, "{} help {size} bytes", command["name"]);
            total += size;
        }
        assert!(total <= 26_000, "help total {total} bytes");
        assert!(serde_json::to_string(&help()?)?.len() <= 1_200);
        Ok(())
    }

    #[test]
    fn skill_profiles_drop_what_the_reader_cannot_use() {
        assert!(SKILL.lines().count() <= 50);
        for prefix in WORKER_OMITS {
            assert!(
                SKILL.lines().any(|line| line.starts_with(prefix)),
                "{prefix}"
            );
        }
        let claude = skill_instructions(Some("claude"));
        assert!(claude.starts_with("# ") && claude.contains("**Delivery setup:**"));
        assert_eq!(skill_instructions(None), SKILL);
        let worker = worker_skill();
        assert!(worker.contains("ackReply") && worker.contains("leaseId"));
        for setup in [
            "**Delivery setup:**",
            "heartbeat",
            "scripts/agents-communication",
        ] {
            assert!(!worker.contains(setup), "{setup}");
        }
        assert!(worker.len() * 10 < SKILL.len() * 8);
    }
}
