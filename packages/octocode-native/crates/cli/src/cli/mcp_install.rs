//! MCP install for all supported clients: the server runs from npm through
//! `--method` (`npx -y octocode-mcp@latest` by default, or bunx/pnpm), never
//! `octo mcp`. JSON clients use their native server map; codex writes
//! `[mcp_servers.octocode]` TOML; goose writes `extensions.octocode` YAML.
use super::mcp_clients::{self, CLIENTS, ClientSpec, EntryShape};
use super::mcp_manage::{self, ServerSpec};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The env name bound to the `local.enabled` config key (`OCTOCODE_ENABLE_LOCAL`).
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
    let mut spec = ServerSpec::new(args.method.as_deref().unwrap_or("npx"));
    if args.pass_env {
        for key in local_enabled_env().into_iter().chain([PASS_ENV_TOKEN]) {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                spec.env.insert(key.to_owned(), value);
            }
        }
    }
    if let (Some(enabled), Some(key)) = (args.enable_local, local_enabled_env()) {
        spec.env.insert(
            key.to_owned(),
            if enabled { "true" } else { "false" }.to_owned(),
        );
    }
    spec
}

/// The entry `install` writes, held to the module promise: never `octo mcp`.
fn install_entry(shape: EntryShape, spec: &ServerSpec) -> Result<Value, String> {
    let server = mcp_manage::server_entry(shape, spec);
    reject_octo_mcp(shape, &server)?;
    Ok(server)
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
    /// Restore config from this .bak file path and exit.
    pub rollback: Option<String>,
}

