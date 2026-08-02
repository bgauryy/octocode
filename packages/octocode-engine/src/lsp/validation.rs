use crate::lsp::commands::{has_path_separator, is_executable_path, is_rejected_shell};
use crate::lsp::uri::uri_to_path;
use napi::{Error, Result, Status};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const MAX_SAFE_READ_FILE_BYTES: u64 = 1_000_000;

fn normalize_path_or_uri(path_or_uri: &str) -> Result<String> {
    if path_or_uri.starts_with("file://") {
        return uri_to_path(path_or_uri);
    }
    Ok(path_or_uri.to_owned())
}

fn canonical_regular_file(path_or_uri: &str) -> Result<(PathBuf, std::fs::Metadata)> {
    let normalized = normalize_path_or_uri(path_or_uri)?;
    let path = Path::new(&normalized);
    if !path.is_absolute() {
        return Err(Error::new(
            Status::InvalidArg,
            format!("File path must be absolute: {path_or_uri}"),
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to resolve {path_or_uri}: {err}"),
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
    Ok((canonical, metadata))
}

pub fn safe_read_file(file_path: String) -> Result<String> {
    let (canonical, metadata) = canonical_regular_file(&file_path)?;
    if metadata.len() > MAX_SAFE_READ_FILE_BYTES {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "File is too large for safe LSP context read: {} ({} bytes > {} bytes)",
                canonical.display(),
                metadata.len(),
                MAX_SAFE_READ_FILE_BYTES
            ),
        ));
    }
    std::fs::read_to_string(&canonical).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {}: {err}", canonical.display()),
        )
    })
}

pub fn safe_read_line_window(
    file_path: String,
    line_zero_based: u32,
    context_lines: u32,
) -> Result<String> {
    let (canonical, metadata) = canonical_regular_file(&file_path)?;
    // Mirror safe_read_file's cap: BufReader::lines() pulls a whole physical
    // line into a String, so a single-huge-line file (minified/generated
    // bundle) would otherwise allocate unbounded memory. Reject before reading.
    if metadata.len() > MAX_SAFE_READ_FILE_BYTES {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "File is too large for safe LSP context read: {} ({} bytes > {} bytes)",
                canonical.display(),
                metadata.len(),
                MAX_SAFE_READ_FILE_BYTES
            ),
        ));
    }
    let start = line_zero_based.saturating_sub(context_lines) as usize;
    let end = line_zero_based.saturating_add(context_lines) as usize;
    let file = std::fs::File::open(&canonical).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {}: {err}", canonical.display()),
        )
    })?;
    let mut out = Vec::new();
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        if idx > end {
            break;
        }
        if idx < start {
            continue;
        }
        let line = line.map_err(|err| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to read {}: {err}", canonical.display()),
            )
        })?;
        out.push((idx, line));
    }
    Ok(out
        .into_iter()
        .map(|(idx, line)| {
            let line_number = idx + 1;
            let is_target = idx == line_zero_based as usize;
            format!(
                "{}{:4}| {}",
                if is_target { '>' } else { ' ' },
                line_number,
                line
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
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
        return absolute_string(command_path);
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
        std::path::absolute(path).map_err(|err| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to resolve {}: {err}", path.display()),
            )
        })?
    };
    absolute
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(Status::InvalidArg, "Path is not valid UTF-8"))
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
        assert!(result
            .expect_err("relative path must be rejected")
            .reason
            .contains("must be absolute"));
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
        assert!(result
            .expect_err("directory must be rejected")
            .reason
            .contains("not a regular file"));
    }

    #[test]
    fn safe_read_file_reads_content_of_existing_file() {
        let path = temp_path("readable");
        fs::write(&path, b"hello octocode").expect("write fixture");
        let result = safe_read_file(path.to_string_lossy().into_owned());
        let _ = fs::remove_file(&path);
        assert_eq!(result.expect("safe_read_file"), "hello octocode");
    }

    #[test]
    fn safe_read_file_rejects_oversized_file() {
        let path = temp_path("oversized");
        fs::write(&path, vec![b'a'; (MAX_SAFE_READ_FILE_BYTES + 1) as usize])
            .expect("write fixture");
        let result = safe_read_file(path.to_string_lossy().into_owned());
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        assert!(result
            .expect_err("oversized file must be rejected")
            .reason
            .contains("too large"));
    }

    #[test]
    fn safe_read_line_window_accepts_file_uri_and_reads_only_window() {
        let path = temp_path("window");
        fs::write(&path, b"one\ntwo\nthree\nfour\n").expect("write fixture");
        let uri = crate::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri");
        let result = safe_read_line_window(uri, 2, 1).expect("line window");
        let _ = fs::remove_file(&path);
        assert!(result.contains("   2| two"), "{result}");
        assert!(result.contains(">   3| three"), "{result}");
        assert!(result.contains("   4| four"), "{result}");
        assert!(!result.contains("one"), "{result}");
    }

    #[test]
    fn safe_read_line_window_rejects_oversized_file() {
        // A single physical line larger than the cap would otherwise be pulled
        // fully into memory by BufReader::lines(); the size guard (mirroring
        // safe_read_file) must reject it before any read.
        let path = temp_path("window_oversized");
        fs::write(&path, vec![b'a'; (MAX_SAFE_READ_FILE_BYTES + 1) as usize])
            .expect("write fixture");
        let result = safe_read_line_window(path.to_string_lossy().into_owned(), 0, 0);
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        assert!(result
            .expect_err("oversized file must be rejected")
            .reason
            .contains("too large"));
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
        assert!(result
            .expect_err("non-executable file must be rejected")
            .reason
            .contains("not executable"));
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
        use std::os::unix::fs::{symlink, PermissionsExt};
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
