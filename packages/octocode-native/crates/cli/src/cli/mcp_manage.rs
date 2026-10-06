//! Sanitized discovery and revision-checked edits of Octocode entries only.
use super::config::ManageError;
use super::mcp_clients::{self, CLIENTS, ClientSpec, ConfigFormat, EnableFlag, EntryShape};
use super::mcp_install;
use octocode_native::config::{config_revision, read_private_config, replace_private_config};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

struct Target {
    client: &'static ClientSpec,
    scope: String,
    path: PathBuf,
    keys: Vec<String>,
}

fn target(client: &str, scope: &str, cwd: &Path) -> Result<Target, String> {
    let client = mcp_clients::client(client).ok_or("Unsupported agent client.")?;
    if scope != "workspace" && client.config_dir_overridden() {
        return Err("Claude uses a custom config directory; its user/local config path cannot be established safely. Use Claude's configuration command.".into());
    }
    let mut keys = client.server_keys();
    let path = match (scope, client.workspace) {
        ("home", _) => client.home_path().ok_or("Cannot resolve home directory.")?,
        ("workspace", Some(relative)) => cwd.join(relative),
        ("local", _) if client.local => {
            keys.splice(
                0..0,
                ["projects".to_owned(), cwd.to_string_lossy().into_owned()],
            );
            client.home_path().ok_or("Cannot resolve home directory.")?
        }
        _ => return Err("This agent does not support the requested scope.".into()),
    };
    Ok(Target {
        client,
        scope: scope.to_owned(),
        path,
        keys,
    })
}

pub fn manage(request: &Value, cwd: &Path) -> Result<Value, ManageError> {
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("Missing operation.")?;
    if operation == "agents" {
        let mut agents = Vec::new();
        for client in &CLIENTS {
            for scope in client.scopes() {
                match target(client.id, scope, cwd) {
                    Ok(target) => agents.push(inspect(&target)),
                    Err(error) => agents.push(json!({"client":client.id,"scope":scope,"status":"invalid","writable":false,"error":error})),
                }
            }
        }
        if let Some(client) = mcp_clients::client("vscode-continue")
            && let Some(path) = client.home_path()
            && let Some(directory) = path.parent().and_then(Path::parent)
        {
            agents.extend(continue_main_rows(client, directory));
        }
        return Ok(json!({"agents":agents}));
    }
    if !matches!(operation, "setAgent" | "removeAgent") {
        return Err("Unsupported agent operation.".into());
    }
    let client = request
        .get("client")
        .and_then(Value::as_str)
        .ok_or("Missing client.")?;
    let scope = request
        .get("scope")
        .and_then(Value::as_str)
        .ok_or("Missing scope.")?;
    let target = target(client, scope, cwd)?;
    update(&target, request, operation == "removeAgent")
}

/// Read-only rows for Continue's main configuration files that exist.
fn continue_main_rows(client: &ClientSpec, directory: &Path) -> Vec<Value> {
    [("config.yaml", true), ("config.json", false)]
        .into_iter()
        .map(|(name, yaml)| (directory.join(name), yaml))
        .filter(|(path, _)| std::fs::symlink_metadata(path).is_ok())
        .map(|(path, yaml)| inspect_continue_main(client, &path, yaml))
        .collect()
}

