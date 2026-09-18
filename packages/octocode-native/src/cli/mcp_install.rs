//! MCP install for all supported clients: always `npx -y octocode-mcp@latest`,
//! never `octo mcp`. JSON clients write `mcpServers.octocode`; codex writes
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

/// Every client whose config is JSON (used by sync analysis and detection).
pub(crate) fn json_clients() -> Vec<&'static str> {
    ALL_IDES
        .iter()
        .copied()
        .filter(|ide| client_format(ide) == ConfigFormat::Json)
        .collect()
}

/// Format-independent description of the octocode MCP server entry.
struct ServerSpec {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

fn server_spec(args: &InstallArgs) -> ServerSpec {
    let mut env = BTreeMap::new();
    if let Some(enabled) = args.enable_local {
        env.insert(
            "ENABLE_LOCAL".to_owned(),
            if enabled { "true" } else { "false" }.to_owned(),
        );
    }
    if args.pass_env {
        for key in ["ENABLE_LOCAL", "GITHUB_TOKEN"] {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                env.insert(key.to_owned(), value);
            }
        }
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
        // Restore: strip .bak extension to get the original path
        let dest = bak.with_extension("");
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
    let servers = root
        .as_object_mut()
        .ok_or_else(|| "MCP config is not a JSON object".to_owned())?
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers is not an object".to_owned())?;
    let already = servers.contains_key("octocode");
    if already && !args.force && !args.dry_run && !args.check {
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
        return Ok(1);
    }
    let server = octocode_server(args);
    reject_octo_mcp(&server)?;
    servers.insert("octocode".into(), server);
    if args.dry_run || args.check {
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
    // Write a backup before overwriting if requested
    let backed_up = if args.backup && config_path.exists() {
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

/// Shared "already installed" response for the structured (TOML/YAML) writers.
fn already_installed(config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
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
    if args.dry_run || args.check {
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
fn render_codex_toml(existing: Option<&str>, args: &InstallArgs) -> Result<(String, bool), String> {
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
    let already = servers.contains_key("octocode");
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
    servers.insert("octocode".to_owned(), toml::Value::Table(entry));
    let rendered = toml::to_string_pretty(&root).map_err(|error| error.to_string())?;
    Ok((rendered, already))
}

fn install_toml(ide: &str, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    let existing = if config_path.exists() {
        Some(std::fs::read_to_string(config_path).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let (rendered, already) = render_codex_toml(existing.as_deref(), args)?;
    if already && !args.force && !args.dry_run && !args.check {
        return already_installed(config_path, args);
    }
    finalize_text(ide, config_path, &rendered, args)
}

/// Render the octocode entry for goose `config.yaml` (`extensions.octocode`).
fn render_goose_yaml(existing: Option<&str>, args: &InstallArgs) -> Result<(String, bool), String> {
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
    let already = extensions.contains_key(&octocode_key);
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
    extensions.insert(octocode_key, Yaml::Mapping(entry));
    let rendered = serde_yaml_ng::to_string(&root).map_err(|error| error.to_string())?;
    Ok((rendered, already))
}

fn install_yaml(ide: &str, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    let existing = if config_path.exists() {
        Some(std::fs::read_to_string(config_path).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let (rendered, already) = render_goose_yaml(existing.as_deref(), args)?;
    if already && !args.force && !args.dry_run && !args.check {
        return already_installed(config_path, args);
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
        "vscode-continue" => home.join(".continue").join("config.json"),
        "zed" => home.join(".config").join("zed").join("settings.json"),
        "opencode" => app_support.join("opencode").join("config.json"),
        "gemini-cli" => home.join(".gemini").join("settings.json"),
        "kiro" => home.join(".kiro").join("mcp.json"),
        "codex" => home.join(".codex").join("config.toml"),
        "goose" => app_support.join("goose").join("config.yaml"),
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
        return Ok(json!({ "mcpServers": {} }));
    }
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
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
        canonical_ide, client_format, config_path, octocode_server, reject_octo_mcp,
        render_codex_toml, render_goose_yaml,
    };
    use serde_json::json;

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
        // SAFETY: single-threaded test; sets HOME so path resolution is deterministic.
        unsafe { std::env::set_var("HOME", "/tmp/octo-test-home") };
        assert!(
            config_path("codex")
                .expect("codex config path")
                .ends_with(".codex/config.toml")
        );
        assert!(
            config_path("goose")
                .expect("goose config path")
                .ends_with("goose/config.yaml")
        );
    }

    #[test]
    fn codex_toml_renders_mcp_servers_table_and_merges() {
        let (rendered, already) = render_codex_toml(None, &default_args()).expect("render");
        assert!(!already);
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
        assert!(!already_present);
        let (_again, now_present) = render_codex_toml(Some(&merged), &default_args()).expect("re");
        assert!(now_present, "detects existing octocode entry");
    }

    #[test]
    fn goose_yaml_renders_extensions_and_merges() {
        let (rendered, already) = render_goose_yaml(None, &default_args()).expect("render");
        assert!(!already);
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
        assert!(now_present, "detects existing octocode extension");
    }
}
