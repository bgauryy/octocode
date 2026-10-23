use super::super::mcp_clients::{ClientSpec, EntryShape};
use super::super::mcp_manage::{self, Method, Target};
use super::{Existing, InstallArgs, already_installed, command_line, dry_run_text, server_spec};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn rollback_destination(backup: &Path) -> PathBuf {
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

pub(super) fn install(target: &Target, args: &InstallArgs) -> Result<u8, String> {
    use octocode_native::config::{config_revision, read_private_config, replace_private_config};
    let (client, config_path, keys) = (target.client, target.path.as_path(), &target.keys);
    let original = read_private_config(config_path)
        .map_err(|error| format!("Cannot safely read agent configuration: {error}"))?;
    let root = mcp_manage::parse_document(client.format, original.as_deref().unwrap_or(""))?;
    if args.check {
        return report_check(target, &root, args);
    }
    if args.remove {
        return remove(target, original.as_deref(), &root, args);
    }
    let server = mcp_manage::server_entry(client.shape, &server_spec(args));
    let existing = Existing::of(mcp_manage::entry_at(client, &root, keys), &server);
    if existing != Existing::Absent && !args.force && !args.dry_run {
        return already_installed(config_path, args, existing);
    }
    let rendered = mcp_manage::render_document(
        client,
        original.as_deref().unwrap_or(""),
        keys,
        Some(&server),
    )?;
    if args.dry_run {
        let preview = json!({"success":true,"dryRun":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"entry":mcp_manage::projection(client,Some(&server))});
        if args.json {
            return Ok(super::super::write_json(&preview));
        }
        println!("{}", dry_run_text(client, config_path, &server, &existing));
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
        return Ok(super::super::write_json(
            &json!({"success":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"backupPath":backed_up}),
        ));
    }
    println!(
        "Installed octocode MCP for {} at {}",
        client.id,
        config_path.display()
    );
    println!(
        "Next: restart {} so it loads the server; check GitHub access with `octocode auth status`.",
        client.id
    );
    Ok(0)
}

/// `install --remove`: delete the Octocode entry, keeping the previous file
/// in the rolling backup. No entry is a successful no-op; an entry install
/// did not write needs `--force`, as overwriting one does.
pub(super) fn remove(
    target: &Target,
    original: Option<&str>,
    root: &Value,
    args: &InstallArgs,
) -> Result<u8, String> {
    use octocode_native::config::{config_revision, replace_private_config};
    let (client, config_path) = (target.client, target.path.as_path());
    let report = |value: Value| super::super::write_json(&value);
    let Some(entry) = mcp_manage::entry_at(client, root, &target.keys) else {
        if args.json {
            return Ok(report(
                json!({"success":true,"removed":false,"ide":client.id,"scope":target.scope,"configPath":config_path}),
            ));
        }
        eprintln!(
            "No octocode entry for {} at {}; nothing to remove",
            client.id,
            config_path.display()
        );
        return Ok(0);
    };
    if !args.force && !valid_server(client, root, &target.keys) {
        if args.json {
            let written = report(
                json!({"success":false,"removed":false,"custom":true,"ide":client.id,"scope":target.scope,"configPath":config_path}),
            );
            return Ok(if written == 0 { 1 } else { written });
        }
        eprintln!(
            "The octocode entry in {} was not written by install; pass --force to remove it",
            config_path.display()
        );
        return Ok(1);
    }
    if args.dry_run {
        if args.json {
            return Ok(report(
                json!({"success":true,"dryRun":true,"remove":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"entry":mcp_manage::projection(client,Some(entry))}),
            ));
        }
        println!(
            "Dry run for {}: nothing written.\n  config   {}\n  action   remove the octocode entry\nRun without --dry-run to remove it.",
            client.id,
            config_path.display()
        );
        return Ok(0);
    }
    let rendered = mcp_manage::render_document(client, original.unwrap_or(""), &target.keys, None)?;
    let (_, backed_up) =
        replace_private_config(config_path, &config_revision(original), &rendered, true)
            .map_err(|error| error.to_string())?;
    if args.json {
        return Ok(report(
            json!({"success":true,"removed":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"backupPath":backed_up}),
        ));
    }
    println!(
        "Removed octocode MCP for {} from {}",
        client.id,
        config_path.display()
    );
    if let Some(backup) = backed_up {
        println!("Undo: octocode install --rollback {}", backup.display());
    }
    println!("Next: restart {} so it unloads the server.", client.id);
    Ok(0)
}

pub(in super::super) fn valid_server(client: &ClientSpec, root: &Value, keys: &[String]) -> bool {
    let Some(server) = mcp_manage::entry_at(client, root, keys) else {
        return false;
    };
    if mcp_manage::entry_enabled(client, root, server) == Some(false) {
        return false;
    }
    if match client.shape {
        EntryShape::Opencode => server.get("type").and_then(Value::as_str) != Some("local"),
        EntryShape::Stdio | EntryShape::Untyped => false,
    } {
        return false;
    }
    let Some((command, args)) = command_line(client.shape, server) else {
        return false;
    };
    let Some(method) = Method::of_command(command) else {
        return false;
    };
    args.iter().all(Value::is_string)
        && args.iter().any(|arg| {
            arg.as_str()
                .is_some_and(|value| value == "octocode-mcp" || value.starts_with("octocode-mcp@"))
        })
        && (method != Method::Pnpm || args.first().and_then(Value::as_str) == Some("dlx"))
}

pub(super) fn report_check(
    target: &Target,
    root: &Value,
    args: &InstallArgs,
) -> Result<u8, String> {
    let (ide, path) = (target.client.id, target.path.as_path());
    let valid = valid_server(target.client, root, &target.keys);
    let exit = if valid { 0 } else { 1 };
    if args.json {
        let written = super::super::write_json(
            &json!({"success": valid, "check": true, "ide": ide, "scope": target.scope, "configPath": path, "installed": valid}),
        );
        return Ok(if written == 0 { exit } else { written });
    }
    if valid {
        println!("Octocode MCP is configured for {ide} at {}", path.display());
    } else {
        eprintln!(
            "No valid Octocode MCP entry for {ide} at {}",
            path.display()
        );
    }
    Ok(exit)
}