fn inspect_continue_main(client: &ClientSpec, path: &Path, yaml: bool) -> Value {
    let scope = if yaml {
        "home-config-yaml"
    } else {
        "home-config-json"
    };
    let mut row = json!({"client":client.id,"scope":scope,"path":path,"writable":false,"supportsEnabled":false,"restartRequired":false,"configured":false,"status":"absent","entries":[],"error":"Continue main configuration uses an array of servers. Manage these entries in Continue; the Octocode standalone block can be edited here."});
    let original = match read_private_config(path) {
        Ok(text) => text,
        Err(error) => {
            row["status"] = json!("invalid");
            row["error"] = json!(format!(
                "Cannot read Continue configuration safely: {error}"
            ));
            return row;
        }
    };
    row["revision"] = json!(config_revision(original.as_deref()));
    let Some(text) = original else {
        return row;
    };
    let parsed = if yaml {
        serde_yaml_ng::from_str::<Value>(&text)
            .map_err(|_| "Continue configuration is not valid YAML.".to_owned())
    } else {
        octocode_native::config::parse_config_json(&text)
    };
    let root = match parsed {
        Ok(root) => root,
        Err(error) => {
            row["status"] = json!("invalid");
            row["error"] = json!(error);
            return row;
        }
    };
    let mut entries = Vec::new();
    let mut unresolved = false;
    for servers in [
        root.get("mcpServers"),
        root.pointer("/experimental/modelContextProtocolServers"),
    ] {
        if let Some(servers) = servers.and_then(Value::as_array) {
            for server in servers {
                unresolved |= server.get("uses").is_some();
                let transport = server.get("transport").unwrap_or(server);
                if transport
                    .get("args")
                    .and_then(Value::as_array)
                    .is_some_and(|args| {
                        args.iter().any(|arg| {
                            arg.as_str().is_some_and(|arg| {
                                arg == "octocode-mcp" || arg.starts_with("octocode-mcp@")
                            })
                        })
                    })
                    || server
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.eq_ignore_ascii_case("octocode"))
                {
                    entries.push(projection(client, Some(transport)));
                }
            }
        }
    }
    row["configured"] = if unresolved && entries.is_empty() {
        Value::Null
    } else {
        json!(!entries.is_empty())
    };
    row["status"] = json!(if !entries.is_empty() {
        "configured"
    } else if unresolved {
        "unknown"
    } else {
        "absent"
    });
    row["coverage"] =
        json!("Inline Octocode package/name entries; imported MCP blocks are not resolved.");
    if unresolved {
        row["error"] = json!(
            "Continue imports MCP blocks whose contents are not inspected. Manage this main configuration in Continue."
        );
    }
    row["entries"] = json!(entries);
    row
}

fn pointer(keys: &[String]) -> String {
    keys.iter()
        .map(|key| format!("/{}", key.replace('~', "~0").replace('/', "~1")))
        .collect()
}

pub(super) fn parse_document(format: ConfigFormat, text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value = match format {
        ConfigFormat::Json => octocode_native::config::parse_config_json(text),
        ConfigFormat::Toml => toml::from_str::<toml::Value>(text)
            .ok()
            .and_then(|value| serde_json::to_value(value).ok())
            .ok_or_else(|| "Agent config is not valid TOML.".to_owned()),
        ConfigFormat::Yaml => serde_yaml_ng::from_str::<Value>(text)
            .map_err(|_| "Agent config is not valid YAML.".to_owned()),
    }?;
    if !value.is_object() {
        return Err("Agent config must contain an object or mapping.".into());
    }
    Ok(value)
}

pub(super) fn document_keys(client: &ClientSpec, root: &Value, keys: &[String]) -> Vec<String> {
    if client.nested_servers(root) && keys == ["mcp", "octocode"] {
        vec!["mcp".into(), "servers".into(), "octocode".into()]
    } else {
        keys.to_vec()
    }
}

pub(super) fn entry_at<'a>(
    client: &ClientSpec,
    root: &'a Value,
    keys: &[String],
) -> Option<&'a Value> {
    root.pointer(&pointer(&document_keys(client, root, keys)))
}

pub(super) fn projection(client: &ClientSpec, server: Option<&Value>) -> Value {
    let Some(server) = server else {
        return Value::Null;
    };
    let env_keys: Vec<_> = server
        .get(client.shape.env_key())
        .and_then(Value::as_object)
        .map(|env| env.keys().cloned().collect())
        .unwrap_or_default();
    let command = server.get(client.shape.command_key());
    let command = if client.shape == EntryShape::Opencode {
        command
            .and_then(Value::as_array)
            .and_then(|args| args.first())
    } else {
        command
    };
    let runner = command
        .and_then(Value::as_str)
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str());
    let method = match runner {
        Some("npx" | "npx.cmd") => Some("npx"),
        Some("bunx" | "bunx.exe") => Some("bunx"),
        Some("pnpm" | "pnpm.cmd") => Some("pnpm"),
        _ => None,
    };
    json!({"method":method,"customCommand":method.is_none(),"enabled":if client.enable.is_some() { Some(!disabled(server)) } else { None },"envKeys":env_keys})
}

