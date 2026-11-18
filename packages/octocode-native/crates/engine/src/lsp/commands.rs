use crate::process_env::EnvMap;
use std::path::{Path, PathBuf};

pub(crate) fn command_resolves_to_executable(command: &str, env: &impl EnvMap) -> bool {
    which_on_path(command, env)
        .map(|path| is_executable_path(&path))
        .unwrap_or(false)
}

/// `command` as `which` finds it on the call's `PATH` (`env`, read through
/// [`crate::process_env::call_var`]), the one lookup of a program by name:
/// `which::which` would read the process environment itself.
pub(crate) fn which_on_path(
    command: impl AsRef<std::ffi::OsStr>,
    env: &impl EnvMap,
) -> which::Result<PathBuf> {
    let paths = crate::process_env::call_var(env, "PATH");
    match std::env::current_dir() {
        Ok(cwd) => which::which_in(command, paths, cwd),
        Err(_) => which::which_in_global(command, paths)?
            .next()
            .ok_or(which::Error::CannotFindBinaryPath),
    }
}

pub(crate) fn has_path_separator(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

/// A guard against a shell wrapper written by mistake (`sh -c …` as the
/// server), not a security boundary: a trusted configuration can name any
/// executable, and an untrusted one launches nothing.
pub(crate) fn is_rejected_shell(command: &str) -> bool {
    let name = Path::new(command)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
    )
}

pub(crate) fn is_executable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests;
