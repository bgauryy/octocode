use super::{Existing, command_line, render_fixture, server_spec};
use crate::cli::mcp_clients::{self, ClientSpec, ConfigFormat, EntryShape};
use crate::cli::mcp_manage::{Method, ServerSpec, Target, server_entry};
use serde_json::json;
mod clients;
mod entries;
mod lifecycle;

fn spec(id: &str) -> &'static ClientSpec {
    mcp_clients::client(id).expect("supported client")
}

/// The picker's labels: every client id, in registry order.
fn labels() -> Vec<String> {
    mcp_clients::CLIENTS
        .iter()
        .map(|client| client.id.to_owned())
        .collect()
}

/// `client`'s home-scope entry, kept in `path`.
fn home(client: &'static ClientSpec, path: &std::path::Path) -> Target {
    Target {
        client,
        scope: "home".into(),
        path: path.to_path_buf(),
        keys: client.server_keys(),
    }
}

fn default_args() -> super::InstallArgs {
    super::InstallArgs {
        ide: Some("cursor".into()),
        ..super::InstallArgs::default()
    }
}
