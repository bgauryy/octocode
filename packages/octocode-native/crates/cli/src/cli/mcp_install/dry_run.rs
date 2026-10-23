use super::super::mcp_clients::{ClientSpec, EntryShape};
use super::InstallArgs;
use serde_json::{Value, json};
use std::path::Path;

/// An entry's runner and its arguments: opencode keeps both in one
/// `command` array; every other shape has a command key plus `args`.
pub(super) fn command_line(shape: EntryShape, server: &Value) -> Option<(&str, &[Value])> {
    if shape == EntryShape::Opencode {
        let (first, rest) = server.get("command")?.as_array()?.split_first()?;
        Some((first.as_str()?, rest))
    } else {
        Some((
            server.get("command")?.as_str()?,
            server.get("args")?.as_array()?.as_slice(),
        ))
    }
}

/// Whether the client config already carries an octocode entry, and whether it
/// matches the entry this install would write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Existing {
    Absent,
    Same,
    Different,
}

impl Existing {
    pub(super) fn of<T: PartialEq>(previous: Option<&T>, next: &T) -> Self {
        match previous {
            None => Self::Absent,
            Some(previous) if previous == next => Self::Same,
            Some(_) => Self::Different,
        }
    }
}

/// The `install --dry-run` preview on a terminal: what would change and the
/// entry's launch command, never env values.
pub(super) fn dry_run_text(
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
    let mut command = words(server.get("command"));
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
pub(super) fn already_installed(
    config_path: &Path,
    args: &InstallArgs,
    existing: Existing,
) -> Result<u8, String> {
    if existing == Existing::Same {
        if args.json {
            return Ok(super::super::write_json(&json!({
                "success": true,
                "alreadyInstalled": true,
                "unchanged": true,
                "configPath": config_path
            })));
        }
        eprintln!(
            "octocode is already installed in {} — unchanged",
            config_path.display()
        );
        return Ok(0);
    }
    if args.json {
        let written = super::super::write_json(&json!({
            "success": false,
            "alreadyInstalled": true,
            "configPath": config_path
        }));
        return Ok(if written == 0 { 1 } else { written });
    }
    eprintln!(
        "octocode is already installed in {} — pass --force to overwrite",
        config_path.display()
    );
    Ok(1)
}

#[cfg(test)]
pub(super) fn render_fixture(
    client: &ClientSpec,
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    let text = existing.unwrap_or("");
    let keys = client.server_keys();
    let root = super::mcp_manage::parse_document(client.format, text)?;
    let server = super::mcp_manage::default_entry(client, args.method);
    let already = Existing::of(super::mcp_manage::entry_at(client, &root, &keys), &server);
    let rendered = super::mcp_manage::render_document(client, text, &keys, Some(&server))?;
    Ok((rendered, already))
}
