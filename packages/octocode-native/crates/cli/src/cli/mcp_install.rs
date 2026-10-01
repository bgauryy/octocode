//! MCP install for all supported clients: always `npx -y octocode-mcp@latest`,
//! never `octo mcp`. JSON clients use their native server map; codex writes
//! `[mcp_servers.octocode]` TOML; goose writes `extensions.octocode` YAML.
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every installable client, including the non-JSON ones.
const ALL_IDES: [&str; 15] = [
    "cursor",
    "claude-desktop",
    "claude-code",
    "windsurf",
    "trae",
    "antigravity",
    "vscode-cline",
    "vscode-roo",
    "vscode-continue",
    "zed",
    "opencode",
    "gemini-cli",
    "kiro",
    "codex",
    "goose",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ConfigFormat {
    Json,
    Toml,
    Yaml,
}

fn client_format(ide: &str) -> ConfigFormat {
    match ide {
        "codex" => ConfigFormat::Toml,
        "goose" => ConfigFormat::Yaml,
        _ => ConfigFormat::Json,
    }
}

/// Format-independent description of the octocode MCP server entry.
struct ServerSpec {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

/// Primary env name bound to the `local.enabled` config key (`ENABLE_LOCAL`).
fn local_enabled_env() -> Option<&'static str> {
    octocode_native::config::CONFIG_FIELDS
        .iter()
        .find(|field| field.path == "local.enabled")
        .and_then(|field| field.env.first())
        .map(|binding| binding.name)
}

/// The GitHub token env var `--pass-env` forwards; one of the config
/// contract's `ENV_TOKEN_VARS` (a unit test pins that).
const PASS_ENV_TOKEN: &str = "GITHUB_TOKEN";

fn server_spec(args: &InstallArgs) -> ServerSpec {
    let mut env = BTreeMap::new();
    if args.pass_env {
        for key in local_enabled_env().into_iter().chain([PASS_ENV_TOKEN]) {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                env.insert(key.to_owned(), value);
            }
        }
    }
    if let (Some(enabled), Some(key)) = (args.enable_local, local_enabled_env()) {
        env.insert(
            key.to_owned(),
            if enabled { "true" } else { "false" }.to_owned(),
        );
    }
    let (cmd, cmd_args): (&str, &[&str]) = match args.method.as_deref().unwrap_or("npx") {
        "bunx" => ("bunx", &["octocode-mcp@latest"]),
        "pnpm" => ("pnpm", &["dlx", "octocode-mcp@latest"]),
        _ => ("npx", &["-y", "octocode-mcp@latest"]),
    };
    ServerSpec {
        command: cmd.to_owned(),
        args: cmd_args.iter().map(|value| (*value).to_owned()).collect(),
        env,
    }
}

pub struct InstallArgs {
    pub ide: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub check: bool,
    pub list: bool,
    pub json: bool,
    pub enable_local: Option<bool>,
    pub pass_env: bool,
    /// Installation runner: "npx" (default), "bunx", or "pnpm".
    pub method: Option<String>,
    /// Write a .bak backup before overwriting the config file.
    pub backup: bool,
    /// Restore config from this .bak file path and exit.
    pub rollback: Option<String>,
}

fn canonical_ide(ide: &str) -> &str {
    match ide {
        "claude" => "claude-desktop",
        "vscode" => "vscode-cline",
        other => other,
    }
}

