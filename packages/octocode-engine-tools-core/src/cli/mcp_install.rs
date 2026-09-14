use clap::Args;
use serde_json::{Map, Value, json};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// VS Code `MCP_COMMAND` / `MCP_ARGS` (`packages/octocode-vscode/src/mcpConfig.ts`).
const MCP_COMMAND: &str = "npx";
const MCP_TYPE: &str = "stdio";
const MCP_ARGS: &[&str] = &["-y", "octocode-mcp@latest"];
const SERVER_KEY: &str = "octocode";
const SERVERS_KEY: &str = "mcpServers";
const TOML_YAML_MESSAGE: &str = "use Node `octocode install --ide …` for TOML/YAML clients";

const JSON_IDE_IDS: &[&str] = &[
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

const TOML_YAML_IDE_IDS: &[&str] = &["codex", "goose"];

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Target JSON IDE id.
    #[arg(long)]
    pub ide: Option<String>,
    /// Overwrite an existing mcpServers.octocode entry.
    #[arg(long)]
    pub force: bool,
    /// Print the JSON that would be written; do not write.
    #[arg(long)]
    pub dry_run: bool,
    /// Preview existing config and merged JSON; do not write.
    #[arg(long)]
    pub check: bool,
    /// Print supported JSON IDE ids.
    #[arg(long)]
    pub list: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
    /// Set ENABLE_LOCAL on the installed server (`true` or `false`).
    #[arg(long, value_name = "true|false", value_parser = ["true", "false"])]
    pub enable_local: Option<String>,
    /// Copy ENABLE_LOCAL and GITHUB_TOKEN from the process env when already set.
    #[arg(long)]
    pub pass_env: bool,
}

struct FsCtx {
    home: PathBuf,
    app_data: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
}

struct CommandResult {
    code: u8,
    json: Value,
}

pub fn run(args: InstallArgs) -> u8 {
    let result = execute(&args, &FsCtx::from_env(), &process_var);
    emit(&args, &result);
    result.code
}

fn process_var(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.is_empty())
}

impl FsCtx {
    fn from_env() -> Self {
        Self {
            home: env::var_os("HOME")
                .or_else(|| env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from(".")),
            app_data: env::var_os("APPDATA").map(PathBuf::from),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        }
    }

    fn windows_app_data(&self) -> PathBuf {
        self.app_data
            .clone()
            .unwrap_or_else(|| self.home.join("AppData").join("Roaming"))
    }

    fn app_support_dir(&self) -> PathBuf {
        if cfg!(windows) {
            self.windows_app_data()
        } else if cfg!(target_os = "macos") {
            self.home.join("Library").join("Application Support")
        } else {
            self.xdg_config_home
                .clone()
                .unwrap_or_else(|| self.home.join(".config"))
        }
    }
}

fn execute(args: &InstallArgs, ctx: &FsCtx, env: &dyn Fn(&str) -> Option<String>) -> CommandResult {
    if args.list {
        return CommandResult {
            code: 0,
            json: json!({ "supported": JSON_IDE_IDS }),
        };
    }

    let Some(ide) = args
        .ide
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return usage_missing_ide();
    };

    if TOML_YAML_IDE_IDS.contains(&ide.as_str()) {
        return CommandResult {
            code: 2,
            json: json!({
                "success": false,
                "ide": ide,
                "error": TOML_YAML_MESSAGE,
            }),
        };
    }

    if !JSON_IDE_IDS.contains(&ide.as_str()) {
        let supported = JSON_IDE_IDS.join(", ");
        return CommandResult {
            code: 2,
            json: json!({
                "success": false,
                "ide": ide,
                "configPath": Value::Null,
                "error": format!("Invalid IDE: {ide}. Supported: {supported}"),
            }),
        };
    }

    let config_path = config_path(&ide, ctx);
    let existing = match read_config(&config_path) {
        Ok(value) => value,
        Err(error) => {
            return CommandResult {
                code: 1,
                json: json!({
                    "success": false,
                    "configPath": config_path,
                    "error": error,
                }),
            };
        }
    };

    if is_octocode_configured(&existing) && !args.force {
        return CommandResult {
            code: 1,
            json: json!({
                "success": false,
                "alreadyInstalled": true,
                "configPath": config_path,
            }),
        };
    }

    let server = octocode_server(args, env);
    let merged = merge_octocode(existing, server);

    if args.dry_run || args.check {
        let mut payload = json!({
            "success": true,
            "configPath": config_path,
            "config": merged,
        });
        if args.dry_run {
            payload["dryRun"] = json!(true);
        }
        if args.check {
            payload["check"] = json!(true);
        }
        return CommandResult {
            code: 0,
            json: payload,
        };
    }

    match write_mcp_config(&config_path, &merged) {
        Ok(backup_path) => {
            let mut payload = json!({
                "success": true,
                "configPath": config_path,
            });
            if let Some(backup_path) = backup_path {
                payload["backupPath"] = json!(backup_path);
            }
            CommandResult {
                code: 0,
                json: payload,
            }
        }
        Err(error) => CommandResult {
            code: 1,
            json: json!({
                "success": false,
                "configPath": config_path,
                "error": error.to_string(),
            }),
        },
    }
}

