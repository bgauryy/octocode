use crate::error::{Error, Result};
use crate::lsp::commands::{has_path_separator, is_executable_path, is_rejected_shell};
use std::path::Path;

pub fn validate_lsp_server_path(command: String) -> Result<String> {
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

    let resolved = which::which(&command).map_err(|err| {
        Error::new(format!(
            "Language server command not found: {command}: {err}"
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
        assert!(
            result
                .expect_err("non-executable file must be rejected")
                .reason
                .contains("not executable")
        );
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
        assert!(
            result.is_ok(),
            "executable file must be accepted: {:?}",
            result
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_lsp_server_path_preserves_a_symlinked_proxy_binary_name() {
        // A rustup-style toolchain proxy is a symlink whose target dispatches
        // on argv[0]'s basename (e.g. `rust-analyzer` -> `rustup`, which then
        // decides to run rust-analyzer only because it was invoked *as*
        // `rust-analyzer`). Canonicalizing here would resolve the symlink to
        // `.../rustup` and silently break that dispatch — the validated path
        // must keep the original (symlink) filename, not the resolved target.
        use std::os::unix::fs::{PermissionsExt, symlink};
        let target = temp_path("proxy_target");
        fs::write(&target, b"#!/bin/sh\necho ok\n").expect("write target");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("chmod");
        let link = temp_path("rust-analyzer");
        symlink(&target, &link).expect("symlink");

        let result = validate_lsp_server_path(link.to_string_lossy().into_owned());
        let _ = fs::remove_file(&target);
        let _ = fs::remove_file(&link);

        let resolved = result.expect("symlinked executable must be accepted");
        assert_eq!(
            Path::new(&resolved).file_name(),
            link.file_name(),
            "expected the symlink's own name to be preserved, got: {resolved}"
        );
    }
}