pub fn run(args: InstallArgs) -> u8 {
    // Rollback mode: restore a backup written by a previous --backup install
    if let Some(ref bak_str) = args.rollback {
        let bak = PathBuf::from(bak_str);
        if !bak.exists() {
            eprintln!("rollback: backup file not found: {}", bak.display());
            return 1;
        }
        if bak.extension().and_then(|value| value.to_str()) != Some("bak") {
            eprintln!("rollback: expected a .bak backup file");
            return 2;
        }
        let dest = bak.with_extension("");
        if args.dry_run {
            if args.json {
                println!(
                    "{}",
                    json!({"success": true, "dryRun": true, "restored": dest, "from": bak})
                );
            } else {
                println!("Would restore {} from {}", dest.display(), bak.display());
            }
            return 0;
        }
        return match std::fs::copy(&bak, &dest) {
            Ok(_) => {
                if args.json {
                    println!(
                        "{}",
                        json!({"success": true, "restored": dest, "from": bak})
                    );
                } else {
                    println!("Restored {} from {}", dest.display(), bak.display());
                }
                0
            }
            Err(e) => {
                eprintln!("rollback: {e}");
                1
            }
        };
    }
    if args.list {
        if args.json {
            println!("{}", json!({ "ides": ALL_IDES }));
        } else {
            for ide in ALL_IDES {
                println!("{ide}");
            }
        }
        return 0;
    }
    let Some(requested_ide) = args.ide.as_deref().filter(|value| !value.is_empty()) else {
        eprintln!("Usage: octocode install --ide <id> [--force] [--dry-run] [--check] [--json]");
        return 2;
    };
    let ide = canonical_ide(requested_ide);
    let Some(config_path) = config_path(ide) else {
        eprintln!("Unknown --ide {requested_ide}");
        return 2;
    };
    let result = match client_format(ide) {
        ConfigFormat::Json => install(ide, &config_path, &args),
        ConfigFormat::Toml => install_toml(ide, &config_path, &args),
        ConfigFormat::Yaml => install_yaml(ide, &config_path, &args),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn install(ide: &str, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    let mut root = read_json(config_path)?;
    if args.check {
        return report_check(ide, config_path, &root, args);
    }
    let key = json_server_key(ide);
    let servers = root
        .as_object_mut()
        .ok_or_else(|| "MCP config is not a JSON object".to_owned())?
        .entry(key)
        .or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| format!("{key} is not an object"))?;
    let server = json_server(ide, args);
    let existing = Existing::of(servers.get("octocode"), &server);
    if existing != Existing::Absent && !args.force && !args.dry_run && !args.check {
        return already_installed(config_path, args, existing);
    }
    reject_octo_mcp(&octocode_server(args))?;
    servers.insert("octocode".into(), server);
    if args.dry_run {
        if args.json {
            println!(
                "{}",
                json!({
                    "success": true,
                    "dryRun": args.dry_run,
                    "check": args.check,
                    "ide": ide,
                    "configPath": config_path,
                    "config": root
                })
            );
        } else {
            println!("{}", config_path.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".into())
            );
        }
        return Ok(0);
    }
    // Rewriting reserializes the client's entire config file (for clients like
    // claude-code that is ~/.claude.json, holding unrelated app state), so back
    // up any existing target by default — not only when --backup is passed.
    let backed_up = if config_path.exists() {
        let bak = config_path.with_extension("json.bak");
        std::fs::copy(config_path, &bak).ok().map(|_| bak)
    } else {
        None
    };
    write_json(config_path, &root)?;
    if args.json {
        println!(
            "{}",
            json!({
                "success": true,
                "configPath": config_path,
                "backupPath": backed_up
            })
        );
    } else {
        if let Some(ref bak) = backed_up {
            println!("Backup written to {}", bak.display());
        }
        println!(
            "Installed octocode MCP for {ide} at {}",
            config_path.display()
        );
    }
    Ok(0)
}

fn json_server_key(ide: &str) -> &'static str {
    match ide {
        "zed" => "context_servers",
        "opencode" => "mcp",
        _ => "mcpServers",
    }
}

fn json_server(ide: &str, args: &InstallArgs) -> Value {
    if ide == "opencode" {
        let spec = server_spec(args);
        let mut command = vec![spec.command];
        command.extend(spec.args);
        let mut server = json!({"type": "local", "command": command});
        if !spec.env.is_empty() {
            server["environment"] = json!(spec.env);
        }
        server
    } else {
        let mut server = octocode_server(args);
        if ide == "zed"
            && let Some(object) = server.as_object_mut()
        {
            object.remove("type");
        }
        server
    }
}