fn usage_missing_ide() -> CommandResult {
    let supported = JSON_IDE_IDS.join(", ");
    CommandResult {
        code: 2,
        json: json!({
            "success": false,
            "ide": Value::Null,
            "configPath": Value::Null,
            "error": format!("Missing required option: --ide. Supported: {supported}"),
        }),
    }
}

fn emit(args: &InstallArgs, result: &CommandResult) {
    if args.json {
        match serde_json::to_string(&result.json) {
            Ok(text) => println!("{text}"),
            Err(error) => eprintln!("{error}"),
        }
        return;
    }

    if let Some(ids) = result.json.get("supported").and_then(Value::as_array) {
        println!("Supported IDE ids for --ide:");
        for id in ids {
            if let Some(id) = id.as_str() {
                println!("  {id}");
            }
        }
        return;
    }

    let error = result.json.get("error").and_then(Value::as_str);
    if result.json.get("alreadyInstalled").and_then(Value::as_bool) == Some(true) {
        eprintln!("Octocode is already configured. Use --force to overwrite.");
        return;
    }
    if let Some(error) = error {
        eprintln!("{error}");
        return;
    }

    let path = result
        .json
        .get("configPath")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if args.dry_run || args.check {
        println!("{path}");
        if let Some(config) = result.json.get("config") {
            match serde_json::to_string_pretty(config) {
                Ok(text) => println!("{text}"),
                Err(error) => eprintln!("{error}"),
            }
        }
        return;
    }

    if result.json.get("success").and_then(Value::as_bool) == Some(true) {
        println!("Installed octocode-mcp");
        println!("Config: {path}");
        if let Some(backup) = result.json.get("backupPath").and_then(Value::as_str) {
            println!("Backup: {backup}");
        }
    }
}

fn octocode_server(args: &InstallArgs, env: &dyn Fn(&str) -> Option<String>) -> Value {
    let mut server = Map::new();
    server.insert("command".into(), json!(MCP_COMMAND));
    server.insert("type".into(), json!(MCP_TYPE));
    server.insert("args".into(), json!(MCP_ARGS));

    let mut env_map = Map::new();
    if args.pass_env {
        if let Some(value) = env("ENABLE_LOCAL") {
            env_map.insert("ENABLE_LOCAL".into(), json!(value));
        }
        if let Some(value) = env("GITHUB_TOKEN") {
            env_map.insert("GITHUB_TOKEN".into(), json!(value));
        }
    }
    if let Some(enable_local) = args.enable_local.as_deref() {
        env_map.insert("ENABLE_LOCAL".into(), json!(enable_local));
    }
    if !env_map.is_empty() {
        server.insert("env".into(), Value::Object(env_map));
    }
    Value::Object(server)
}

