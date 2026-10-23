//! Sanitized discovery and revision-checked edits of Octocode entries only.
mod continue_main;
mod document;
mod update;
use super::config::ManageError;
use super::mcp_clients::{self, CLIENTS, ClientSpec, ConfigFormat, EnableFlag, EntryShape};
use continue_main::continue_main_rows;
pub(super) use document::render_document;
use octocode_native::config::{config_revision, read_private_config};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use update::update;

/// One client's config file at one scope, and the keys of the Octocode
/// entry inside it.
pub(super) struct Target {
    pub(super) client: &'static ClientSpec,
    pub(super) scope: String,
    pub(super) path: PathBuf,
    pub(super) keys: Vec<String>,
}

/// The config `client` reads at `scope`: `home` (user config), `workspace`
/// (the project file under `cwd`) or `local` (Claude Code's per-project
/// section of its user config).
pub(super) fn target(client: &str, scope: &str, cwd: &Path) -> Result<Target, String> {
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
    let command = server.get("command");
    let command = if client.shape == EntryShape::Opencode {
        command
            .and_then(Value::as_array)
            .and_then(|args| args.first())
    } else {
        command
    };
    let method = command
        .and_then(Value::as_str)
        .and_then(Method::of_command)
        .map(Method::name);
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

pub(super) fn inspect(target: &Target) -> Value {
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

/// The package runner an installed server starts with.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub(super) enum Method {
    #[default]
    Npx,
    Bunx,
    Pnpm,
}

impl Method {
    const ALL: [Self; 3] = [Self::Npx, Self::Bunx, Self::Pnpm];

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Npx => "npx",
            Self::Bunx => "bunx",
            Self::Pnpm => "pnpm",
        }
    }

    pub(super) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.name() == name)
    }

    /// The runner a configured command starts: its file name, with the
    /// Windows shim suffix (`npx.cmd`, `bunx.exe`, `pnpm.cmd`) accepted.
    pub(super) fn of_command(command: &str) -> Option<Self> {
        let runner = Path::new(command).file_name()?.to_str()?;
        let runner = runner
            .strip_suffix(".cmd")
            .or_else(|| runner.strip_suffix(".exe"))
            .unwrap_or(runner);
        Self::from_name(runner)
    }

    /// The command and arguments that run the published server.
    const fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Npx => ("npx", &["-y", "octocode-mcp@latest"]),
            Self::Bunx => ("bunx", &["octocode-mcp@latest"]),
            Self::Pnpm => ("pnpm", &["dlx", "octocode-mcp@latest"]),
        }
    }
}

/// Format-independent description of the octocode MCP server entry.
pub(super) struct ServerSpec {
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub env: BTreeMap<String, String>,
}

impl ServerSpec {
    /// The server `method` runs, with no env.
    pub(super) fn new(method: Method) -> Self {
        let (command, args) = method.command();
        Self {
            command,
            args,
            env: BTreeMap::new(),
        }
    }
}

/// The octocode entry in `shape`'s format; a non-empty env goes under the
/// shape's env key.
pub(super) fn server_entry(shape: EntryShape, spec: &ServerSpec) -> Value {
    let mut server = match shape {
        EntryShape::Opencode => {
            let command: Vec<&str> = std::iter::once(spec.command)
                .chain(spec.args.iter().copied())
                .collect();
            json!({"type": "local", "command": command})
        }
        EntryShape::Untyped => json!({"command": spec.command, "args": spec.args}),
        EntryShape::Stdio => json!({"command": spec.command, "type": "stdio", "args": spec.args}),
    };
    if !spec.env.is_empty() {
        server[shape.env_key()] = json!(spec.env);
    }
    server
}

pub(super) fn default_entry(client: &ClientSpec, method: Method) -> Value {
    server_entry(client.shape, &ServerSpec::new(method))
}

#[cfg(test)]
mod tests;
