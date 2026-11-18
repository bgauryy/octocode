//! Where the `typescript-language-server` CLI is found when it is not on
//! `PATH`, and the node that runs it.

use crate::lsp::commands::{is_executable_path, which_on_path};
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use crate::process_env::EnvMap;
use std::path::{Path, PathBuf};

/// Where the TypeScript server may be looked up when it is not on `PATH`.
pub(in crate::lsp::config) struct TypeScriptServerSearch<'a> {
    /// The running native executable, for both CLI and MCP: its install
    /// tree's `node_modules` may supply an installed TypeScript server.
    pub(in crate::lsp::config) managed_start: Option<&'a Path>,
    /// The invocation directory: the user's own project.
    pub(in crate::lsp::config) current_dir: Option<&'a Path>,
    /// Whether the workspace's own `node_modules` may supply the server.
    pub(in crate::lsp::config) workspace_trusted: bool,
}

impl<'a> TypeScriptServerSearch<'a> {
    /// The search for `workspace_root` from `invocation_dir`: the workspace
    /// is trusted by opt-in (`trust_workspace`) or by lying in the
    /// invocation directory, never by the process's own directory.
    pub(in crate::lsp::config) fn new(
        managed_start: Option<&'a Path>,
        workspace_root: &str,
        trust_workspace: bool,
        invocation_dir: Option<&'a Path>,
    ) -> Self {
        Self {
            managed_start,
            current_dir: invocation_dir,
            workspace_trusted: trust_workspace
                || workspace_is_invocation_root(workspace_root, invocation_dir),
        }
    }
}

/// `true` when `workspace_root` is the directory octocode was invoked in, or
/// inside it — the user's own project rather than an arbitrary scanned
/// checkout (an Octocode clone, say). An invocation directory
/// that is the filesystem root or the home directory proves nothing and
/// never counts.
pub(in crate::lsp::config) fn workspace_is_invocation_root(
    workspace_root: &str,
    current_dir: Option<&Path>,
) -> bool {
    let Some(cwd) = current_dir.map(|cwd| std::fs::canonicalize(cwd).unwrap_or(cwd.to_path_buf()))
    else {
        return false;
    };
    if cwd.parent().is_none() {
        return false;
    }
    let home = std::env::home_dir().map(|home| std::fs::canonicalize(&home).unwrap_or(home));
    if home.as_deref() == Some(cwd.as_path()) {
        return false;
    }
    let workspace = Path::new(workspace_root);
    std::fs::canonicalize(workspace)
        .unwrap_or_else(|_| workspace.to_path_buf())
        .starts_with(&cwd)
}

/// The `typescript-language-server` CLI to run under node when the command
/// is not on `PATH`. Safe by default: a scanned checkout must not be able to
/// swap in an executable it ships, so the lookup order is an explicit
/// `cli.mjs` path, then octocode's own install tree (ancestors of the running
/// executable), then — only when `workspace_trusted` — the workspace's
/// `node_modules`, then the invocation directory's (the user's own project).
/// Returns the CLI with the source it came from.
pub(in crate::lsp::config) fn resolve_typescript_server_cli(
    command: &str,
    workspace_root: &str,
    search: &TypeScriptServerSearch<'_>,
    command_available_on_path: bool,
) -> Option<(PathBuf, ServerSource)> {
    const CLI: &str = "typescript-language-server/lib/cli.mjs";
    if !is_typescript_server_command(command) {
        return None;
    }
    if command_available_on_path || is_executable_path(Path::new(command)) {
        return None;
    }
    let from = |source: ServerSource| move |cli: PathBuf| (cli, source);
    typescript_cli_from_command(command)
        .map(from(ServerSource::Path))
        .or_else(|| {
            search
                .managed_start
                .and_then(|exe| exe.parent())
                .and_then(|dir| find_node_module_file_from(dir, CLI))
                .map(from(ServerSource::Bundled))
        })
        .or_else(|| {
            search
                .workspace_trusted
                .then(|| find_node_module_file_from(Path::new(workspace_root), CLI))
                .flatten()
                .map(from(ServerSource::Workspace))
        })
        .or_else(|| {
            search
                .current_dir
                .and_then(|cwd| find_node_module_file_from(cwd, CLI))
                .map(from(ServerSource::Workspace))
        })
}

/// typescript-language-server takes tsserver from the workspace's own
/// `node_modules/typescript` before its own, which runs repository code on a
/// JavaScript or TypeScript query. Unless the workspace is trusted (the rule of
/// [`TypeScriptServerSearch::new`]), `tsserver.path` names the TypeScript that Node
/// resolves from the server's CLI; a path the configuration already sets stays.
pub(in crate::lsp::config) fn pin_tsserver(
    config: &mut LspLanguageServerConfig,
    workspace_trusted: bool,
) {
    if workspace_trusted {
        return;
    }
    let Some(tsserver) = server_cli(config)
        .as_deref()
        .and_then(Path::parent)
        .and_then(|dir| find_node_module_file_from(dir, "typescript/lib/tsserver.js"))
    else {
        return;
    };
    let pin = serde_json::json!({ "tsserver": { "path": tsserver.to_string_lossy() } });
    super::super::defaults::merge_default_options(config, pin);
}

/// The typescript-language-server CLI a configuration runs: `node <cli.mjs>`, or
/// the command itself (on `PATH`, a link to `cli.mjs`), resolved through links.
fn server_cli(config: &LspLanguageServerConfig) -> Option<PathBuf> {
    let command = Path::new(&config.command);
    let cli = if is_node_executable(command) {
        PathBuf::from(config.args.as_ref()?.first()?)
    } else if is_typescript_server_command(&config.command) {
        which_on_path(command, &config.env.clone().unwrap_or_default()).ok()?
    } else {
        return None;
    };
    let cli = std::fs::canonicalize(&cli).unwrap_or(cli);
    cli.to_string_lossy()
        .contains("typescript-language-server")
        .then_some(cli)
}

pub(in crate::lsp::config) fn is_typescript_server_command(command: &str) -> bool {
    let path = Path::new(command);
    let file_name = path.file_name().and_then(|name| name.to_str());
    matches!(file_name, Some("typescript-language-server" | "cli.mjs"))
        && command.contains("typescript-language-server")
}

pub(in crate::lsp::config) fn typescript_cli_from_command(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if !path.exists() {
        return None;
    }
    let candidate = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if candidate.file_name().and_then(|name| name.to_str()) == Some("cli.mjs")
        && candidate
            .to_string_lossy()
            .contains("typescript-language-server")
    {
        return Some(candidate);
    }
    None
}

fn find_node_module_file_from(start: &Path, package_relative_path: &str) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("node_modules").join(package_relative_path);
        if candidate.exists() {
            return Some(std::fs::canonicalize(&candidate).unwrap_or(candidate));
        }
    }
    None
}

pub(in crate::lsp) fn current_node_command(env: &impl EnvMap) -> Option<String> {
    std::env::current_exe()
        .ok()
        .filter(|path| is_executable_path(path) && is_node_executable(path))
        .or_else(|| which_on_path("node", env).ok())
        .filter(|path| is_executable_path(path))
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .map(|path| path.to_string_lossy().into_owned())
}

pub(in crate::lsp::config) fn is_node_executable(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("node"))
}