pub(super) fn entry_enabled(client: &ClientSpec, root: &Value, server: &Value) -> Option<bool> {
    client.enable?;
    Some(if uses_enabled_key(client, root) {
        server
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    } else {
        !server
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}

/// Whether the client's enable flag is `enabled` (else `disabled`).
fn uses_enabled_key(client: &ClientSpec, root: &Value) -> bool {
    client.enable == Some(EnableFlag::Enabled) && !client.nested_servers(root)
}

fn disabled(server: &Value) -> bool {
    server.get("disabled").and_then(Value::as_bool) == Some(true)
        || server.get("enabled").and_then(Value::as_bool) == Some(false)
}

fn inspect(target: &Target) -> Value {
    let original = match read_private_config(&target.path) {
        Ok(text) => text,
        Err(error) => {
            return json!({"client":target.client.id,"scope":target.scope,"path":target.path,"status":"invalid","configured":false,"writable":false,"restartRequired":false,"error":format!("Agent config cannot be read safely: {error}")});
        }
    };
    let revision = config_revision(original.as_deref());
    let root = match parse_document(target.client.format, original.as_deref().unwrap_or("")) {
        Ok(root) => root,
        Err(error) => {
            return json!({"client":target.client.id,"scope":target.scope,"path":target.path,"status":"invalid","configured":false,"revision":revision,"writable":false,"restartRequired":false,"error":error});
        }
    };
    let server = entry_at(target.client, &root, &target.keys);
    let status = match server {
        None => "absent",
        Some(server) if !server.is_object() => "invalid",
        Some(server) if entry_enabled(target.client, &root, server) == Some(false) => "disabled",
        _ => "configured",
    };
    // Render a no-op to prove this layout can be safely edited without rewriting other content.
    let render_result = render_document(
        target.client,
        original.as_deref().unwrap_or(""),
        &target.keys,
        server,
    );
    let writable = render_result.is_ok();
    let error = render_result.err();
    let status = if !writable && server.is_none() {
        "invalid"
    } else {
        status
    };
    let mut projected = projection(target.client, server);
    if let Some(server) = server {
        projected["enabled"] = json!(entry_enabled(target.client, &root, server));
    }
    json!({"client":target.client.id,"scope":target.scope,"path":target.path,"status":status,"configured":server.is_some(),"revision":revision,"writable":writable,"restartRequired":false,"supportsEnabled":target.client.enable.is_some(),"entry":projected,"error":error})
}

fn update(target: &Target, request: &Value, remove: bool) -> Result<Value, ManageError> {
    let revision = request
        .get("revision")
        .and_then(Value::as_str)
        .ok_or("Missing revision; refresh agent configuration before editing.")?;
    let original = read_private_config(&target.path)
        .map_err(|error| format!("Agent config cannot be read safely: {error}"))?;
    let root = parse_document(target.client.format, original.as_deref().unwrap_or(""))?;
    let next = if remove {
        None
    } else {
        let patch = request
            .get("patch")
            .and_then(Value::as_object)
            .ok_or("Missing agent patch.")?;
        if patch
            .keys()
            .any(|key| !matches!(key.as_str(), "env" | "enabled" | "method"))
        {
            return Err("Unknown agent patch field.".into());
        }
        let mut server = entry_at(target.client, &root, &target.keys)
            .cloned()
            .unwrap_or_else(|| default_entry(target.client, "npx"));
        if !server.is_object() {
            return Err("Octocode entry must be an object; remove the invalid entry first.".into());
        }
        if let Some(method) = patch.get("method") {
            let method = method
                .as_str()
                .filter(|method| matches!(*method, "npx" | "bunx" | "pnpm"))
                .ok_or("Unsupported installation method.")?;
            let generated = default_entry(target.client, method);
            for key in ["command", "cmd", "args", "type"] {
                if let Some(value) = generated.get(key) {
                    server[key] = value.clone();
                }
            }
        }
        if let Some(enabled) = patch.get("enabled") {
            if target.client.enable.is_none() {
                return Err("This agent's file format does not expose a supported enable flag; use its own MCP controls or remove the entry.".into());
            }
            let enabled = enabled.as_bool().ok_or("enabled must be a boolean.")?;
            if uses_enabled_key(target.client, &root) {
                server["enabled"] = json!(enabled);
            } else {
                server["disabled"] = json!(!enabled);
            }
            // Do not leave an opposite disabling flag behind.
            if enabled && let Some(map) = server.as_object_mut() {
                map.remove("disabled");
                if map.contains_key("enabled") {
                    map.insert("enabled".into(), json!(true));
                }
            }
        }
        if let Some(env) = patch.get("env") {
            let env = env.as_object().ok_or("env must be an object.")?;
            let env_key = target.client.shape.env_key();
            if server.get(env_key).is_none() {
                server[env_key] = json!({});
            }
            let values = server
                .get_mut(env_key)
                .and_then(Value::as_object_mut)
                .ok_or("Existing agent environment is not an object.")?;
            for (key, value) in env {
                let value = if value.is_null() {
                    None
                } else {
                    Some(
                        value
                            .as_str()
                            .ok_or("Environment values must be strings or null.")?,
                    )
                };
                octocode_native::config::validate_env_edit(key, value, target.scope == "workspace")
                    .map_err(|error| error.to_string())?;
                match value {
                    Some(value) => {
                        values.insert(key.clone(), json!(value));
                    }
                    None => {
                        values.remove(key);
                    }
                }
            }
        }
        Some(server)
    };
    let rendered = render_document(
        target.client,
        original.as_deref().unwrap_or(""),
        &target.keys,
        next.as_ref(),
    )?;
    let (changed, _) = replace_private_config(&target.path, revision, &rendered, true)
        .map_err(|error| ManageError::from_edit(&error, &error.to_string()))?;
    let mut result = inspect(target);
    result["changed"] = json!(changed);
    result["restartRequired"] = json!(changed);
    Ok(result)
}

pub(super) fn default_entry(client: &ClientSpec, method: &str) -> Value {
    let args = mcp_install::InstallArgs {
        ide: Some(client.id.into()),
        force: false,
        dry_run: false,
        check: false,
        list: false,
        json: true,
        enable_local: None,
        pass_env: false,
        method: Some(method.into()),
        rollback: None,
    };
    mcp_install::json_server(client.shape, &args)
}

pub(super) fn render_document(
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
        ConfigFormat::Yaml => render_yaml(text, &keys, next),
    }
}

fn edit_json_members(
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

fn expected_root(
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

fn ensure_expected(
    format: ConfigFormat,
    rendered: String,
    expected: &Value,
) -> Result<String, String> {
    if &parse_document(format, &rendered)? != expected {
        return Err("Agent layout cannot be edited safely; edit this file manually.".into());
    }
    Ok(rendered)
}

fn has_inline_comment(line: &str, yaml: bool) -> bool {
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
        } else if character == '#' && (!yaml || before.is_none_or(char::is_whitespace)) {
            return true;
        }
    }
    false
}

/// Line-based so user comments and formatting outside the Octocode table survive; `toml` re-serialization would drop them.
fn render_toml(text: &str, keys: &[String], next: Option<&Value>) -> Result<String, String> {
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
        if skipping && has_inline_comment(line, false) {
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
            .is_some_and(|value| value.as_object().is_some_and(|object| object.is_empty()))
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

/// Line-based so user comments and formatting outside the Octocode entry survive; `serde_yaml_ng` re-serialization would drop them.
fn render_yaml(text: &str, keys: &[String], next: Option<&Value>) -> Result<String, String> {
    let expected = expected_root(ConfigFormat::Yaml, text, keys, next)?;
    let mut lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start = lines
        .iter()
        .position(|line| matches!(line.trim_end(), "extensions:" | "extensions: {}"));
    let mut rendered = String::new();
    if let Some(start) = start {
        let end = (start + 1..lines.len())
            .find(|&i| {
                !lines[i].trim().is_empty()
                    && !lines[i].trim_start().starts_with('#')
                    && !lines[i].starts_with(' ')
            })
            .unwrap_or(lines.len());
        let entry_start = (start + 1..end).find(|&i| lines[i].trim_end() == "  octocode:");
        if let Some(entry_start) = entry_start {
            let entry_end = (entry_start + 1..end)
                .find(|&i| {
                    !lines[i].trim().is_empty()
                        && !lines[i].trim_start().starts_with('#')
                        && !lines[i].starts_with("    ")
                })
                .unwrap_or(end);
            if lines[entry_start..entry_end]
                .iter()
                .any(|line| has_inline_comment(line, true))
            {
                return Err("Inline comments inside the Octocode YAML section require manual editing to preserve their placement.".into());
            }
            let comments: String = lines[entry_start..entry_end]
                .iter()
                .filter(|line| line.trim_start().starts_with('#'))
                .copied()
                .collect();
            lines.drain(entry_start..entry_end);
            rendered = lines.concat();
            if !comments.is_empty() {
                let insertion = rendered
                    .split_inclusive('\n')
                    .take(start + 1)
                    .map(str::len)
                    .sum::<usize>();
                rendered.insert_str(insertion, &comments);
            }
        } else {
            rendered = text.to_owned();
        }
        if let Some(next) = next {
            if lines
                .get(start)
                .is_some_and(|line| line.trim_end() == "extensions: {}")
            {
                let mut parts: Vec<_> = rendered.split_inclusive('\n').map(str::to_owned).collect();
                parts[start] = "extensions:\n".into();
                rendered = parts.concat();
            }
            if !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            let insertion = rendered
                .split_inclusive('\n')
                .take(start + 1)
                .map(str::len)
                .sum::<usize>();
            let encoded = serde_yaml_ng::to_string(&json!({"octocode":next}))
                .map_err(|_| "Cannot encode agent entry as YAML.")?;
            let block: String = encoded.lines().map(|line| format!("  {line}\n")).collect();
            rendered.insert_str(insertion, &block);
        }
    } else {
        if parse_document(ConfigFormat::Yaml, text)?
            .get("extensions")
            .is_some()
        {
            return Err(
                "This YAML extensions mapping uses an unsupported layout; edit it manually.".into(),
            );
        }
        rendered.push_str(text);
        if let Some(next) = next {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push_str(
                &serde_yaml_ng::to_string(&json!({"extensions":{"octocode":next}}))
                    .map_err(|_| "Cannot encode agent entry as YAML.")?,
            );
        }
    }
    if next.is_none()
        && expected
            .get("extensions")
            .is_some_and(|value| value.as_object().is_some_and(|map| map.is_empty()))
        && parse_document(ConfigFormat::Yaml, &rendered)?
            .get("extensions")
            .is_some_and(Value::is_null)
    {
        let mut parts: Vec<_> = rendered.split_inclusive('\n').map(str::to_owned).collect();
        if let Some(index) = parts
            .iter()
            .position(|line| line.trim_end() == "extensions:")
        {
            parts[index] = "extensions: {}\n".into();
        }
        rendered = parts.concat();
    }
    ensure_expected(ConfigFormat::Yaml, rendered, &expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> &'static ClientSpec {
        mcp_clients::client(id).expect("supported client")
    }

    fn fixture(client: &'static ClientSpec, dir: &Path) -> Target {
        Target {
            client,
            scope: "home".into(),
            path: dir.join(match client.format {
                ConfigFormat::Json => "agent.json",
                ConfigFormat::Toml => "agent.toml",
                ConfigFormat::Yaml => "agent.yaml",
            }),
            keys: client.server_keys(),
        }
    }

    fn initial(client: &ClientSpec) -> &'static str {
        match client.format {
            ConfigFormat::Json => {
                "{\n // Keep my theme\n \"theme\": \"dark\",\n \"other\": {\"token\":\"unrelated-secret\"}\n}\n"
            }
            ConfigFormat::Toml => {
                "# Keep my theme\ntheme = \"dark\"\n[mcp_servers.other]\ncommand = \"unrelated-secret\"\nargs = []\n"
            }
            ConfigFormat::Yaml => {
                "# Keep my theme\ntheme: dark\nextensions:\n  other:\n    cmd: unrelated-secret\n"
            }
        }
    }

    #[test]
    fn every_agent_updates_redacts_preserves_and_rejects_stale_edits() {
        for client in &CLIENTS {
            let id = client.id;
            let dir = tempfile::tempdir().expect("tempdir");
            let canonical = dir.path().canonicalize().expect("canonical path");
            let target = fixture(client, &canonical);
            let original = initial(client);
            std::fs::write(&target.path, original).expect("fixture");
            let discovered = inspect(&target);
            assert_eq!(discovered["status"], "absent", "{id}: {discovered}");
            assert_eq!(discovered["writable"], true, "{id}: {discovered}");
            let mut request = json!({"revision":discovered["revision"],"patch":{"env":{"API_KEY":"octocode-secret"},"method":"bunx"}});
            if client.enable.is_some() {
                request["patch"]["enabled"] = json!(false);
            }
            let result =
                update(&target, &request, false).unwrap_or_else(|error| panic!("{id}: {error}"));
            assert_eq!(
                result["status"],
                if client.enable.is_some() {
                    "disabled"
                } else {
                    "configured"
                },
                "{id}: {result}"
            );
            assert_eq!(result["entry"]["method"], "bunx");
            assert_eq!(result["entry"]["envKeys"], json!(["API_KEY"]));
            let sanitized = result.to_string();
            assert!(
                !sanitized.contains("octocode-secret") && !sanitized.contains("unrelated-secret")
            );
            let contents = std::fs::read_to_string(&target.path).expect("read");
            assert!(contents.contains("Keep my theme"), "{id}: {contents}");
            assert!(contents.contains("unrelated-secret"), "{id}: {contents}");
            assert!(matches!(
                update(&target, &request, false).expect_err("stale"),
                ManageError::Conflict(_)
            ));
            assert_eq!(
                std::fs::read_to_string(&target.path).expect("read"),
                contents
            );
            let result = update(
                &target,
                &{
                    let mut patch =
                        json!({"revision":result["revision"],"patch":{"env":{"API_KEY":null}}});
                    if client.enable.is_some() {
                        patch["patch"]["enabled"] = json!(true);
                    }
                    patch
                },
                false,
            )
            .expect("enable");
            assert_eq!(result["status"], "configured");
            assert_eq!(result["entry"]["envKeys"], json!([]));
            let result =
                update(&target, &json!({"revision":result["revision"]}), true).expect("remove");
            assert_eq!(result["status"], "absent");
            let root = parse_document(
                client.format,
                &std::fs::read_to_string(&target.path).expect("read"),
            )
            .expect("parse");
            assert_eq!(root["theme"], "dark");
        }
    }

    #[test]
    fn all_absent_formats_can_be_created_then_removed() {
        for client in ["cursor", "codex", "goose"] {
            let dir = tempfile::tempdir().expect("dir");
            let target = fixture(spec(client), &dir.path().canonicalize().expect("canonical"));
            let result = update(
                &target,
                &json!({"revision":config_revision(None),"patch":{"method":"npx"}}),
                false,
            )
            .expect("create");
            update(&target, &json!({"revision":result["revision"]}), true).expect("remove");
        }
    }

    #[test]
    fn malformed_and_ambiguous_files_are_read_only_without_parser_secret_echoes() {
        for (client, text) in [
            ("cursor", "{\"token\":\"dont-echo-secret\""),
            ("codex", "token = \"dont-echo-secret"),
            ("goose", "{token: dont-echo-secret"),
        ] {
            let dir = tempfile::tempdir().expect("dir");
            let target = fixture(spec(client), &dir.path().canonicalize().expect("canonical"));
            std::fs::write(&target.path, text).expect("fixture");
            let result = inspect(&target);
            assert_eq!(result["status"], "invalid");
            assert_eq!(result["writable"], false);
            assert!(!result.to_string().contains("dont-echo-secret"));
        }
        let text =
            "mcp_servers = { octocode = { command = \"npx\", args = [\"octocode-mcp\"] } }\n";
        assert!(
            render_toml(
                text,
                &["mcp_servers".into(), "octocode".into()],
                Some(&default_entry(spec("codex"), "npx"))
            )
            .is_err()
        );
        let text = "extensions: {octocode: {cmd: npx, args: [octocode-mcp]}}\n";
        assert!(
            render_yaml(
                text,
                &["extensions".into(), "octocode".into()],
                Some(&default_entry(spec("goose"), "npx"))
            )
            .is_err()
        );
    }

    #[test]
    fn untrusted_command_args_and_env_values_never_reach_projection() {
        for client in &CLIENTS {
            let projected = projection(
                client,
                Some(
                    &json!({"command":["secret-executable","--token=secret-arg"],"cmd":"secret-command","args":["secret-arg"],"env":{"API":"secret-env"},"environment":{"API":"secret-env"},"envs":{"API":"secret-env"}}),
                ),
            );
            assert!(!projected.to_string().contains("secret"));
        }
    }

    #[test]
    fn protected_env_keys_are_rejected_without_any_write() {
        let dir = tempfile::tempdir().expect("dir");
        let target = fixture(
            spec("cursor"),
            &dir.path().canonicalize().expect("canonical"),
        );
        let error=update(&target,&json!({"revision":config_revision(None),"patch":{"env":{"NODE_OPTIONS":"--import attacker"}}}),false).expect_err("protected");
        assert!(!target.path.exists());
        assert!(!error.to_string().contains("attacker"));
    }

    #[test]
    fn concurrent_updates_have_at_most_one_success() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().canonicalize().expect("canonical");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads:Vec<_>=(0..2).map(|index| {
            let path=path.clone(); let barrier=barrier.clone();
            std::thread::spawn(move || { let target=fixture(spec("cursor"),&path); barrier.wait(); update(&target,&json!({"revision":config_revision(None),"patch":{"env":{"API":index.to_string()}}}),false) })
        }).collect();
        let successes = threads
            .into_iter()
            .map(|thread| thread.join().expect("thread"))
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_is_rejected_and_external_file_untouched() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().canonicalize().expect("canonical");
        let target = fixture(spec("cursor"), &path);
        let external = path.join("external.json");
        std::fs::write(&external, "{}").expect("fixture");
        std::os::unix::fs::symlink(&external, &target.path).expect("symlink");
        assert_eq!(inspect(&target)["writable"], false);
        assert!(
            update(
                &target,
                &json!({"revision":config_revision(Some("{}")),"patch":{}}),
                false
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(external).expect("read"), "{}");
    }

    #[test]
    fn nested_claude_local_entry_preserves_sibling_projects() {
        let text = "{\"projects\":{\"/a\":{\"theme\":\"dark\"},\"/b\":{\"mcpServers\":{\"other\":{\"secret\":\"keep\"}}}}}";
        let keys = [
            "projects".into(),
            "/a".into(),
            "mcpServers".into(),
            "octocode".into(),
        ];
        let client = spec("claude-code");
        let rendered = render_document(client, text, &keys, Some(&default_entry(client, "npx")))
            .expect("render");
        let root = parse_document(ConfigFormat::Json, &rendered).expect("parse");
        assert_eq!(root["projects"]["/a"]["theme"], "dark");
        assert_eq!(
            root["projects"]["/b"]["mcpServers"]["other"]["secret"],
            "keep"
        );
    }
    #[test]
    fn opencode_v2_updates_the_servers_mapping_and_disabled_flag() {
        let dir = tempfile::tempdir().expect("dir");
        let target = fixture(
            spec("opencode"),
            &dir.path().canonicalize().expect("canonical"),
        );
        let text = r#"{"mcp":{"servers":{"other":{"command":["keep-secret"]}}},"theme":"dark"}"#;
        std::fs::write(&target.path, text).expect("fixture");
        let result=update(&target,&json!({"revision":config_revision(Some(text)),"patch":{"enabled":false,"env":{"API":"secret"}}}),false).expect("update V2");
        assert_eq!(result["status"], "disabled");
        let root = parse_document(
            ConfigFormat::Json,
            &std::fs::read_to_string(&target.path).expect("read"),
        )
        .expect("parse");
        assert_eq!(root["mcp"]["servers"]["octocode"]["disabled"], true);
        assert!(root["mcp"].get("octocode").is_none());
        assert_eq!(
            root["mcp"]["servers"]["other"]["command"],
            json!(["keep-secret"])
        );
    }

    #[test]
    fn unsupported_enable_toggle_is_explicit_and_does_not_write() {
        let dir = tempfile::tempdir().expect("dir");
        let target = fixture(
            spec("claude-code"),
            &dir.path().canonicalize().expect("canonical"),
        );
        assert!(
            update(
                &target,
                &json!({"revision":config_revision(None),"patch":{"enabled":false}}),
                false
            )
            .expect_err("unsupported")
            .to_string()
            .contains("supported enable flag")
        );
        assert!(!target.path.exists());
    }
    #[test]
    fn absent_continue_main_files_produce_no_rows() {
        let dir = tempfile::tempdir().expect("dir");
        let client = spec("vscode-continue");
        assert!(continue_main_rows(client, dir.path()).is_empty());
        std::fs::write(dir.path().join("config.yaml"), "mcpServers: []\n").expect("fixture");
        let rows = continue_main_rows(client, dir.path());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["scope"], "home-config-yaml");
    }
    #[test]
    fn continue_main_yaml_and_legacy_json_are_sanitized_read_only_sources() {
        let dir = tempfile::tempdir().expect("dir");
        let directory = dir.path().canonicalize().expect("canonical");
        let yaml = directory.join("config.yaml");
        std::fs::write(&yaml,"models:\n  - apiKey: unrelated-secret\nmcpServers:\n  - name: octocode\n    command: npx\n    args: [octocode-mcp@latest, secret-argument]\n    env: {TOKEN: secret-token}\n").expect("fixture");
        let result = inspect_continue_main(spec("vscode-continue"), &yaml, true);
        assert_eq!(result["configured"], true);
        assert_eq!(result["writable"], false);
        assert_eq!(result["entries"][0]["envKeys"], json!(["TOKEN"]));
        assert!(!result.to_string().contains("secret"));
        let legacy = directory.join("config.json");
        std::fs::write(&legacy,r#"{"experimental":{"modelContextProtocolServers":[{"transport":{"command":"npx","args":["octocode-mcp@latest"],"env":{"TOKEN":"secret-token"}}}]}}"#).expect("fixture");
        let result = inspect_continue_main(spec("vscode-continue"), &legacy, false);
        assert_eq!(result["configured"], true);
        assert_eq!(result["writable"], false);
        assert!(!result.to_string().contains("secret-token"));
        std::fs::write(&yaml, "mcpServers:\n  - uses: organization/block\n")
            .expect("import fixture");
        let result = inspect_continue_main(spec("vscode-continue"), &yaml, true);
        assert_eq!(result["status"], "unknown");
        assert!(result["configured"].is_null());
    }
    #[test]
    fn jsonc_updates_preserve_comments_inside_unchanged_octocode_fields() {
        let text = "{\n \"mcpServers\": {\n \"octocode\": {\n // Keep launch explanation\n \"command\": \"npx\",\n \"args\": [\"octocode-mcp@latest\"],\n \"env\": {\n // Keep key explanation\n \"API\": \"old\",\n // Keep unrelated key\n \"OTHER\": \"keep\"\n }}}}\n";
        let root = parse_document(ConfigFormat::Json, text).expect("parse");
        let mut next = root["mcpServers"]["octocode"].clone();
        next["env"]["API"] = json!("new");
        let rendered = render_document(
            spec("cursor"),
            text,
            &["mcpServers".into(), "octocode".into()],
            Some(&next),
        )
        .expect("render");
        assert!(rendered.contains("// Keep launch explanation"));
        assert!(rendered.contains("// Keep key explanation"));
        assert!(rendered.contains("// Keep unrelated key"));
        assert_eq!(
            parse_document(ConfigFormat::Json, &rendered).expect("parse")["mcpServers"]["octocode"]
                ["env"]["API"],
            "new"
        );
    }
    #[test]
    fn toml_and_yaml_inline_comment_layouts_are_explicitly_read_only() {
        let toml = "[mcp_servers.octocode]\ncommand = \"npx\" # keep runner explanation\nargs = [\"octocode-mcp@latest\"]\n";
        let error = render_toml(
            toml,
            &["mcp_servers".into(), "octocode".into()],
            Some(&default_entry(spec("codex"), "npx")),
        )
        .expect_err("read only");
        assert!(error.contains("Inline comments"));
        let yaml = "extensions:\n  octocode:\n    cmd: npx # keep runner explanation\n    args: [octocode-mcp@latest]\n    type: stdio\n";
        let error = render_yaml(
            yaml,
            &["extensions".into(), "octocode".into()],
            Some(&default_entry(spec("goose"), "npx")),
        )
        .expect_err("read only");
        assert!(error.contains("Inline comments"));
        assert!(!has_inline_comment("token = \"abc#def\"", false));
    }
}