fn valid_server(ide: &str, root: &Value) -> bool {
    let key = match ide {
        "codex" => "mcp_servers",
        "goose" => "extensions",
        _ => json_server_key(ide),
    };
    let Some(server) = root.get(key).and_then(|servers| servers.get("octocode")) else {
        return false;
    };
    if server.get("disabled").and_then(Value::as_bool) == Some(true)
        || server.get("enabled").and_then(Value::as_bool) == Some(false)
    {
        return false;
    }
    let (command, args) = if ide == "opencode" {
        if server.get("type").and_then(Value::as_str) != Some("local") {
            return false;
        }
        let Some(command) = server.get("command").and_then(Value::as_array) else {
            return false;
        };
        let Some((first, rest)) = command.split_first() else {
            return false;
        };
        (first.as_str(), rest)
    } else {
        let command_key = if ide == "goose" { "cmd" } else { "command" };
        if ide == "goose" && server.get("type").and_then(Value::as_str) != Some("stdio") {
            return false;
        }
        let Some(args) = server.get("args").and_then(Value::as_array) else {
            return false;
        };
        (
            server.get(command_key).and_then(Value::as_str),
            args.as_slice(),
        )
    };
    let Some(command) = command else {
        return false;
    };
    let runner = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command);
    matches!(
        runner,
        "npx" | "npx.cmd" | "bunx" | "bunx.exe" | "pnpm" | "pnpm.cmd"
    ) && args.iter().all(Value::is_string)
        && args.iter().any(|arg| {
            arg.as_str()
                .is_some_and(|value| value == "octocode-mcp" || value.starts_with("octocode-mcp@"))
        })
        && (runner != "pnpm" && runner != "pnpm.cmd"
            || args.first().and_then(Value::as_str) == Some("dlx"))
}

fn report_check(ide: &str, path: &Path, root: &Value, args: &InstallArgs) -> Result<u8, String> {
    let valid = valid_server(ide, root);
    if args.json {
        println!(
            "{}",
            json!({"success": valid, "check": true, "ide": ide, "configPath": path, "installed": valid})
        );
    } else if valid {
        println!("Octocode MCP is configured for {ide} at {}", path.display());
    } else {
        eprintln!(
            "No valid Octocode MCP entry for {ide} at {}",
            path.display()
        );
    }
    Ok(if valid { 0 } else { 1 })
}

fn octocode_server(args: &InstallArgs) -> Value {
    let spec = server_spec(args);
    let mut server = json!({
        "command": spec.command,
        "type": "stdio",
        "args": spec.args
    });
    if !spec.env.is_empty() {
        let mut env = Map::new();
        for (key, value) in spec.env {
            env.insert(key, json!(value));
        }
        server["env"] = Value::Object(env);
    }
    server
}

/// Whether the client config already carries an octocode entry, and whether it
/// matches the entry this install would write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    Absent,
    Same,
    Different,
}

impl Existing {
    fn of<T: PartialEq>(previous: Option<&T>, next: &T) -> Self {
        match previous {
            None => Self::Absent,
            Some(previous) if previous == next => Self::Same,
            Some(_) => Self::Different,
        }
    }
}

/// Shared "already installed" response. An identical entry is a successful
/// no-op (exit 0) so setup scripts can re-run install; a differing entry is
/// left untouched and needs `--force` (exit 1).
fn already_installed(
    config_path: &Path,
    args: &InstallArgs,
    existing: Existing,
) -> Result<u8, String> {
    if existing == Existing::Same {
        if args.json {
            println!(
                "{}",
                json!({
                    "success": true,
                    "alreadyInstalled": true,
                    "unchanged": true,
                    "configPath": config_path
                })
            );
        } else {
            eprintln!(
                "octocode is already installed in {} — unchanged",
                config_path.display()
            );
        }
        return Ok(0);
    }
    if args.json {
        println!(
            "{}",
            json!({
                "success": false,
                "alreadyInstalled": true,
                "configPath": config_path
            })
        );
    } else {
        eprintln!(
            "octocode is already installed in {} — pass --force to overwrite",
            config_path.display()
        );
    }
    Ok(1)
}

/// Shared dry-run / backup / write / report path for rendered text configs.
fn finalize_text(
    ide: &str,
    config_path: &Path,
    rendered: &str,
    args: &InstallArgs,
) -> Result<u8, String> {
    if args.dry_run {
        if args.json {
            println!(
                "{}",
                json!({
                    "success": true,
                    "dryRun": args.dry_run,
                    "check": args.check,
                    "ide": ide,
                    "configPath": config_path,
                    "config": rendered
                })
            );
        } else {
            println!("{}", config_path.display());
            println!("{rendered}");
        }
        return Ok(0);
    }
    let backed_up = if args.backup && config_path.exists() {
        let ext = config_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bak");
        let bak = config_path.with_extension(format!("{ext}.bak"));
        std::fs::copy(config_path, &bak).ok().map(|_| bak)
    } else {
        None
    };
    write_text(config_path, rendered)?;
    if args.json {
        println!(
            "{}",
            json!({
                "success": true,
                "configPath": config_path,
                "backupPath": backed_up
            })
        );
    } else {
        if let Some(ref bak) = backed_up {
            println!("Backup written to {}", bak.display());
        }
        println!(
            "Installed octocode MCP for {ide} at {}",
            config_path.display()
        );
    }
    Ok(0)
}

