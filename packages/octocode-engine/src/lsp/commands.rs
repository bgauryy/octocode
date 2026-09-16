use std::path::Path;

pub(crate) fn command_resolves_to_executable(command: &str) -> bool {
    which::which(command)
        .map(|path| is_executable_path(&path))
        .unwrap_or(false)
}

pub(crate) fn has_path_separator(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

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
