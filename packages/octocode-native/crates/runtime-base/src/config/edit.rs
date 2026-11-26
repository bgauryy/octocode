//! Private configuration files share bounded reads, revision checks, and atomic writes.
use super::dotenv::dotenv_may_set;
use super::parse_env;
use crate::private_file::{open_no_follow, read_limited};
use octocode_io::atomic::write_atomic;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
pub(super) const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
/// How long an edit waits for the config lock before reporting it busy. A
/// lock can stay held briefly after its owner released it: a child forked
/// by any thread carries a copy of the descriptor until it execs.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn changed() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "Configuration changed; refresh before saving.",
    )
}
/// A configuration file's text; `None` when absent. The final path component
/// must be a regular file (no symlink, FIFO, or directory) of at most 4 MiB.
pub fn read_private_config(path: &Path) -> io::Result<Option<String>> {
    let file = match open_no_follow(path, false) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    String::from_utf8(read_limited(file, MAX_CONFIG_BYTES)?)
        .map(Some)
        .map_err(|_| invalid("Configuration is not UTF-8 text."))
}
/// Content identity of a configuration file, checked before every replace.
pub fn config_revision(text: Option<&str>) -> String {
    match text {
        None => "missing".into(),
        Some(text) => format!(
            "sha256:{}",
            octocode_engine::digest::sha256(text.as_bytes())
        ),
    }
}
/// The one rolling backup of a file Octocode does not own: `.{name}.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    path.with_file_name(format!(".{name}.bak"))
}
/// Replace `path` with `body` when its content still has `expected_revision`;
/// returns whether bytes changed and, with `backup`, the rolling backup that
/// now holds the previous content. Octocode's own files take no backup, so a
/// removed secret does not survive in an old copy.
/// Cooperating writers serialize on a lock file; noncooperating writers can
/// still race the final rename, so the revision is rechecked just before it.
pub fn replace_private_config(
    path: &Path,
    expected_revision: &str,
    body: &str,
    backup: bool,
) -> io::Result<(bool, Option<PathBuf>)> {
    if body.len() as u64 > MAX_CONFIG_BYTES {
        return Err(invalid("Configuration exceeds 4 MiB."));
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Configuration needs a parent directory."))?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    let lock = open_no_follow(
        &path.with_file_name(format!(
            "{}.lock",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("config")
        )),
        true,
    )?;
    let _lock = match octocode_io::lock::lock_within(lock, LOCK_WAIT, octocode_io::lock::LOCK_POLL)
    {
        Ok(lock) => lock,
        Err(octocode_io::lock::LockError::Busy) => {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Configuration is being edited; retry.",
            ));
        }
        Err(octocode_io::lock::LockError::Io(error)) => return Err(error),
    };
    let original = read_private_config(path)?;
    if config_revision(original.as_deref()) != expected_revision {
        return Err(changed());
    }
    if original.as_deref() == Some(body) {
        return Ok((false, None));
    }
    let saved_backup = match original.as_deref().filter(|_| backup) {
        Some(original) => {
            let backup_path = backup_path(path);
            write_atomic(&backup_path, original.as_bytes(), true)?;
            Some(backup_path)
        }
        None => None,
    };
    if config_revision(read_private_config(path)?.as_deref()) != expected_revision {
        return Err(changed());
    }
    write_atomic(path, body.as_bytes(), true)?;
    Ok((true, saved_backup))
}
pub fn validate_env_edit(key: &str, value: Option<&str>, workspace: bool) -> io::Result<()> {
    let key = super::canonical_env_name(key);
    if key.is_empty()
        || !key
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
    {
        return Err(invalid("Key must match [A-Za-z_][A-Za-z0-9_]*."));
    }
    let Some(value) = value else { return Ok(()) };
    if !dotenv_may_set(key, value, workspace) {
        return Err(invalid(
            "This key cannot be loaded from this .env scope; use the process environment.",
        ));
    }
    if value.contains(['\n', '\r', '\0']) {
        return Err(invalid(
            "Values must be a single line without NUL characters.",
        ));
    }
    super::manage::validate_env_value(key, value).map_err(|e| invalid(&e))
}
/// Set (`Some`) or remove (`None`) one key in a `.env` file; returns whether
/// bytes changed. `revision` defaults to the file's current content.
pub fn edit_scoped_env(
    path: &Path,
    key: &str,
    value: Option<&str>,
    workspace: bool,
    revision: Option<&str>,
) -> io::Result<bool> {
    // An earlier name of a setting's variable is saved under its current one,
    // replacing a line that still spells the earlier name.
    let key = super::canonical_env_name(key);
    validate_env_edit(key, value, workspace)?;
    let original = read_private_config(path)?;
    let newline = if original.as_deref().unwrap_or("").contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated = String::new();
    let mut found = false;
    for line in original.as_deref().unwrap_or("").split_inclusive('\n') {
        if parse_env(Some(line))
            .keys()
            .any(|name| super::canonical_env_name(name) == key)
        {
            if !found && let Some(value) = value {
                updated.push_str(&format!("{key}=\"{value}\"{newline}"));
            }
            found = true;
        } else {
            updated.push_str(line);
        }
    }
    if !found && let Some(value) = value {
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&format!("{key}=\"{value}\"{newline}"));
    }
    let current = config_revision(original.as_deref());
    if original.is_none() && value.is_none() {
        return match revision {
            Some(revision) if revision != current => Err(changed()),
            _ => Ok(false),
        };
    }
    replace_private_config(path, revision.unwrap_or(&current), &updated, false)
        .map(|(changed, _)| changed)
}

#[cfg(test)]
mod tests;