/// Render the octocode entry for codex `config.toml` (`[mcp_servers.octocode]`).
fn render_codex_toml(
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    let mut root: toml::Value = match existing {
        Some(text) if !text.trim().is_empty() => toml::from_str(text)
            .map_err(|error| format!("codex config.toml is not valid TOML: {error}"))?,
        _ => toml::Value::Table(toml::Table::new()),
    };
    let table = root
        .as_table_mut()
        .ok_or_else(|| "codex config is not a TOML table".to_owned())?;
    let servers = table
        .entry("mcp_servers".to_owned())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| "mcp_servers is not a TOML table".to_owned())?;
    let spec = server_spec(args);
    let mut entry = toml::Table::new();
    entry.insert("command".to_owned(), toml::Value::String(spec.command));
    entry.insert(
        "args".to_owned(),
        toml::Value::Array(spec.args.into_iter().map(toml::Value::String).collect()),
    );
    if !spec.env.is_empty() {
        let mut env_table = toml::Table::new();
        for (key, value) in spec.env {
            env_table.insert(key, toml::Value::String(value));
        }
        entry.insert("env".to_owned(), toml::Value::Table(env_table));
    }
    let entry = toml::Value::Table(entry);
    let already = Existing::of(servers.get("octocode"), &entry);
    servers.insert("octocode".to_owned(), entry);
    let rendered = toml::to_string_pretty(&root).map_err(|error| error.to_string())?;
    Ok((rendered, already))
}

fn install_toml(ide: &str, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    let existing = if config_path.exists() {
        Some(std::fs::read_to_string(config_path).map_err(|error| error.to_string())?)
    } else {
        None
    };
    if args.check {
        let root: toml::Value = toml::from_str(existing.as_deref().unwrap_or(""))
            .map_err(|error| format!("codex config.toml is not valid TOML: {error}"))?;
        let root = serde_json::to_value(root).map_err(|error| error.to_string())?;
        return report_check(ide, config_path, &root, args);
    }
    let (rendered, already) = render_codex_toml(existing.as_deref(), args)?;
    if already != Existing::Absent && !args.force && !args.dry_run && !args.check {
        return already_installed(config_path, args, already);
    }
    finalize_text(ide, config_path, &rendered, args)
}

/// Render the octocode entry for goose `config.yaml` (`extensions.octocode`).
fn render_goose_yaml(
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    use serde_yaml_ng::{Mapping, Value as Yaml};
    let mut root: Yaml = match existing {
        Some(text) if !text.trim().is_empty() => serde_yaml_ng::from_str(text)
            .map_err(|error| format!("goose config.yaml is not valid YAML: {error}"))?,
        _ => Yaml::Mapping(Mapping::new()),
    };
    let map = root
        .as_mapping_mut()
        .ok_or_else(|| "goose config is not a YAML mapping".to_owned())?;
    let ext_key = Yaml::String("extensions".to_owned());
    if !map.contains_key(&ext_key) {
        map.insert(ext_key.clone(), Yaml::Mapping(Mapping::new()));
    }
    let extensions = map
        .get_mut(&ext_key)
        .and_then(Yaml::as_mapping_mut)
        .ok_or_else(|| "extensions is not a YAML mapping".to_owned())?;
    let octocode_key = Yaml::String("octocode".to_owned());
    let spec = server_spec(args);
    let mut entry = Mapping::new();
    entry.insert(Yaml::String("name".into()), Yaml::String("octocode".into()));
    entry.insert(Yaml::String("type".into()), Yaml::String("stdio".into()));
    entry.insert(Yaml::String("cmd".into()), Yaml::String(spec.command));
    entry.insert(
        Yaml::String("args".into()),
        Yaml::Sequence(spec.args.into_iter().map(Yaml::String).collect()),
    );
    entry.insert(Yaml::String("enabled".into()), Yaml::Bool(true));
    let mut envs = Mapping::new();
    for (key, value) in spec.env {
        envs.insert(Yaml::String(key), Yaml::String(value));
    }
    entry.insert(Yaml::String("envs".into()), Yaml::Mapping(envs));
    let entry = Yaml::Mapping(entry);
    let already = Existing::of(extensions.get(&octocode_key), &entry);
    extensions.insert(octocode_key, entry);
    let rendered = serde_yaml_ng::to_string(&root).map_err(|error| error.to_string())?;
    Ok((rendered, already))
}

