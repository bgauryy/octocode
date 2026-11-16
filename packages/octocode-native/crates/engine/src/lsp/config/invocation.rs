use super::route::Route;
use super::types::{LspDiscoveryOptions, ServerSpec};
use crate::lsp::bundled_tsgo::bundled_tsgo;
use crate::lsp::commands::{command_resolves_to_executable, is_executable_path};
use crate::process_env::EnvMap;
use crate::lsp::module_path::install_anchor;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(super) mod rustup;
mod typescript_cli;

pub(in crate::lsp) use typescript_cli::current_node_command;
#[cfg(all(test, unix))]
pub(super) use typescript_cli::workspace_is_invocation_root;
pub(super) use typescript_cli::{
    TypeScriptServerSearch, is_node_executable, is_typescript_server_command,
    resolve_typescript_server_cli, typescript_cli_from_command,
};

#[cfg(all(test, unix))]
pub(super) use typescript_cli::pin_tsserver as typescript_cli_pin_for_tests;

/// [`typescript_cli::pin_tsserver`] under `options`' trust of `workspace_root`.
pub(super) fn pin_tsserver(config: &mut LspLanguageServerConfig, options: &LspDiscoveryOptions) {
    let trusted = options.trusts_project()
        || typescript_cli::workspace_is_invocation_root(
            &config.workspace_root,
            options.invocation_dir.as_deref(),
        );
    typescript_cli::pin_tsserver(config, trusted);
}

/// The built-in route of `spec` for `workspace_root`: its launch before the
/// per-server defaults ([`Route::finished`]).
pub(super) fn route_from_spec(
    spec: &ServerSpec,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
) -> Route {
    let resolved =
        resolve_spec_invocation(spec, workspace_root, options, anchor, native_typescript);
    let config = LspLanguageServerConfig {
        command: resolved.command,
        args: Some(resolved.args),
        workspace_root: workspace_root.to_owned(),
        language_id: Some(spec.language_id.to_owned()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: resolved.source,
    };
    Route::built_in(spec, config, resolved.answered)
}

/// A launch resolved for a spec, and whether every question resolving it
/// asked was answered.
struct Resolved {
    command: String,
    args: Vec<String>,
    source: ServerSource,
    answered: bool,
}

/// Resolve a spec's command + args, and where they came from: a rustup
/// toolchain that ships the command for this workspace, else a managed
/// install, else the command on `PATH`; Python prefers an installed
/// pyright-family server. Any other server (e.g. `tsgo`) is a user
/// `lsp-servers.json` entry.
fn resolve_spec_invocation(
    spec: &ServerSpec,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
) -> Resolved {
    let trust_workspace = options.trusts_project();
    if spec.language_id == "python" {
        let path_var = crate::process_env::call_var(&options.env, "PATH");
        if let Some((command, args)) = resolve_pyright_family(
            workspace_root,
            path_var.as_deref(),
            std::env::home_dir().as_deref(),
        ) {
            return Resolved {
                command,
                args,
                source: ServerSource::Path,
                answered: true,
            };
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
        return Resolved {
            command: tsgo.to_string_lossy().into_owned(),
            args: TSGO_ARGS.iter().map(|arg| (*arg).to_owned()).collect(),
            source,
            answered: true,
        };
    }

    // A toolchain-matched rustup component is an official build for this
    // project's Rust version: the rustup proxy on PATH runs it, ahead of a
    // managed download.
    let toolchain_component = rustup::toolchain_component(
        &rustup::RustupView::of(&options.env),
        Path::new(workspace_root),
        spec.command,
    );
    let answered = toolchain_component.is_ok();
    let managed = toolchain_component
        .ok()
        .flatten()
        .is_none()
        .then(|| options.managed_server(spec.command))
        .flatten();
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
        &options.env,
    );
    let source = if managed.is_some() {
        ServerSource::Managed
    } else {
        source
    };
    Resolved {
        command,
        args,
        source,
        answered,
    }
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
pub(super) const PYRIGHT_FAMILY: &[&str] = &["basedpyright-langserver", "pyright-langserver"];

/// The first pyright-family server installed on `path_var` or in the user's
/// Python script directories under `home` (`pip install --user` lands outside
/// `PATH` as often as not), as `(command, ["--stdio"])`. A checkout's own
/// `node_modules/.bin` is never searched: this preference is automatic, so
/// a checkout must not be able to swap in an executable it ships. Users
/// who want a workspace-local server add an `lsp-servers.json` entry.
/// `None` means neither is installed and the caller falls back to `pylsp`.
pub(super) fn resolve_pyright_family(
    workspace_root: &str,
    path_var: Option<&OsStr>,
    home: Option<&Path>,
) -> Option<(String, Vec<String>)> {
    let cwd = Path::new(workspace_root);
    PYRIGHT_FAMILY.iter().find_map(|name| {
        let found = path_var
            .and_then(|paths| which::which_in(name, Some(paths), cwd).ok())
            .filter(|path| is_executable_path(path))
            .or_else(|| {
                find_python_user_script_under(home?, name)
                    .map(PathBuf::from)
                    .filter(|path| is_executable_path(path))
            })?;
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

pub(crate) fn is_generic_interpreter_command(command: &str) -> bool {
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
    env: &impl EnvMap,
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
        command_resolves_to_executable(command, env),
        env,
    )
}

pub(super) fn resolve_server_invocation_with_environment(
    command: &str,
    args: Vec<String>,
    workspace_root: &str,
    search: &TypeScriptServerSearch<'_>,
    command_available_on_path: bool,
    env: &impl EnvMap,
) -> (String, Vec<String>, ServerSource) {
    if let Some((cli_path, source)) =
        resolve_typescript_server_cli(command, workspace_root, search, command_available_on_path)
        && let Some(node_command) = current_node_command(env)
    {
        let mut resolved_args = Vec::with_capacity(args.len() + 1);
        resolved_args.push(cli_path.to_string_lossy().into_owned());
        resolved_args.extend(args);
        return (node_command, resolved_args, source);
    }

    (
        resolve_known_server_command(command, env),
        args,
        ServerSource::Path,
    )
}

pub(super) fn resolve_known_server_command(command: &str, env: &impl EnvMap) -> String {
    if Path::new(command).is_absolute() || command_resolves_to_executable(command, env) {
        return command.to_owned();
    }
    match command {
        "pylsp" => find_python_user_script("pylsp").unwrap_or_else(|| command.to_owned()),
        _ => command.to_owned(),
    }
}

fn find_python_user_script(script_name: &str) -> Option<String> {
    find_python_user_script_under(&std::env::home_dir()?, script_name)
}

/// `home`'s Python user script directories, in search order: `~/.local/bin`,
/// then each `~/Library/Python/*/bin`.
pub(super) fn python_user_script_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![home.join(".local/bin")];
    dirs.extend(
        std::fs::read_dir(home.join("Library/Python"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("bin")),
    );
    dirs
}

/// `script_name` in `home`'s Python user script directories.
fn find_python_user_script_under(home: &Path, script_name: &str) -> Option<String> {
    python_user_script_dirs(home)
        .into_iter()
        .map(|dir| dir.join(script_name))
        .find(|candidate| candidate.exists())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}
