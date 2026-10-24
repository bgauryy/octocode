use super::super::mcp_clients::{ClientSpec, ConfigFormat};
use super::{document_keys, parse_document, pointer};
use serde_json::{Value, json};

pub(in super::super) fn render_document(
    client: &ClientSpec,
    text: &str,
    keys: &[String],
    next: Option<&Value>,
) -> Result<String, String> {
    let root = parse_document(client.format, text)?;
    let keys = document_keys(client, &root, keys);
    match client.format {
        ConfigFormat::Json => {
            let rendered = edit_json_members(text, &keys, root.pointer(&pointer(&keys)), next)?;
            let expected = expected_root(ConfigFormat::Json, text, &keys, next)?;
            ensure_expected(ConfigFormat::Json, rendered, &expected)
        }
        ConfigFormat::Toml => render_toml(text, &keys, next),
    }
}

pub(super) fn edit_json_members(
    text: &str,
    keys: &[String],
    current: Option<&Value>,
    next: Option<&Value>,
) -> Result<String, String> {
    if current == next {
        return Ok(text.to_owned());
    }
    if let (Some(current), Some(next)) = (
        current.and_then(Value::as_object),
        next.and_then(Value::as_object),
    ) {
        let mut rendered = text.to_owned();
        for key in current.keys().filter(|key| !next.contains_key(*key)) {
            let mut path = keys.to_vec();
            path.push(key.clone());
            rendered = edit_json_members(&rendered, &path, current.get(key), None)?;
        }
        for (key, value) in next {
            let mut path = keys.to_vec();
            path.push(key.clone());
            rendered = edit_json_members(&rendered, &path, current.get(key), Some(value))?;
        }
        return Ok(rendered);
    }
    octocode_native::config::edit_config_json(
        text,
        &keys.iter().map(String::as_str).collect::<Vec<_>>(),
        next,
    )
}

pub(super) fn expected_root(
    format: ConfigFormat,
    text: &str,
    keys: &[String],
    next: Option<&Value>,
) -> Result<Value, String> {
    let mut root = parse_document(format, text)?;
    let (last, parents) = keys.split_last().ok_or("Missing agent entry path.")?;
    let mut object = &mut root;
    for key in parents {
        if object.get(key).is_none() && next.is_none() {
            return Ok(root);
        }
        if object.get(key).is_none() {
            object[key] = json!({});
        }
        object = object.get_mut(key).ok_or("Missing config section.")?;
        if !object.is_object() {
            return Err("Agent config section is not an object.".into());
        }
    }
    let map = object
        .as_object_mut()
        .ok_or("Agent config section is not an object.")?;
    match next {
        Some(next) => {
            map.insert(last.clone(), next.clone());
        }
        None => {
            map.remove(last);
        }
    }
    Ok(root)
}

pub(super) fn ensure_expected(
    format: ConfigFormat,
    rendered: String,
    expected: &Value,
) -> Result<String, String> {
    if &parse_document(format, &rendered)? != expected {
        return Err("Agent layout cannot be edited safely; edit this file manually.".into());
    }
    Ok(rendered)
}

pub(super) fn has_inline_comment(line: &str) -> bool {
    if line.trim_start().starts_with('#') {
        return false;
    }
    let mut quote = None;
    let mut escaped = false;
    let mut previous: Option<char> = None;
    for character in line.chars() {
        let before = previous.replace(character);
        if escaped {
            escaped = false;
            continue;
        }
        if quote == Some('"') && character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(current) = quote {
            if character == current {
                quote = None;
            }
        } else if matches!(character, '\'' | '"')
            && before.is_none_or(|before| {
                before.is_whitespace() || matches!(before, ':' | '=' | '[' | '{' | ',')
            })
        {
            quote = Some(character);
        } else if character == '#' {
            return true;
        }
    }
    false
}

/// Line-based so user comments and formatting outside the Octocode table survive; `toml` re-serialization would drop them.
pub(super) fn render_toml(
    text: &str,
    keys: &[String],
    next: Option<&Value>,
) -> Result<String, String> {
    let expected = expected_root(ConfigFormat::Toml, text, keys, next)?;
    let mut rendered = String::new();
    let mut skipping = false;
    let mut found = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let header = trimmed.split('#').next().unwrap_or("").trim();
            skipping =
                header == "[mcp_servers.octocode]" || header.starts_with("[mcp_servers.octocode.");
            found |= skipping;
        }
        if skipping && has_inline_comment(line) {
            return Err("Inline comments inside the Octocode TOML section require manual editing to preserve their placement.".into());
        }
        if !skipping || trimmed.starts_with('#') {
            rendered.push_str(line);
        }
    }
    // Inline tables and quoted table names need a syntax-aware editor; reject them.
    let existing = parse_document(ConfigFormat::Toml, text)?
        .pointer("/mcp_servers/octocode")
        .is_some();
    if existing && !found {
        return Err("This TOML entry uses an unsupported layout; edit it manually.".into());
    }
    if let Some(next) = next {
        if !rendered.is_empty() && !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        let mut root = toml::Table::new();
        let entry =
            toml::Value::try_from(next).map_err(|_| "Cannot encode agent entry as TOML.")?;
        let mut servers = toml::Table::new();
        servers.insert("octocode".into(), entry);
        root.insert("mcp_servers".into(), toml::Value::Table(servers));
        rendered.push_str(
            &toml::to_string_pretty(&root).map_err(|_| "Cannot encode agent entry as TOML.")?,
        );
    }
    if next.is_none()
        && expected
            .pointer("/mcp_servers")
            .is_some_and(|value| value.as_object().is_some_and(serde_json::Map::is_empty))
        && parse_document(ConfigFormat::Toml, &rendered)?
            .get("mcp_servers")
            .is_none()
    {
        if !rendered.is_empty() && !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        rendered.push_str("[mcp_servers]\n");
    }
    ensure_expected(ConfigFormat::Toml, rendered, &expected)
}
