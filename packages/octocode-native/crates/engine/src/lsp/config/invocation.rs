use super::defaults::{apply_rust_analyzer_defaults, apply_server_default_options};
use super::types::{LspDiscoveryOptions, ServerSpec};
use crate::lsp::bundled_tsgo::bundled_tsgo;
use crate::lsp::commands::{command_resolves_to_executable, is_executable_path};
use crate::lsp::module_path::install_anchor;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(super) fn config_from_spec(
    spec: ServerSpec,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
) -> LspLanguageServerConfig {
    let (command, args, source) =
        resolve_spec_invocation(&spec, workspace_root, options, anchor, native_typescript);
    let mut config = LspLanguageServerConfig {
        command,
        args: Some(args),
        workspace_root: workspace_root.to_owned(),
        language_id: Some(spec.language_id.to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source,
    };
    // The built-in Rust route is rust-analyzer even from a managed install
    // whose file name differs.
    if spec.language_id == "rust" {
        apply_rust_analyzer_defaults(&mut config);
    } else {
        apply_server_default_options(&mut config, options.octocode_home.as_deref());
    }
    config
}

/// Resolve a spec's command + args, and where they came from: a managed
/// install, else the command on `PATH`; Python prefers an installed
/// pyright-family server. Any other server (e.g. `tsgo`) is a user
/// `lsp-servers.json` entry.
fn resolve_spec_invocation(
    spec: &ServerSpec,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
) -> (String, Vec<String>, ServerSource) {
    let trust_workspace = options.trusts_project();
    if spec.language_id == "python" {
        let path_var = crate::process_env::var_os("PATH");
        if let Some((command, args)) = resolve_pyright_family(workspace_root, path_var.as_deref()) {
            return (command, args, ServerSource::Path);
        }
    }

    // TypeScript prefers a native tsgo: the trusted workspace's own (its
    // TypeScript version), else octocode's bundled one, which is safe for any
    // workspace because it comes from octocode's install tree.
    if native_typescript
        && is_typescript_only_spec(spec)
        && let Some((tsgo, source)) = trust_workspace
            .then(|| workspace_tsgo(workspace_root))
            .flatten()
            .map(|tsgo| (tsgo, ServerSource::Workspace))
            .or_else(|| {
                anchor
                    .and_then(bundled_tsgo)
                    .map(|tsgo| (tsgo, ServerSource::Bundled))
            })
    {
        return (
            tsgo.to_string_lossy().into_owned(),
            TSGO_ARGS.iter().map(|arg| (*arg).to_owned()).collect(),
            source,
        );
    }

    let managed = options.managed_server(spec.command);
    let command = managed
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| spec.command.to_owned());
    let (command, args, source) = resolve_server_invocation(
        &command,
        spec.args.iter().map(|arg| (*arg).to_owned()).collect(),
        workspace_root,
        trust_workspace,
        options.invocation_dir.as_deref(),
    );
    let source = if managed.is_some() {
        ServerSource::Managed
    } else {
        source
    };
    (command, args, source)
}

/// Arguments that start the native TypeScript server in LSP mode.
const TSGO_ARGS: &[&str] = &["--lsp", "-stdio"];

/// TypeScript sources only: tsgo drops JavaScript constructor functions and
/// `this` expandos, so JavaScript keeps tsserver.
fn is_typescript_only_spec(spec: &ServerSpec) -> bool {
    spec.command == "typescript-language-server"
        && matches!(spec.language_id, "typescript" | "typescriptreact")
}

/// The workspace's own native TypeScript server (`node_modules/.bin/tsgo`
/// of the workspace or an ancestor): 2-3x faster than tsserver on a large
/// program. Only a trusted workspace is searched, because a checkout must
/// not swap in an executable it ships.
fn workspace_tsgo(workspace_root: &str) -> Option<PathBuf> {
    let name = if cfg!(windows) { "tsgo.cmd" } else { "tsgo" };
    Path::new(workspace_root)
        .ancestors()
        .map(|dir| dir.join("node_modules").join(".bin").join(name))
        .find(|candidate| is_executable_path(candidate))
}

/// Python servers in preference order. basedpyright and pyright implement
/// callHierarchy, workspace/symbol, and implementation; pylsp (the fallback)
/// implements none of them.
const PYRIGHT_FAMILY: &[&str] = &["basedpyright-langserver", "pyright-langserver"];

/// The first pyright-family server installed on `path_var`, as
/// `(command, ["--stdio"])`. Only `PATH` is searched: this preference is
/// automatic, so a checkout's own `node_modules/.bin` must not be able to
/// swap in an executable it ships. Users who want a workspace-local server add
/// an `lsp-servers.json` entry. `None` means neither
/// is installed and the caller falls back to `pylsp`.
pub(super) fn resolve_pyright_family(
    workspace_root: &str,
    path_var: Option<&OsStr>,
) -> Option<(String, Vec<String>)> {
    let cwd = Path::new(workspace_root);
    PYRIGHT_FAMILY.iter().find_map(|name| {
        let found = path_var
            .and_then(|paths| which::which_in(name, Some(paths), cwd).ok())
            .filter(|path| is_executable_path(path))?;
        Some((
            found.to_string_lossy().into_owned(),
            vec!["--stdio".to_owned()],
        ))
    })
}