pub fn run(args: InstallArgs) -> u8 {
    if args
        .method
        .as_deref()
        .is_some_and(|method| !matches!(method, "npx" | "bunx" | "pnpm"))
    {
        eprintln!("Installation method must be npx, bunx, or pnpm.");
        return 2;
    }
    // Rollback mode: restore the backup an earlier install left beside the config
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
        let dest = rollback_destination(&bak);
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
        let restore = (|| {
            use octocode_native::config::{
                config_revision, read_private_config, replace_private_config,
            };
            let backup = read_private_config(&bak)?
                .ok_or_else(|| std::io::Error::other("Backup is missing."))?;
            let current = read_private_config(&dest)?;
            replace_private_config(&dest, &config_revision(current.as_deref()), &backup, false)
                .map(|(changed, _)| changed)
        })();
        return match restore {
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
        let ids: Vec<_> = CLIENTS.iter().map(|client| client.id).collect();
        if args.json {
            println!("{}", json!({ "ides": ids }));
        } else {
            for id in ids {
                println!("{id}");
            }
        }
        return 0;
    }
    let Some(requested_ide) = args.ide.as_deref().filter(|value| !value.is_empty()) else {
        super::emit_error(
            "Usage: octocode install --ide <id> [--force] [--dry-run] [--check] [--json]; ids: octocode install --list",
            args.json,
        );
        return 2;
    };
    let client = mcp_clients::client(requested_ide);
    if client.is_some_and(ClientSpec::config_dir_overridden) {
        eprintln!(
            "Claude custom config directory is set; user config path cannot be established safely."
        );
        return 1;
    }
    let Some((client, config_path)) = client.and_then(|client| Some((client, client.home_path()?)))
    else {
        super::emit_error(&unknown_client_message(requested_ide), args.json);
        return 2;
    };
    match install(client, &config_path, &args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// An unknown client id, with the ids that contain it (`claude` names both
/// `claude-code` and `claude-desktop`) or else every id.
fn unknown_client_message(requested: &str) -> String {
    let ids: Vec<&str> = CLIENTS.iter().map(|client| client.id).collect();
    let near: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| id.contains(requested))
        .collect();
    if near.is_empty() {
        format!("Unknown --ide {requested}. Ids: {}", ids.join(", "))
    } else {
        format!(
            "Unknown --ide {requested}. Did you mean: {}?",
            near.join(", ")
        )
    }
}

fn rollback_destination(backup: &Path) -> PathBuf {
    let name = backup
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // Hidden backups are the rolling `.{name}.bak`, or older ones that add a
    // 128-bit nonce: `.{name}-{hex}.bak`. Otherwise `{name}.bak`.
    let Some(stem) = name
        .strip_suffix(".bak")
        .and_then(|name| name.strip_prefix('.'))
        .filter(|stem| !stem.is_empty())
    else {
        return backup.with_extension("");
    };
    let original = match stem.rsplit_once('-') {
        Some((original, nonce))
            if nonce.len() == 32
                && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !original.is_empty() =>
        {
            original
        }
        _ => stem,
    };
    backup.with_file_name(original)
}

fn install(client: &ClientSpec, config_path: &Path, args: &InstallArgs) -> Result<u8, String> {
    use octocode_native::config::{config_revision, read_private_config, replace_private_config};
    let original = read_private_config(config_path)
        .map_err(|error| format!("Cannot safely read agent configuration: {error}"))?;
    let root = mcp_manage::parse_document(client.format, original.as_deref().unwrap_or(""))?;
    if args.check {
        return report_check(client, config_path, &root, args);
    }
    let server = install_entry(client.shape, &server_spec(args))?;
    let keys = client.server_keys();
    let existing = Existing::of(mcp_manage::entry_at(client, &root, &keys), &server);
    if existing != Existing::Absent && !args.force && !args.dry_run {
        return already_installed(config_path, args, existing);
    }
    let rendered = mcp_manage::render_document(
        client,
        original.as_deref().unwrap_or(""),
        &keys,
        Some(&server),
    )?;
    if args.dry_run {
        let preview = json!({"success":true,"dryRun":true,"ide":client.id,"configPath":config_path,"entry":mcp_manage::projection(client,Some(&server))});
        if args.json {
            println!("{preview}");
        } else {
            println!("{}", dry_run_text(client, config_path, &server, &existing));
        }
        return Ok(0);
    }
    // An existing agent file keeps its previous content in the rolling backup.
    let (_, backed_up) = replace_private_config(
        config_path,
        &config_revision(original.as_deref()),
        &rendered,
        original.is_some(),
    )
    .map_err(|error| error.to_string())?;
    if args.json {
        println!(
            "{}",
            json!({"success":true,"configPath":config_path,"backupPath":backed_up})
        );
    } else {
        println!(
            "Installed octocode MCP for {} at {}",
            client.id,
            config_path.display()
        );
    }
    Ok(0)
}

pub(super) fn valid_server(client: &ClientSpec, root: &Value) -> bool {
    let Some(server) = mcp_manage::entry_at(client, root, &client.server_keys()) else {
        return false;
    };
    if mcp_manage::entry_enabled(client, root, server) == Some(false) {
        return false;
    }
    if match client.shape {
        EntryShape::Opencode => server.get("type").and_then(Value::as_str) != Some("local"),
        EntryShape::Goose => server.get("type").and_then(Value::as_str) != Some("stdio"),
        EntryShape::Stdio | EntryShape::Untyped => false,
    } {
        return false;
    }
    let Some((command, args)) = command_line(client.shape, server) else {
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

fn report_check(
    client: &ClientSpec,
    path: &Path,
    root: &Value,
    args: &InstallArgs,
) -> Result<u8, String> {
    let ide = client.id;
    let valid = valid_server(client, root);
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

/// An entry's runner and its arguments: opencode keeps both in one
/// `command` array; every other shape has a command key plus `args`.
fn command_line(shape: EntryShape, server: &Value) -> Option<(&str, &[Value])> {
    if shape == EntryShape::Opencode {
        let (first, rest) = server.get("command")?.as_array()?.split_first()?;
        Some((first.as_str()?, rest))
    } else {
        Some((
            server.get(shape.command_key())?.as_str()?,
            server.get("args")?.as_array()?.as_slice(),
        ))
    }
}

/// Refuse an entry that would run the native host (`octo`/`octocode`) or an
/// `mcp` subcommand instead of the npm server.
fn reject_octo_mcp(shape: EntryShape, server: &Value) -> Result<(), String> {
    let Some((command, args)) = command_line(shape, server) else {
        return Err("refusing to write an MCP entry without a command".into());
    };
    let runner = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(command);
    if runner == "octo" || runner == "octocode" {
        return Err("refusing to write a native octo/octocode MCP command".into());
    }
    if args.iter().any(|value| value.as_str() == Some("mcp")) {
        return Err("refusing to write args containing mcp".into());
    }
    Ok(())
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

/// The `install --dry-run` preview on a terminal: what would change and the
/// entry's launch command, never env values.
fn dry_run_text(
    client: &ClientSpec,
    config_path: &Path,
    server: &Value,
    existing: &Existing,
) -> String {
    let words = |value: Option<&Value>| -> Vec<String> {
        match value {
            Some(Value::String(word)) => vec![word.clone()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        }
    };
    let mut command = words(server.get(client.shape.command_key()));
    command.extend(words(server.get("args")));
    let env = server
        .get(client.shape.env_key())
        .and_then(Value::as_object)
        .map(|env| env.keys().cloned().collect::<Vec<_>>().join(", "))
        .filter(|keys| !keys.is_empty())
        .unwrap_or_else(|| "none".to_owned());
    let action = match existing {
        Existing::Absent => "add the octocode entry",
        Existing::Same => "none: the entry already matches",
        Existing::Different => "replace the existing octocode entry",
    };
    format!(
        "Dry run for {}: nothing written.\n  config   {}\n  action   {action}\n  command  {}\n  env      {env}\nRun without --dry-run to write it.",
        client.id,
        config_path.display(),
        command.join(" ")
    )
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

#[cfg(test)]
fn render_fixture(
    client: &ClientSpec,
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    let text = existing.unwrap_or("");
    let keys = client.server_keys();
    let root = mcp_manage::parse_document(client.format, text)?;
    let server = mcp_manage::default_entry(client, args.method.as_deref().unwrap_or("npx"));
    let already = Existing::of(mcp_manage::entry_at(client, &root, &keys), &server);
    let rendered = mcp_manage::render_document(client, text, &keys, Some(&server))?;
    Ok((rendered, already))
}

#[cfg(test)]
mod tests {
    use super::{Existing, install_entry, reject_octo_mcp, render_fixture, server_spec};
    use crate::cli::mcp_clients::{self, ClientSpec, ConfigFormat, EntryShape};
    use crate::cli::mcp_manage::ServerSpec;
    use serde_json::json;

    fn spec(id: &str) -> &'static ClientSpec {
        mcp_clients::client(id).expect("supported client")
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
    fn client_ids_resolve_exactly_and_near_misses_are_named() {
        assert_eq!(spec("claude-desktop").id, "claude-desktop");
        assert_eq!(spec("cursor").id, "cursor");
        assert!(mcp_clients::client("claude").is_none());
        assert!(mcp_clients::client("vscode").is_none());
        let message = super::unknown_client_message("claude");
        assert!(
            message.contains("claude-code") && message.contains("claude-desktop"),
            "{message}"
        );
    }

    #[test]
    fn server_is_npx_latest_with_yes_flag() {
        let server =
            install_entry(EntryShape::Stdio, &server_spec(&default_args())).expect("allowed");
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
        assert_ne!(spec("codex").format, ConfigFormat::Json);
    }

    #[test]
    fn server_uses_bunx_when_specified() {
        let spec = server_spec(&super::InstallArgs {
            method: Some("bunx".into()),
            ..default_args()
        });
        let server = install_entry(EntryShape::Stdio, &spec).expect("allowed");
        assert_eq!(server["command"], "bunx");
        assert_eq!(server["args"], json!(["octocode-mcp@latest"]));
    }

    /// The install path itself refuses a native-host entry, in every shape.
    #[test]
    fn install_refuses_octo_mcp_entries() {
        for (command, args) in [
            ("octo", &["mcp"][..]),
            ("/usr/local/bin/octocode", &[][..]),
            ("npx", &["mcp"][..]),
        ] {
            let native = ServerSpec {
                command,
                args,
                env: Default::default(),
            };
            for shape in [
                EntryShape::Stdio,
                EntryShape::Untyped,
                EntryShape::Opencode,
                EntryShape::Goose,
            ] {
                assert!(
                    install_entry(shape, &native).is_err(),
                    "{shape:?} {command} {args:?}"
                );
            }
        }
        assert!(reject_octo_mcp(EntryShape::Stdio, &json!({"args": []})).is_err());
    }

    #[test]
    fn client_formats_route_by_extension() {
        assert_eq!(spec("cursor").format, ConfigFormat::Json);
        assert_eq!(spec("codex").format, ConfigFormat::Toml);
        assert_eq!(spec("goose").format, ConfigFormat::Yaml);
    }

    #[test]
    fn codex_and_goose_have_config_paths() {
        assert!(
            spec("codex")
                .home_path()
                .expect("codex config path")
                .ends_with(".codex/config.toml")
        );
        assert!(
            spec("goose")
                .home_path()
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
        let (rendered, already) =
            render_fixture(spec("codex"), None, &default_args()).expect("render");
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
            render_fixture(spec("codex"), Some(existing), &default_args()).expect("merge");
        let parsed: toml::Value = toml::from_str(&merged).expect("valid toml");
        assert!(
            parsed["mcp_servers"].get("other").is_some(),
            "preserves other"
        );
        assert!(parsed["mcp_servers"].get("octocode").is_some());
        assert_eq!(already_present, Existing::Absent);
        let (_again, now_present) =
            render_fixture(spec("codex"), Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode entry"
        );
        let edited = merged.replace("npx", "node");
        let (_, differs) =
            render_fixture(spec("codex"), Some(&edited), &default_args()).expect("diff");
        assert_eq!(differs, Existing::Different, "detects a user-edited entry");
    }

    #[test]
    fn goose_yaml_renders_extensions_and_merges() {
        let (rendered, already) =
            render_fixture(spec("goose"), None, &default_args()).expect("render");
        assert_eq!(already, Existing::Absent);
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).expect("valid yaml");
        let entry = &parsed["extensions"]["octocode"];
        assert_eq!(entry["type"].as_str(), Some("stdio"));
        assert_eq!(entry["cmd"].as_str(), Some("npx"));
        assert_eq!(entry["enabled"].as_bool(), Some(true));

        let existing = "extensions:\n  other:\n    type: stdio\n    cmd: foo\n";
        let (merged, _) =
            render_fixture(spec("goose"), Some(existing), &default_args()).expect("merge");
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&merged).expect("valid yaml");
        assert!(
            parsed["extensions"].get("other").is_some(),
            "preserves other"
        );
        let (_again, now_present) =
            render_fixture(spec("goose"), Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode extension"
        );
    }

    #[test]
    fn reinstall_is_a_noop_when_identical_and_needs_force_when_edited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("mcp.json");
        assert_eq!(
            super::install(spec("cursor"), &path, &default_args()).expect("first"),
            0
        );
        let written = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            super::install(spec("cursor"), &path, &default_args()).expect("again"),
            0
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            written,
            "no rewrite"
        );
        std::fs::write(&path, written.replace("npx", "node")).expect("edit");
        assert_eq!(
            super::install(spec("cursor"), &path, &default_args()).expect("edited"),
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
            let path = dir
                .path()
                .canonicalize()
                .expect("canonical tempdir")
                .join("config.json");
            std::fs::write(&path, r#"{"theme":"dark","unrelated":{"keep":true}}"#)
                .expect("fixture");
            assert_eq!(
                super::install(spec(ide), &path, &default_args()).expect("install"),
                0
            );
            let root: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
            assert_eq!(root["theme"], "dark");
            assert_eq!(root["unrelated"]["keep"], true);
            assert!(root[key].get("octocode").is_some());
            assert!(super::valid_server(spec(ide), &root));
            if ide == "opencode" {
                assert_eq!(
                    root[key]["octocode"]["command"],
                    json!(["npx", "-y", "octocode-mcp@latest"])
                );
            }
            let written = std::fs::read_to_string(&path).expect("read");
            assert_eq!(
                super::install(spec(ide), &path, &default_args()).expect("existing"),
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
            let path = dir
                .path()
                .canonicalize()
                .expect("canonical tempdir")
                .join(file);
            let inspect = |args: &super::InstallArgs| super::install(spec(ide), &path, args);
            assert_eq!(inspect(&check).expect("missing"), 1, "{ide}");
            assert!(!path.exists());
            assert_eq!(inspect(&default_args()).expect("install"), 0);
            let before = std::fs::read(&path).expect("before");
            assert_eq!(inspect(&check).expect("valid"), 0);
            assert_eq!(std::fs::read(&path).expect("after"), before);
        }
        assert!(!super::valid_server(
            spec("cursor"),
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["other-package"]}}})
        ));
        assert!(!super::valid_server(
            spec("vscode-cline"),
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["octocode-mcp@latest"],"disabled":true}}})
        ));
    }

    #[test]
    fn rollback_dry_run_preserves_destination_and_real_rollback_restores() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("config.json");
        let backup = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("config.json.bak");
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
            let path = spec(ide).home_path().expect("path");
            assert!(
                path.ends_with(suffix)
                    || ide == "opencode" && path.ends_with("opencode/opencode.jsonc"),
                "{ide}"
            );
        }
    }
    #[test]
    fn randomized_private_backup_restores_the_original_filename() {
        let path = std::path::Path::new("/safe/.config.json-0123456789abcdef0123456789abcdef.bak");
        assert_eq!(
            super::rollback_destination(path),
            std::path::Path::new("/safe/config.json")
        );
        let path = std::path::Path::new("/safe/..claude.json-0123456789abcdef0123456789abcdef.bak");
        assert_eq!(
            super::rollback_destination(path),
            std::path::Path::new("/safe/.claude.json")
        );
        for (rolling, original) in [
            ("/safe/.mcp.json.bak", "/safe/mcp.json"),
            ("/safe/..claude.json.bak", "/safe/.claude.json"),
        ] {
            let rolling = std::path::Path::new(rolling);
            assert_eq!(
                super::rollback_destination(rolling),
                std::path::Path::new(original)
            );
            assert_eq!(
                octocode_native::config::backup_path(&super::rollback_destination(rolling)),
                rolling
            );
        }
    }
}
