//! `lsp-servers.json`: the user's own server entries, consulted before the
//! built-in routes (see [`super::catalog`]).

use super::catalog::spec_for_extension;
use super::invocation::{is_interpreter_eval_launch, resolve_server_invocation};
use super::types::LspDiscoveryOptions;
use crate::lsp::commands::is_rejected_shell;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct UserConfigFile {
    #[serde(rename = "languageServers")]
    language_servers: HashMap<String, UserServerSpec>,
}

#[derive(Deserialize)]
struct UserServerSpec {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(rename = "languageId")]
    language_id: String,
    #[serde(rename = "initializationOptions")]
    initialization_options: Option<Value>,
    /// Resident-memory cap for this server's process tree, in MiB; unset
    /// takes the host-derived default. `0` disables the cap.
    #[serde(rename = "maxMemoryMb")]
    max_memory_mb: Option<u32>,
    /// Extra environment variables for the server process.
    env: Option<HashMap<String, String>>,
}

pub(super) fn user_server_for_extension(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    for (config_path, source) in user_config_paths(workspace_root, options) {
        let Ok(content) = std::fs::read_to_string(config_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<UserConfigFile>(&content) else {
            continue;
        };
        // An entry for one extension of a language family routes the whole
        // family (`.ts` also `.mts`, `.cts`, `.tsx`): one project, one
        // server, not two project models split by spelling. The entry's own
        // extension keeps its language id; a sibling takes the built-in one.
        let (server, language_id) = match parsed.language_servers.get(extension) {
            Some(server) => (server, server.language_id.clone()),
            None => {
                let Some((server, spec)) = family_siblings(extension).iter().find_map(|sibling| {
                    let server = parsed.language_servers.get(*sibling)?;
                    Some((server, spec_for_extension(extension)?))
                }) else {
                    continue;
                };
                (server, spec.language_id.to_owned())
            }
        };
        if is_rejected_shell(&server.command)
            || is_interpreter_eval_launch(&server.command, &server.args)
        {
            continue;
        }
        let trust_workspace = options.trusts_project();
        // The entry is the user's choice wherever its program resolves.
        let (command, args, _) = resolve_server_invocation(
            &server.command,
            server.args.clone(),
            workspace_root,
            trust_workspace,
            options.invocation_dir.as_deref(),
        );
        return Some(LspLanguageServerConfig {
            command,
            args: Some(args),
            workspace_root: workspace_root.to_owned(),
            language_id: Some(language_id),
            initialization_options: server.initialization_options.clone(),
            env: server.env.clone(),
            max_memory_mb: server.max_memory_mb,
            source,
        });
    }
    None
}

/// The extensions one `lsp-servers.json` entry also routes, in the order
/// they are tried: the TypeScript family's other spellings, and the
/// JavaScript family's.
fn family_siblings(extension: &str) -> &'static [&'static str] {
    match extension {
        ".ts" | ".mts" | ".cts" | ".tsx" => &[".ts", ".tsx", ".mts", ".cts"],
        ".js" | ".mjs" | ".cjs" | ".jsx" => &[".js", ".jsx", ".mjs", ".cjs"],
        _ => &[],
    }
}

/// The `lsp-servers.json` files consulted, in order, each with the source
/// its entries report.
fn user_config_paths(
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Vec<(PathBuf, ServerSource)> {
    let mut paths = Vec::new();
    if let Some(path) = options.config_path.as_ref() {
        paths.push((path.clone(), ServerSource::User));
    }
    if options.trusts_project() {
        paths.push((
            Path::new(workspace_root).join(".octocode/lsp-servers.json"),
            ServerSource::Project,
        ));
    }
    if let Some(home) = &options.octocode_home {
        paths.push((home.join("lsp-servers.json"), ServerSource::User));
    }
    paths
}
