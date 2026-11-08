use std::path::{Path, PathBuf};

pub(crate) fn command_resolves_to_executable(command: &str) -> bool {
    which_on_path(command)
        .map(|path| is_executable_path(&path))
        .unwrap_or(false)
}

/// `command` as `which` finds it on the `PATH` discovery reads
/// ([`crate::process_env`], the source route resolution keys on), the one
/// lookup of a program by name: `which::which` reads the process
/// environment itself.
pub(crate) fn which_on_path(command: impl AsRef<std::ffi::OsStr>) -> which::Result<PathBuf> {
    let paths = crate::process_env::var_os("PATH");
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
mod tests {
    use super::*;

    #[test]
    fn rejected_shell_matches_common_wrappers_by_file_name() {
        for shell in ["sh", "/bin/bash", "powershell.exe", "pwsh"] {
            assert!(is_rejected_shell(shell), "{shell}");
        }
        assert!(!is_rejected_shell("rust-analyzer"));
    }

    #[test]
    fn has_path_separator_detects_unix_and_windows_paths() {
        assert!(has_path_separator("bin/server"));
        assert!(has_path_separator("bin\\server"));
        assert!(!has_path_separator("server"));
    }
}