pub(super) fn is_interpreter_eval_launch(command: &str, args: &[String]) -> bool {
    if !is_generic_interpreter_command(command) {
        return false;
    }
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "-e" | "--eval" | "-c" | "--command" | "-p" | "--print"
        )
    })
}

fn is_generic_interpreter_command(command: &str) -> bool {
    let Some(stem) = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    let normalized = stem.to_ascii_lowercase();
    normalized == "node"
        || normalized == "python"
        || normalized.starts_with("python3")
        || normalized == "ruby"
        || normalized == "perl"
        || normalized == "php"
}

/// `rust-analyzer` or `rust-analyzer.exe` by file name (any directory).
pub(crate) fn is_rust_analyzer_command(command: &str) -> bool {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "rust-analyzer" || name == "rust-analyzer.exe"
        })
}

/// Resolve a server command to `(program, args, source)`: the source is where
/// a TypeScript server CLI was found, else [`ServerSource::Path`].
/// `trust_workspace` is the explicit opt-in (`lsp.trustProjectConfig` /
/// trusted project config) that lets the workspace's own `node_modules`
/// supply the TypeScript server; see [`resolve_typescript_server_cli`].
pub(super) fn resolve_server_invocation(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    trust_workspace: bool,
    invocation_dir: Option<&Path>,
) -> (String, Vec<String>, ServerSource) {
    let managed_start = install_anchor();
    resolve_server_invocation_with_environment(
        command,
        args,
        workspace_root,
        &TypeScriptServerSearch::new(
            managed_start.as_deref(),
            workspace_root,
            trust_workspace,
            invocation_dir,
        ),
        command_resolves_to_executable(command),
    )
}

/// Where the TypeScript server may be looked up when it is not on `PATH`.
pub(super) struct TypeScriptServerSearch<'a> {
    /// The file octocode's code was loaded from (the executable, or the Node
    /// addon under MCP): its install tree's `node_modules` (octocode ships
    /// `typescript-language-server`) is octocode-managed.
    pub(super) managed_start: Option<&'a Path>,
    /// The invocation directory: the user's own project.
    pub(super) current_dir: Option<&'a Path>,
    /// Whether the workspace's own `node_modules` may supply the server.
    pub(super) workspace_trusted: bool,
}

impl<'a> TypeScriptServerSearch<'a> {
    /// The search for `workspace_root` from `invocation_dir`: the workspace
    /// is trusted by opt-in (`trust_workspace`) or by lying in the
    /// invocation directory, never by the process's own directory.
    pub(super) fn new(
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
pub(super) fn workspace_is_invocation_root(
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

pub(super) fn resolve_server_invocation_with_environment(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    search: &TypeScriptServerSearch<'_>,
    command_available_on_path: bool,
) -> (String, Vec<String>, ServerSource) {
    if let Some((cli_path, source)) =
        resolve_typescript_server_cli(command, workspace_root, search, command_available_on_path)
        && let Some(node_command) = current_node_command()
    {
        let mut resolved_args = Vec::with_capacity(args.len() + 1);
        resolved_args.push(cli_path.to_string_lossy().into_owned());
        resolved_args.extend(args);
        return (node_command, resolved_args, source);
    }

    (
        resolve_known_server_command(command),
        args,
        ServerSource::Path,
    )
}

pub(super) fn resolve_known_server_command(command: &str) -> String {
    if Path::new(command).is_absolute() || command_resolves_to_executable(command) {
        return command.to_owned();
    }
    match command {
        "pylsp" => find_python_user_script("pylsp").unwrap_or_else(|| command.to_owned()),
        _ => command.to_owned(),
    }
}

/// The `typescript-language-server` CLI to run under node when the command
/// is not on `PATH`. Safe by default: a scanned checkout must not be able to
/// swap in an executable it ships, so the lookup order is an explicit
/// `cli.mjs` path, then octocode's own install tree (ancestors of the running
/// executable), then — only when `workspace_trusted` — the workspace's
/// `node_modules`, then the invocation directory's (the user's own project).
/// Returns the CLI with the source it came from.
pub(super) fn resolve_typescript_server_cli(
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

pub(super) fn typescript_cli_from_command(command: &str) -> Option<PathBuf> {
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

pub(super) fn is_node_executable(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("node"))
}

fn find_python_user_script(script_name: &str) -> Option<String> {
    let home = std::env::home_dir()?;
    let candidate = home.join(".local/bin").join(script_name);
    if candidate.exists() {
        return Some(candidate.to_string_lossy().into_owned());
    }

    let python_dir = home.join("Library/Python");
    let Ok(entries) = std::fs::read_dir(python_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(script_name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}