fn merge_octocode(existing: Value, server: Value) -> Value {
    let mut config = match existing {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let mut servers = match config.get(SERVERS_KEY) {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    servers.insert(SERVER_KEY.into(), server);
    config.insert(SERVERS_KEY.into(), Value::Object(servers));
    Value::Object(config)
}

fn is_octocode_configured(config: &Value) -> bool {
    match config.get(SERVERS_KEY) {
        Some(Value::Object(servers)) => servers
            .get(SERVER_KEY)
            .is_some_and(|value| !value.is_null()),
        _ => false,
    }
}

fn read_config(path: &Path) -> Result<Value, String> {
    if !path.is_file() {
        return Ok(json!({ SERVERS_KEY: {} }));
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(json!({ SERVERS_KEY: {} }));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid JSON at {}: {error}", path.display()))?;
    if !value.is_object() {
        return Err(format!("invalid JSON object at {}", path.display()));
    }
    if let Some(servers) = value.get(SERVERS_KEY)
        && !servers.is_object()
        && !servers.is_null()
    {
        return Err(format!(
            "invalid {SERVERS_KEY} object at {}",
            path.display()
        ));
    }
    Ok(value)
}

fn write_mcp_config(path: &Path, config: &Value) -> io::Result<Option<PathBuf>> {
    let backup_path = if path.is_file() {
        Some(backup_file(path)?)
    } else {
        None
    };
    if let Some(parent) = path.parent() {
        mkdir_private(parent)?;
    }
    let encoded = serde_json::to_string_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    atomic_write_private(path, format!("{encoded}\n").as_bytes())?;
    Ok(backup_path)
}

fn backup_file(path: &Path) -> io::Result<PathBuf> {
    let stamp = backup_stamp();
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".backup-");
    backup.push(&stamp);
    let backup = PathBuf::from(backup);
    fs::copy(path, &backup)?;
    Ok(backup)
}

fn backup_stamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{:03}", duration.as_secs(), duration.subsec_millis())
}

fn mkdir_private(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

fn atomic_write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_path(path);
    let result = write_private_create_new(&tmp, bytes).and_then(|()| replace_file(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(format!(".{nanos}.tmp"));
    PathBuf::from(tmp)
}

fn replace_file(tmp: &Path, dest: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        if dest.exists() {
            fs::remove_file(dest)?;
        }
    }
    fs::rename(tmp, dest)
}

#[cfg(unix)]
fn write_private_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)
}

