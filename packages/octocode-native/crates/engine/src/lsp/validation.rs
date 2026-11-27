use crate::error::{Error, Result};
use crate::lsp::commands::{
    has_path_separator, is_executable_path, is_rejected_shell, which_on_path,
};
use crate::lsp::config::install_hint;
use crate::process_env::EnvMap;
use std::path::Path;

/// The launchable form of a configured server command: a shell wrapper is
/// refused; an absolute command, or a relative one with a path separator
/// (`./node_modules/.bin/foo`) resolved against `base` (the
/// workspace root) when it is not found from the process directory: an
/// `lsp-servers.json` entry then launches the same program from the CLI in
/// the repository and from an MCP server started elsewhere.
pub(crate) fn validate_lsp_server_path(
    command: String,
    base: Option<&Path>,
    env: &impl EnvMap,
) -> Result<String> {
    if command.trim().is_empty() {
        return Err(Error::new("Language server command is required"));
    }
    if is_rejected_shell(&command) {
        return Err(Error::new(format!(
            "Shell wrapper commands are not allowed: {command}"
        )));
    }

    let command_path = Path::new(&command);
    if command_path.is_absolute() || has_path_separator(&command) {
        let rebased = base
            .filter(|_| command_path.is_relative() && !command_path.exists())
            .map(|base| base.join(command_path));
        let command_path = rebased.as_deref().unwrap_or(command_path);
        if !command_path.exists() {
            return Err(Error::new(format!(
                "Language server path does not exist: {command}"
            )));
        }
        if !is_executable_path(command_path) {
            return Err(Error::new(format!(
                "Language server path is not executable: {command}"
            )));
        }
        return absolute_string(command_path);
    }

    let resolved = which_on_path(&command, env).map_err(|err| {
        Error::new(format!(
            "Language server command not found: {command}: {err}{}",
            lookup_note(&command)
        ))
    })?;
    if !is_executable_path(&resolved) {
        return Err(Error::new(format!(
            "Language server command is not executable: {}",
            resolved.display()
        )));
    }
    absolute_string(&resolved)
}

/// How to install a missing built-in server; empty for any other command.
fn lookup_note(command: &str) -> String {
    install_hint(command)
        .map(|hint| format!(". {hint}"))
        .unwrap_or_default()
}

/// Absolute path as a string, preserving the executable's own filename
/// (unlike `fs::canonicalize`, this never resolves symlinks). A rustup-style
/// toolchain proxy (`rust-analyzer`, `rustfmt`, `cargo-clippy`, ...) is a
/// symlink to a single `rustup` binary that decides which tool to run by
/// looking at its own invoked name (`argv[0]`'s basename) — canonicalizing
/// `~/.cargo/bin/rust-analyzer` resolves it to `~/.cargo/bin/rustup`, so the
/// spawned process runs as bare `rustup` (prints its own CLI help and exits)
/// instead of proxying to rust-analyzer. `which::which` and the explicit
/// absolute-path branch above already resolve to a real, executable file;
/// this only needs to make the path absolute, not canonical.
fn absolute_string(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::absolute(path)
            .map_err(|err| Error::new(format!("Failed to resolve {}: {err}", path.display())))?
    };
    absolute
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("Path is not valid UTF-8"))
}

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod base_tests;
