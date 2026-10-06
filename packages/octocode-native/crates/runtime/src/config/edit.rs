//! Private configuration files share bounded reads, revision checks, and atomic writes.
use super::dotenv::dotenv_may_set;
use super::parse_env;
use crate::private_file::{open_no_follow, read_limited, write_atomic};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
pub(super) const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
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
        Some(text) => format!("sha256:{}", hex::encode(Sha256::digest(text.as_bytes()))),
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
    lock.try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "Configuration is being edited; retry.",
        )
    })?;
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
        if parse_env(Some(line)).contains_key(key) {
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
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(root.path()).unwrap().join(".env");
        (root, path)
    }
    #[test]
    fn revision_backup_and_permissions() {
        let (_root, path) = fixture();
        assert!(
            replace_private_config(&path, "missing", "KEY=one\n", true)
                .unwrap()
                .0
        );
        assert!(replace_private_config(&path, "missing", "KEY=two\n", true).is_err());
        let revision = config_revision(read_private_config(&path).unwrap().as_deref());
        let (changed, backup) =
            replace_private_config(&path, &revision, "KEY=two\n", true).unwrap();
        assert!(changed);
        let backup = backup.unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "KEY=one\n");
        let revision = config_revision(Some("KEY=two\n"));
        assert_eq!(
            replace_private_config(&path, &revision, "KEY=three\n", true)
                .unwrap()
                .1,
            Some(backup.clone())
        );
        assert_eq!(fs::read_to_string(&backup).unwrap(), "KEY=two\n");
        let backups = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".bak")
            })
            .count();
        assert_eq!(backups, 1, "one rolling backup, never one per save");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_and_retains_directory_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (_root, path) = fixture();
        let parent = path.parent().unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
        replace_private_config(&path, "missing", "KEY=one\n", false).unwrap();
        assert_eq!(
            fs::metadata(parent).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let link = parent.join("linked.env");
        symlink(&path, &link).unwrap();
        assert!(read_private_config(&link).is_err());
        assert!(replace_private_config(&link, "missing", "KEY=two\n", false).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "KEY=one\n");
    }
    /// A symlinked directory above the file (macOS `/tmp`, dotfile managers)
    /// is followed; only the file itself must not be a symlink.
    #[cfg(unix)]
    #[test]
    fn symlinked_parent_directory_is_readable_and_writable() {
        let (_root, path) = fixture();
        let real = path.parent().unwrap().join("real");
        fs::create_dir(&real).unwrap();
        let linked = path.parent().unwrap().join("linked");
        std::os::unix::fs::symlink(&real, &linked).unwrap();
        let file = linked.join(".env");
        assert!(edit_scoped_env(&file, "CUSTOM_KEY", Some("one"), false, None).unwrap());
        assert_eq!(
            parse_env(read_private_config(&file).unwrap().as_deref())["CUSTOM_KEY"],
            "one"
        );
        assert!(real.join(".env").is_file());
    }
    #[test]
    fn owned_env_edits_leave_no_backup_of_removed_secrets() {
        let (_root, path) = fixture();
        edit_scoped_env(&path, "CUSTOM_KEY", Some("secret-one"), false, None).unwrap();
        edit_scoped_env(&path, "CUSTOM_KEY", None, false, None).unwrap();
        for entry in fs::read_dir(path.parent().unwrap()).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !fs::read_to_string(entry.path())
                    .unwrap_or_default()
                    .contains("secret-one"),
                "{:?}",
                entry.file_name()
            );
        }
    }
    #[test]
    fn scoped_policy_and_roundtrip_quotes() {
        let (_root, path) = fixture();
        assert!(edit_scoped_env(&path, "PATH", Some("bad"), false, Some("missing")).is_err());
        assert!(
            edit_scoped_env(
                &path,
                "OCTOCODE_STORAGE_MODE",
                Some("persistent"),
                true,
                Some("missing")
            )
            .is_err()
        );
        assert!(
            edit_scoped_env(
                &path,
                "OCTOCODE_STORAGE_MODE",
                Some("memory"),
                true,
                Some("missing")
            )
            .unwrap()
        );
        let revision = config_revision(read_private_config(&path).unwrap().as_deref());
        let value = "quoted\"value'\\literal";
        edit_scoped_env(&path, "CUSTOM_KEY", Some(value), false, Some(&revision)).unwrap();
        assert_eq!(
            parse_env(read_private_config(&path).unwrap().as_deref())["CUSTOM_KEY"],
            value
        );
    }
}
