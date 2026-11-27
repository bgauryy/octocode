use super::{Existing, InstallArgs, command_line, server_spec};
use crate::cli::mcp_clients::{self, ClientSpec, ConfigFormat, EntryShape};
use crate::cli::mcp_manage::{self, Method, ServerSpec, Target, server_entry};
use serde_json::json;
mod clients;
mod entries;
mod lifecycle;
mod output;

/// The document installing for `client` writes over `existing` (none: an
/// empty file), and how the entry already there compares with it.
fn render_fixture(
    client: &ClientSpec,
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    let text = existing.unwrap_or("");
    let keys = client.server_keys();
    let root = mcp_manage::parse_document(client.format, text)?;
    let server = mcp_manage::default_entry(client, args.method);
    let already = Existing::of(mcp_manage::entry_at(client, &root, &keys), &server);
    let rendered = mcp_manage::render_document(client, text, &keys, Some(&server))?;
    Ok((rendered, already))
}

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
