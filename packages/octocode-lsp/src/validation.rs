use napi::{Error, Result, Status};
use std::path::{Path, PathBuf};

pub fn safe_read_file(file_path: String) -> Result<String> {
    let path = Path::new(&file_path);
    if !path.is_absolute() {
        return Err(Error::new(
            Status::InvalidArg,
            format!("File path must be absolute: {file_path}"),
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to resolve {file_path}: {err}"),
        )
    })?;
    let metadata = std::fs::metadata(&canonical).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to inspect {}: {err}", canonical.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(Error::new(
            Status::InvalidArg,
            format!("Path is not a regular file: {}", canonical.display()),
        ));
    }
    std::fs::read_to_string(&canonical).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {}: {err}", canonical.display()),
        )
    })
}

pub fn validate_lsp_server_path(command: String) -> Result<String> {
    if command.trim().is_empty() {
        return Err(Error::new(
            Status::InvalidArg,
            "Language server command is required",
        ));
    }
    if is_rejected_shell(&command) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("Shell wrapper commands are not allowed: {command}"),
        ));
    }

    let command_path = Path::new(&command);
    if command_path.is_absolute() || has_path_separator(&command) {
        if !command_path.exists() {
            return Err(Error::new(
                Status::InvalidArg,
                format!("Language server path does not exist: {command}"),
            ));
        }
        if !is_executable_path(command_path) {
            return Err(Error::new(
                Status::InvalidArg,
                format!("Language server path is not executable: {command}"),
            ));
        }
        return canonical_string(command_path);
    }

    let resolved = which::which(&command).map_err(|err| {
        Error::new(
            Status::InvalidArg,
            format!("Language server command not found: {command}: {err}"),
        )
    })?;
    if !is_executable_path(&resolved) {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "Language server command is not executable: {}",
                resolved.display()
            ),
        ));
    }
    canonical_string(&resolved)
}

fn canonical_string(path: &Path) -> Result<String> {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(Status::InvalidArg, "Path is not valid UTF-8"))
}

fn has_path_separator(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

fn is_rejected_shell(command: &str) -> bool {
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

fn is_executable_path(path: &Path) -> bool {
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
    use std::fs;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("octocode_lsp_validation_{name}_{}", nanos))
    }

    // ── safe_read_file ────────────────────────────────────────────────────────

    #[test]
    fn safe_read_file_rejects_relative_path() {
        let result = safe_read_file("relative/path.txt".to_owned());
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("must be absolute"));
    }

    #[test]
    fn safe_read_file_rejects_nonexistent_path() {
        let path = temp_path("nonexistent");
        let result = safe_read_file(path.to_string_lossy().into_owned());
        assert!(result.is_err());
    }

    #[test]
    fn safe_read_file_rejects_directory() {
        let dir = temp_path("dir");
        fs::create_dir_all(&dir).expect("create dir");
        let result = safe_read_file(dir.to_string_lossy().into_owned());
        let _ = fs::remove_dir(&dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("not a regular file"));
    }

    #[test]
    fn safe_read_file_reads_content_of_existing_file() {
        let path = temp_path("readable");
        fs::write(&path, b"hello octocode").expect("write fixture");
        let result = safe_read_file(path.to_string_lossy().into_owned());
        let _ = fs::remove_file(&path);
        assert_eq!(result.expect("safe_read_file"), "hello octocode");
    }

    // ── validate_lsp_server_path ──────────────────────────────────────────────

    #[test]
    fn validate_lsp_server_path_rejects_empty_string() {
        assert!(validate_lsp_server_path(String::new()).is_err());
        assert!(validate_lsp_server_path("   ".to_owned()).is_err());
    }

    #[test]
    fn validate_lsp_server_path_rejects_shell_wrappers() {
        for shell in ["sh", "bash", "zsh", "fish", "cmd", "powershell", "pwsh"] {
            let result = validate_lsp_server_path(shell.to_owned());
            assert!(
                result.is_err(),
                "shell '{shell}' must be rejected but was accepted"
            );
        }
    }

    #[test]
    fn validate_lsp_server_path_rejects_nonexistent_absolute_path() {
        let path = temp_path("nonexistent_server");
        let result = validate_lsp_server_path(path.to_string_lossy().into_owned());
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn validate_lsp_server_path_rejects_non_executable_file() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("nonexec");
        fs::write(&path, b"#!/bin/sh").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        let result = validate_lsp_server_path(path.to_string_lossy().into_owned());
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("not executable"));
    }

    #[cfg(unix)]
    #[test]
    fn validate_lsp_server_path_accepts_executable_absolute_path() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("server_exec");
        fs::write(&path, b"#!/bin/sh\necho ok\n").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        let result = validate_lsp_server_path(path.to_string_lossy().into_owned());
        let _ = fs::remove_file(&path);
        assert!(result.is_ok(), "executable file must be accepted: {:?}", result);
    }
}
