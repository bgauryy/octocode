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