fn install_yaml(ide: &str, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    let existing = if config_path.exists() {
        Some(std::fs::read_to_string(config_path).map_err(|error| error.to_string())?)
    } else {
        None
    };
    if args.check {
        let root: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(existing.as_deref().unwrap_or("{}"))
                .map_err(|error| format!("goose config.yaml is not valid YAML: {error}"))?;
        let root = serde_json::to_value(root).map_err(|error| error.to_string())?;
        return report_check(ide, config_path, &root, args);
    }
    let (rendered, already) = render_goose_yaml(existing.as_deref(), args)?;
    if already != Existing::Absent && !args.force && !args.dry_run && !args.check {
        return already_installed(config_path, args, already);
    }
    finalize_text(ide, config_path, &rendered, args)
}

fn write_text(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())
}

fn reject_octo_mcp(server: &Value) -> Result<(), String> {
    let command = server.get("command").and_then(Value::as_str).unwrap_or("");
    let args = server
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if command == "octo" || command == "octocode" {
        return Err("refusing to write a native octo/octocode MCP command".into());
    }
    if args.iter().any(|value| value.as_str() == Some("mcp")) {
        return Err("refusing to write args containing mcp".into());
    }
    Ok(())
}

pub fn config_path(ide: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    let app_support = app_support_dir(&home);
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let vscode_storage = app_support.join("Code").join("User").join("globalStorage");
    Some(match ide {
        "cursor" => home.join(".cursor").join("mcp.json"),
        "claude-desktop" => app_support
            .join("Claude")
            .join("claude_desktop_config.json"),
        "claude-code" => home.join(".claude.json"),
        "windsurf" => home
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
        "trae" => app_support.join("Trae").join("mcp.json"),
        "antigravity" => home
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json"),
        "vscode-cline" => vscode_storage
            .join("saoudrizwan.claude-dev")
            .join("settings")
            .join("cline_mcp_settings.json"),
        "vscode-roo" => vscode_storage
            .join("rooveterinaryinc.roo-cline")
            .join("settings")
            .join("mcp_settings.json"),
        "vscode-continue" => home
            .join(".continue")
            .join("mcpServers")
            .join("octocode.json"),
        "zed" => home.join(".config").join("zed").join("settings.json"),
        "opencode" => config_dir.join("opencode").join("opencode.json"),
        "gemini-cli" => home.join(".gemini").join("settings.json"),
        "kiro" => home.join(".kiro").join("settings").join("mcp.json"),
        "codex" => home.join(".codex").join("config.toml"),
        "goose" if cfg!(windows) => app_support
            .join("Block")
            .join("goose")
            .join("config")
            .join("config.yaml"),
        "goose" => config_dir.join("goose").join("config.yaml"),
        _ => return None,
    })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn app_support_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "cannot parse {} as JSON ({error}). Files with comments (JSONC) are not rewritten so your comments are kept; add the octocode server entry to it manually, or use --dry-run to preview the entry.",
            path.display()
        )
    })
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(&tmp, body).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        Existing, canonical_ide, client_format, config_path, octocode_server, reject_octo_mcp,
        render_codex_toml, render_goose_yaml,
    };
    use serde_json::json;

    #[test]
    fn unparseable_client_config_error_names_the_file_and_keeps_it_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let original = "{\n  // my theme\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(&path, original).expect("write");
        let error = super::read_json(&path).expect_err("comments are not plain JSON");
        assert!(error.contains(&path.display().to_string()), "{error}");
        assert!(error.contains("--dry-run"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
    }

    fn default_args() -> super::InstallArgs {
        super::InstallArgs {
            ide: Some("cursor".into()),
            force: false,
            dry_run: false,
            check: false,
            list: false,
            json: false,
            enable_local: None,
            pass_env: false,
            method: None,
            backup: false,
            rollback: None,
        }
    }

    #[test]
    fn passed_env_names_are_config_contract_names() {
        assert!(super::local_enabled_env().is_some());
        assert!(octocode_native::config::ENV_TOKEN_VARS.contains(&super::PASS_ENV_TOKEN));
        let spec = super::server_spec(&super::InstallArgs {
            enable_local: Some(false),
            ..default_args()
        });
        assert_eq!(
            spec.env.get(super::local_enabled_env().unwrap_or_default()),
            Some(&"false".to_owned())
        );
    }

    #[test]
    fn ide_aliases_resolve_to_supported_json_clients() {
        assert_eq!(canonical_ide("claude"), "claude-desktop");
        assert_eq!(canonical_ide("vscode"), "vscode-cline");
        assert_eq!(canonical_ide("cursor"), "cursor");
    }

    #[test]
    fn server_is_npx_latest_with_yes_flag() {
        let server = octocode_server(&default_args());
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
        reject_octo_mcp(&server).expect("allowed");
        assert_ne!(client_format("codex"), super::ConfigFormat::Json);
    }

    #[test]
    fn server_uses_bunx_when_specified() {
        let server = octocode_server(&super::InstallArgs {
            method: Some("bunx".into()),
            ..default_args()
        });
        assert_eq!(server["command"], "bunx");
        assert_eq!(server["args"], json!(["octocode-mcp@latest"]));
    }

    #[test]
    fn refuses_octo_mcp_command() {
        assert!(reject_octo_mcp(&json!({"command":"octo","args":["mcp"]})).is_err());
        assert!(reject_octo_mcp(&json!({"command":"npx","args":["mcp"]})).is_err());
    }

    #[test]
    fn client_formats_route_by_extension() {
        assert_eq!(client_format("cursor"), super::ConfigFormat::Json);
        assert_eq!(client_format("codex"), super::ConfigFormat::Toml);
        assert_eq!(client_format("goose"), super::ConfigFormat::Yaml);
    }

    #[test]
    fn codex_and_goose_have_config_paths() {
        assert!(
            config_path("codex")
                .expect("codex config path")
                .ends_with(".codex/config.toml")
        );
        assert!(
            config_path("goose")
                .expect("goose config path")
                .ends_with(if cfg!(windows) {
                    "Block/goose/config/config.yaml"
                } else {
                    "goose/config.yaml"
                })
        );
    }

    #[test]
    fn codex_toml_renders_mcp_servers_table_and_merges() {
        let (rendered, already) = render_codex_toml(None, &default_args()).expect("render");
        assert_eq!(already, Existing::Absent);
        // Valid TOML with the codex-native table shape.
        let parsed: toml::Value = toml::from_str(&rendered).expect("valid toml");
        let entry = &parsed["mcp_servers"]["octocode"];
        assert_eq!(entry["command"].as_str(), Some("npx"));
        assert_eq!(
            entry["args"].as_array().expect("args is array").len(),
            2,
            "npx -y octocode-mcp@latest"
        );

        // Merges into an existing config, preserving other servers and detecting reinstall.
        let existing = "[mcp_servers.other]\ncommand = \"foo\"\nargs = []\n";
        let (merged, already_present) =
            render_codex_toml(Some(existing), &default_args()).expect("merge");
        let parsed: toml::Value = toml::from_str(&merged).expect("valid toml");
        assert!(
            parsed["mcp_servers"].get("other").is_some(),
            "preserves other"
        );
        assert!(parsed["mcp_servers"].get("octocode").is_some());
        assert_eq!(already_present, Existing::Absent);
        let (_again, now_present) = render_codex_toml(Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode entry"
        );
        let edited = merged.replace("npx", "node");
        let (_, differs) = render_codex_toml(Some(&edited), &default_args()).expect("diff");
        assert_eq!(differs, Existing::Different, "detects a user-edited entry");
    }

    #[test]
    fn goose_yaml_renders_extensions_and_merges() {
        let (rendered, already) = render_goose_yaml(None, &default_args()).expect("render");
        assert_eq!(already, Existing::Absent);
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).expect("valid yaml");
        let entry = &parsed["extensions"]["octocode"];
        assert_eq!(entry["type"].as_str(), Some("stdio"));
        assert_eq!(entry["cmd"].as_str(), Some("npx"));
        assert_eq!(entry["enabled"].as_bool(), Some(true));

        let existing = "extensions:\n  other:\n    type: stdio\n    cmd: foo\n";
        let (merged, _) = render_goose_yaml(Some(existing), &default_args()).expect("merge");
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&merged).expect("valid yaml");
        assert!(
            parsed["extensions"].get("other").is_some(),
            "preserves other"
        );
        let (_again, now_present) = render_goose_yaml(Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode extension"
        );
    }

    #[test]
    fn reinstall_is_a_noop_when_identical_and_needs_force_when_edited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        assert_eq!(
            super::install("cursor", &path, &default_args()).expect("first"),
            0
        );
        let written = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            super::install("cursor", &path, &default_args()).expect("again"),
            0
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            written,
            "no rewrite"
        );
        std::fs::write(&path, written.replace("npx", "node")).expect("edit");
        assert_eq!(
            super::install("cursor", &path, &default_args()).expect("edited"),
            1
        );
    }
    #[test]
    fn client_specific_json_shapes_are_valid_and_preserve_other_settings() {
        for (ide, key) in [
            ("cursor", "mcpServers"),
            ("zed", "context_servers"),
            ("opencode", "mcp"),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("config.json");
            std::fs::write(&path, r#"{"theme":"dark","unrelated":{"keep":true}}"#)
                .expect("fixture");
            assert_eq!(
                super::install(ide, &path, &default_args()).expect("install"),
                0
            );
            let root: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
            assert_eq!(root["theme"], "dark");
            assert_eq!(root["unrelated"]["keep"], true);
            assert!(root[key].get("octocode").is_some());
            assert!(super::valid_server(ide, &root));
            if ide == "opencode" {
                assert_eq!(
                    root[key]["octocode"]["command"],
                    json!(["npx", "-y", "octocode-mcp@latest"])
                );
            }
            let written = std::fs::read_to_string(&path).expect("read");
            assert_eq!(
                super::install(ide, &path, &default_args()).expect("existing"),
                0,
                "identical reinstall is a no-op"
            );
            assert_eq!(std::fs::read_to_string(&path).expect("read"), written);
        }
    }

    #[test]
    fn checks_inspect_existing_entries_without_creating_or_repairing_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let check = super::InstallArgs {
            check: true,
            json: true,
            ..default_args()
        };
        for (ide, file) in [
            ("cursor", "c.json"),
            ("zed", "z.json"),
            ("opencode", "o.json"),
            ("codex", "c.toml"),
            ("goose", "g.yaml"),
        ] {
            let path = dir.path().join(file);
            let inspect = |args: &super::InstallArgs| match ide {
                "codex" => super::install_toml(ide, &path, args),
                "goose" => super::install_yaml(ide, &path, args),
                _ => super::install(ide, &path, args),
            };
            assert_eq!(inspect(&check).expect("missing"), 1, "{ide}");
            assert!(!path.exists());
            assert_eq!(inspect(&default_args()).expect("install"), 0);
            let before = std::fs::read(&path).expect("before");
            assert_eq!(inspect(&check).expect("valid"), 0);
            assert_eq!(std::fs::read(&path).expect("after"), before);
        }
        assert!(!super::valid_server(
            "cursor",
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["other-package"]}}})
        ));
        assert!(!super::valid_server(
            "cursor",
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["octocode-mcp@latest"],"disabled":true}}})
        ));
    }

    #[test]
    fn rollback_dry_run_preserves_destination_and_real_rollback_restores() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let backup = dir.path().join("config.json.bak");
        std::fs::write(&path, "current").expect("current");
        std::fs::write(&backup, "previous").expect("backup");
        let mut args = super::InstallArgs {
            rollback: Some(backup.to_string_lossy().into_owned()),
            dry_run: true,
            json: true,
            ..default_args()
        };
        assert_eq!(super::run(args), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "current");
        args = super::InstallArgs {
            rollback: Some(backup.to_string_lossy().into_owned()),
            ..default_args()
        };
        assert_eq!(super::run(args), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "previous");
    }

    #[test]
    fn client_paths_match_their_documented_configuration_files() {
        for (ide, suffix) in [
            ("kiro", ".kiro/settings/mcp.json"),
            ("vscode-continue", ".continue/mcpServers/octocode.json"),
            ("opencode", "opencode/opencode.json"),
        ] {
            assert!(config_path(ide).expect("path").ends_with(suffix), "{ide}");
        }
    }
}