fn config_path(ide: &str, ctx: &FsCtx) -> PathBuf {
    let home = ctx.home.as_path();
    let app_support = ctx.app_support_dir();
    let app_data = ctx.windows_app_data();
    let vscode_storage = app_support.join("Code").join("User").join("globalStorage");
    match ide {
        "cursor" if cfg!(windows) => app_data.join("Cursor").join("mcp.json"),
        "cursor" => home.join(".cursor").join("mcp.json"),
        "claude-desktop" if cfg!(windows) => {
            app_data.join("Claude").join("claude_desktop_config.json")
        }
        "claude-desktop" if cfg!(target_os = "macos") => app_support
            .join("Claude")
            .join("claude_desktop_config.json"),
        "claude-desktop" => app_support
            .join("claude")
            .join("claude_desktop_config.json"),
        "claude-code" => home.join(".claude.json"),
        "windsurf" => home
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
        "trae" if cfg!(windows) => app_data.join("Trae").join("mcp.json"),
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
        "zed" if cfg!(windows) => app_data.join("Zed").join("settings.json"),
        "zed" if cfg!(target_os = "macos") => {
            home.join(".config").join("zed").join("settings.json")
        }
        "zed" => app_support.join("zed").join("settings.json"),
        "opencode" if cfg!(windows) => app_data.join("opencode").join("config.json"),
        "opencode" => app_support.join("opencode").join("config.json"),
        "gemini-cli" => home.join(".gemini").join("settings.json"),
        "kiro" if cfg!(windows) => app_data.join("Kiro").join("mcp.json"),
        "kiro" => home.join(".kiro").join("mcp.json"),
        _ => home.join(".cursor").join("mcp.json"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    fn temp_ctx() -> (tempfile::TempDir, FsCtx) {
        let dir = tempfile::TempDir::new().expect("temp HOME");
        let home = dir.path().to_path_buf();
        let ctx = FsCtx {
            app_data: Some(home.join("AppData").join("Roaming")),
            xdg_config_home: Some(home.join("xdg-config")),
            home,
        };
        (dir, ctx)
    }

    fn install_args(ide: &str) -> InstallArgs {
        InstallArgs {
            ide: Some(ide.into()),
            force: false,
            dry_run: false,
            check: false,
            list: false,
            json: true,
            enable_local: None,
            pass_env: false,
        }
    }

    fn no_env(_key: &str) -> Option<String> {
        None
    }

    fn written_server(path: &Path) -> Value {
        let text = fs::read_to_string(path).expect("read installed JSON");
        let parsed: Value = serde_json::from_str(&text).expect("parse installed JSON");
        parsed[SERVERS_KEY][SERVER_KEY].clone()
    }

    fn assert_rejects_octo_mcp(server: &Value) {
        assert_eq!(server.get("command").and_then(Value::as_str), Some("npx"));
        assert_ne!(server.get("command").and_then(Value::as_str), Some("octo"));
        assert_eq!(server.get("type").and_then(Value::as_str), Some("stdio"));
        let args = server
            .get("args")
            .and_then(Value::as_array)
            .expect("args array");
        assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
        assert!(
            args.iter()
                .all(|arg| arg.as_str() != Some("mcp") && arg.as_str() != Some("octo")),
            "written args must not contain command octo or arg mcp: {args:?}"
        );
    }

    #[test]
    fn writes_npx_y_octocode_mcp_latest() {
        let (_dir, ctx) = temp_ctx();
        let result = execute(&install_args("cursor"), &ctx, &no_env);
        assert_eq!(result.code, 0, "{:?}", result.json);
        let path = config_path("cursor", &ctx);
        let server = written_server(&path);
        assert_rejects_octo_mcp(&server);
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"][0], "-y");
        assert_eq!(server["args"][1], "octocode-mcp@latest");
    }

    #[test]
    fn rejects_octo_command_and_mcp_arg() {
        let server = json!({
            "command": "npx",
            "type": "stdio",
            "args": ["-y", "octocode-mcp@latest"],
        });
        assert_rejects_octo_mcp(&server);
        let forbidden = json!({"command": "octo", "args": ["mcp"]});
        assert_ne!(forbidden["command"], "npx");
        assert!(
            forbidden["args"]
                .as_array()
                .expect("args")
                .iter()
                .any(|arg| arg.as_str() == Some("mcp"))
        );
    }

    #[test]
    fn already_installed_without_force_exits_1() {
        let (_dir, ctx) = temp_ctx();
        let path = config_path("cursor", &ctx);
        fs::create_dir_all(path.parent().expect("parent")).expect("parent");
        fs::write(&path, r#"{"mcpServers":{"octocode":{"command":"npx"}}}"#).expect("seed");
        let original = fs::read_to_string(&path).expect("original");
        let result = execute(&install_args("cursor"), &ctx, &no_env);
        assert_eq!(result.code, 1);
        assert_eq!(result.json["alreadyInstalled"], true);
        assert_eq!(result.json["success"], false);
        assert_eq!(fs::read_to_string(&path).expect("unchanged"), original);
    }

    #[test]
    fn force_overwrites_and_keeps_other_servers() {
        let (_dir, ctx) = temp_ctx();
        let path = config_path("cursor", &ctx);
        fs::create_dir_all(path.parent().expect("parent")).expect("parent");
        fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"node"},"octocode":{"command":"old"}}}"#,
        )
        .expect("seed");
        let mut args = install_args("cursor");
        args.force = true;
        let result = execute(&args, &ctx, &no_env);
        assert_eq!(result.code, 0, "{:?}", result.json);
        assert!(result.json.get("backupPath").is_some());
        let text = fs::read_to_string(&path).expect("read");
        let parsed: Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed["mcpServers"]["other"]["command"], "node");
        assert_rejects_octo_mcp(&parsed["mcpServers"]["octocode"]);
        let backup = result.json["backupPath"].as_str().expect("backup path");
        let backup_text = fs::read_to_string(backup).expect("backup");
        assert!(backup_text.contains("\"old\""));
    }

    #[test]
    fn dry_run_leaves_file_absent() {
        let (_dir, ctx) = temp_ctx();
        let mut args = install_args("cursor");
        args.dry_run = true;
        let result = execute(&args, &ctx, &no_env);
        assert_eq!(result.code, 0);
        assert_eq!(result.json["dryRun"], true);
        let path = config_path("cursor", &ctx);
        assert!(!path.exists());
        assert!(!path.parent().expect("parent").exists());
        assert_rejects_octo_mcp(&result.json["config"]["mcpServers"]["octocode"]);
    }

    #[test]
    fn check_does_not_write() {
        let (_dir, ctx) = temp_ctx();
        let mut args = install_args("windsurf");
        args.check = true;
        let result = execute(&args, &ctx, &no_env);
        assert_eq!(result.code, 0);
        assert_eq!(result.json["check"], true);
        assert!(!config_path("windsurf", &ctx).exists());
    }

    #[test]
    fn missing_ide_is_usage_exit_2() {
        let (_dir, ctx) = temp_ctx();
        let mut args = install_args("cursor");
        args.ide = None;
        let result = execute(&args, &ctx, &no_env);
        assert_eq!(result.code, 2);
        assert!(
            result.json["error"]
                .as_str()
                .expect("error")
                .contains("Missing required option: --ide")
        );
    }

    #[test]
    fn unknown_ide_is_usage_exit_2() {
        let (_dir, ctx) = temp_ctx();
        let result = execute(&install_args("notepad"), &ctx, &no_env);
        assert_eq!(result.code, 2);
        assert!(
            result.json["error"]
                .as_str()
                .expect("error")
                .contains("Invalid IDE: notepad")
        );
    }

    #[test]
    fn codex_is_rejected_as_toml() {
        let (_dir, ctx) = temp_ctx();
        let result = execute(&install_args("codex"), &ctx, &no_env);
        assert_eq!(result.code, 2);
        assert_eq!(result.json["error"], TOML_YAML_MESSAGE);
        assert!(!ctx.home.join(".codex").exists());
    }

    #[test]
    fn goose_is_rejected_as_yaml() {
        let (_dir, ctx) = temp_ctx();
        let result = execute(&install_args("goose"), &ctx, &no_env);
        assert_eq!(result.code, 2);
        assert_eq!(result.json["error"], TOML_YAML_MESSAGE);
    }

    #[test]
    fn list_prints_json_ides_only() {
        let (_dir, ctx) = temp_ctx();
        let args = InstallArgs {
            ide: None,
            list: true,
            ..install_args("cursor")
        };
        let result = execute(&args, &ctx, &no_env);
        assert_eq!(result.code, 0);
        let ids = result.json["supported"]
            .as_array()
            .expect("supported")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert!(ids.contains(&"cursor"));
        assert!(!ids.contains(&"codex"));
        assert!(!ids.contains(&"goose"));
        assert!(!ids.contains(&"custom"));
    }

    #[test]
    fn enable_local_and_pass_env_only_copy_requested_keys() {
        let (_dir, ctx) = temp_ctx();
        let mut args = install_args("kiro");
        args.enable_local = Some("false".into());
        args.pass_env = true;
        let vars = BTreeMap::from([
            ("ENABLE_LOCAL".to_owned(), "1".to_owned()),
            ("GITHUB_TOKEN".to_owned(), "secret-token".to_owned()),
            ("OCTOCODE_TOKEN".to_owned(), "must-not-copy".to_owned()),
        ]);
        let result = execute(&args, &ctx, &|key| vars.get(key).cloned());
        assert_eq!(result.code, 0, "{:?}", result.json);
        let server = written_server(&config_path("kiro", &ctx));
        assert_rejects_octo_mcp(&server);
        assert_eq!(server["env"]["ENABLE_LOCAL"], "false");
        assert_eq!(server["env"]["GITHUB_TOKEN"], "secret-token");
        assert!(server["env"].get("OCTOCODE_TOKEN").is_none());
    }

    #[test]
    fn json_ide_paths_stay_under_temp_home() {
        let (_dir, ctx) = temp_ctx();
        for id in JSON_IDE_IDS {
            let path = config_path(id, &ctx);
            assert!(
                path.starts_with(&ctx.home),
                "{id} escaped temp HOME: {}",
                path.display()
            );
        }
        #[cfg(unix)]
        {
            assert_eq!(
                config_path("cursor", &ctx),
                ctx.home.join(".cursor").join("mcp.json")
            );
            assert_eq!(
                config_path("claude-code", &ctx),
                ctx.home.join(".claude.json")
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn parent_dir_is_0o700_and_file_is_0o600() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, ctx) = temp_ctx();
        let result = execute(&install_args("cursor"), &ctx, &no_env);
        assert_eq!(result.code, 0, "{:?}", result.json);
        let path = config_path("cursor", &ctx);
        let parent = path.parent().expect("parent");
        let dir_mode = fs::metadata(parent).expect("dir meta").permissions().mode() & 0o777;
        let file_mode = fs::metadata(&path).expect("file meta").permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }
}
