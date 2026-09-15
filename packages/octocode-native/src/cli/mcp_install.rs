//! JSON MCP install: always `npx -y octocode-mcp@latest`. Never `octo mcp`.
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const MCP_ARGS: [&str; 2] = ["-y", "octocode-mcp@latest"];
const JSON_IDES: [&str; 13] = [
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
];

pub struct InstallArgs {
    pub ide: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub check: bool,
    pub list: bool,
    pub json: bool,
    pub enable_local: Option<bool>,
    pub pass_env: bool,
}

pub fn run(args: InstallArgs) -> u8 {
    if args.list {
        if args.json {
            println!("{}", json!({ "ides": JSON_IDES }));
        } else {
            for ide in JSON_IDES {
                println!("{ide}");
            }
        }
        return 0;
    }
    let Some(ide) = args.ide.as_deref().filter(|value| !value.is_empty()) else {
        eprintln!("Usage: octocode install --ide <id> [--force] [--dry-run] [--check] [--json]");
        return 2;
    };
    if matches!(ide, "codex" | "goose") {
        eprintln!("use Node `octocode install --ide {ide}` for TOML/YAML clients");
        return 2;
    }
    let Some(config_path) = config_path(ide) else {
        eprintln!("Unknown --ide {ide}");
        return 2;
    };
    match install(ide, &config_path, &args) {
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
    write_json(config_path, &root)?;
    if args.json {
        println!(
            "{}",
            json!({
                "success": true,
                "configPath": config_path
            })
        );
    } else {
        println!(
            "Installed octocode MCP for {ide} at {}",
            config_path.display()
        );
    }
    Ok(0)
}

fn octocode_server(args: &InstallArgs) -> Value {
    let mut env = Map::new();
    if let Some(enabled) = args.enable_local {
        env.insert(
            "ENABLE_LOCAL".into(),
            json!(if enabled { "true" } else { "false" }),
        );
    }
    if args.pass_env {
        for key in ["ENABLE_LOCAL", "GITHUB_TOKEN"] {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                env.insert(key.into(), json!(value));
            }
        }
    }
    let mut server = json!({
        "command": "npx",
        "type": "stdio",
        "args": MCP_ARGS
    });
    if !env.is_empty() {
        server["env"] = Value::Object(env);
    }
    server
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

fn config_path(ide: &str) -> Option<PathBuf> {
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
    use super::{JSON_IDES, octocode_server, reject_octo_mcp};
    use serde_json::json;

    #[test]
    fn server_is_npx_latest_with_yes_flag() {
        let server = octocode_server(&super::InstallArgs {
            ide: Some("cursor".into()),
            force: false,
            dry_run: false,
            check: false,
            list: false,
            json: false,
            enable_local: None,
            pass_env: false,
        });
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
        reject_octo_mcp(&server).expect("allowed");
        assert!(!JSON_IDES.contains(&"codex"));
    }

    #[test]
    fn refuses_octo_mcp_command() {
        assert!(reject_octo_mcp(&json!({"command":"octo","args":["mcp"]})).is_err());
        assert!(reject_octo_mcp(&json!({"command":"npx","args":["mcp"]})).is_err());
    }
}
