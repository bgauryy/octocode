//! Where the `typescript-language-server` CLI is found when it is not on
//! `PATH`, and the node that runs it.

use crate::lsp::commands::is_executable_path;
use crate::lsp::types::ServerSource;
use std::path::{Path, PathBuf};

/// Where the TypeScript server may be looked up when it is not on `PATH`.
pub(in crate::lsp::config) struct TypeScriptServerSearch<'a> {
    /// The file octocode's code was loaded from (the executable, or the Node
    /// addon under MCP): its install tree's `node_modules` (octocode ships
    /// `typescript-language-server`) is octocode-managed.
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
/// checkout (a clone under the clone cache, say). An invocation directory
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

fn is_typescript_server_command(command: &str) -> bool {
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

pub(in crate::lsp) fn current_node_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .filter(|path| is_executable_path(path) && is_node_executable(path))
        .or_else(|| which::which("node").ok())
        .filter(|path| is_executable_path(path))
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .map(|path| path.to_string_lossy().into_owned())
}

pub(in crate::lsp::config) fn is_node_executable(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("node"))
}
