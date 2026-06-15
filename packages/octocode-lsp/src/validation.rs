use napi::{Error, Result, Status};
use std::path::Path;

pub fn safe_read_file(file_path: String) -> Result<String> {
    std::fs::read_to_string(&file_path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {file_path}: {err}"),
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
    if Path::new(&command).is_absolute() && !Path::new(&command).exists() {
        return Err(Error::new(
            Status::InvalidArg,
            format!("Language server path does not exist: {command}"),
        ));
    }
    Ok(command)
}
