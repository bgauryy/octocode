use super::super::mcp_clients::{CLIENTS, ClientSpec};
use super::super::mcp_manage::{self};
use super::{Scope, valid_server};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// Ask for a client by number; `labels` describe [`CLIENTS`] in order.
pub(super) fn select_client(
    labels: &[String],
    reader: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Option<&'static str>> {
    writeln!(output, "Select an MCP client:")?;
    for (index, label) in labels.iter().enumerate() {
        writeln!(output, "  {}. {label}", index + 1)?;
    }
    loop {
        write!(output, "Client number (Enter to cancel): ")?;
        output.flush()?;
        let mut line = Vec::new();
        std::io::Read::take(&mut *reader, 1025).read_until(b'\n', &mut line)?;
        if line.len() > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selection is too long.",
            ));
        }
        let selection = std::str::from_utf8(&line)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
            .trim();
        if selection.is_empty() {
            return Ok(None);
        }
        if let Some(client) = selection
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| CLIENTS.get(index))
        {
            return Ok(Some(client.id));
        }
        writeln!(output, "Enter a number from 1 to {}.", CLIENTS.len())?;
    }
}

/// One client's Octocode entry at a scope, as `--list` and the picker show it.
pub(super) struct ClientStatus {
    pub(super) id: &'static str,
    /// `installed` (a valid entry), `disabled`, `custom` (an `octocode`
    /// entry install did not write), `absent`, `invalid` (unreadable or
    /// unparsable config) or `unsupported` (no such scope).
    pub(super) status: &'static str,
    /// The scopes the client reads.
    scopes: Vec<&'static str>,
    path: Option<PathBuf>,
    error: Option<String>,
}

pub(super) fn client_status(client: &'static ClientSpec, scope: Scope, cwd: &Path) -> ClientStatus {
    let scopes = || client.scopes().collect::<Vec<_>>();
    let unsupported = |error: Option<String>| ClientStatus {
        id: client.id,
        status: if error.is_some() {
            "invalid"
        } else {
            "unsupported"
        },
        scopes: scopes(),
        path: None,
        error,
    };
    if !client.scopes().any(|name| name == scope.name()) {
        return unsupported(None);
    }
    let target = match mcp_manage::target(client.id, scope.name(), cwd) {
        Ok(target) => target,
        Err(error) => return unsupported(Some(error)),
    };
    let inspected = mcp_manage::inspect(&target);
    let status = match inspected["status"].as_str() {
        Some("configured") => {
            let valid = octocode_native::config::read_private_config(&target.path)
                .ok()
                .flatten()
                .and_then(|text| mcp_manage::parse_document(client.format, &text).ok())
                .is_some_and(|root| valid_server(client, &root, &target.keys));
            if valid { "installed" } else { "custom" }
        }
        Some("disabled") => "disabled",
        Some("absent") => "absent",
        _ => "invalid",
    };
    ClientStatus {
        id: client.id,
        status,
        scopes: scopes(),
        path: Some(target.path),
        error: inspected["error"].as_str().map(str::to_owned),
    }
}

pub(super) fn list_clients(scope: Scope, statuses: &[ClientStatus], json_out: bool) -> u8 {
    if json_out {
        let ides = statuses
            .iter()
            .map(|row| {
                let mut value = json!({"id": row.id, "status": row.status, "scopes": row.scopes, "configPath": row.path});
                if let Some(error) = &row.error {
                    value["error"] = json!(error);
                }
                value
            })
            .collect::<Vec<_>>();
        return super::super::write_json(&json!({"scope": scope.name(), "ides": ides}));
    }
    let width = statuses.iter().map(|row| row.id.len()).max().unwrap_or(0);
    println!("MCP clients (--scope {}):", scope.name());
    for row in statuses {
        let detail = row
            .path
            .as_deref()
            .map(|path| path.display().to_string())
            .or_else(|| row.error.clone())
            .unwrap_or_else(|| format!("scopes: {}", row.scopes.join(", ")));
        println!("  {:width$}  {:11}  {detail}", row.id, row.status);
    }
    println!(
        "Install: octocode install --ide <id> [--scope {}]",
        scope.name()
    );
    0
}

/// An unknown client id, with the ids that contain it (`claude` names both
/// `claude-code` and `claude-desktop`) or else every id.
pub(super) fn unknown_client_message(requested: &str) -> String {
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
