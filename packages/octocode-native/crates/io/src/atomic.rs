//! Owner-only file creation and atomic replacement. The one place Octocode
//! writes state files: a reader sees the old or the new content, never a mix.
//! On Unix nothing is created through a symlink or readable by other users;
//! other platforms get the atomic replacement and inherit the directory's
//! access control, since no ACLs are set here.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Create `path` (and missing parents) owner-only, `0700`, for private state.
/// An existing directory keeps its mode, because a caller may have pointed at a
/// directory the user chose; it is refused when it is a symlink, when another
/// user owns it, or when every user can write it, since that would let them
/// plant files in it.
pub fn create_private_dir_all(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        // The leaf itself, never through a link: a symlink here could point the
        // caller's private state into another directory.
        let meta = std::fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory path is a symbolic link",
            ));
        }
        // SAFETY: getuid() takes no arguments, has no preconditions, cannot fail,
        // and only reads the caller's real user id.
        let current_uid = unsafe { libc::getuid() };
        if meta.uid() != current_uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory is owned by another user",
            ));
        }
        // World-writable lets anyone plant files. Group-writable is accepted:
        // umask 002 systems give each user a private group and create 0775.
        if meta.mode() & 0o002 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory is writable by every user",
            ));
        }
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path)?;
    Ok(())
}

/// Create `path` exclusively, never through a symlink, with at most the
/// owner bits of `mode` (Unix), so no other user can read it from its first
/// byte.
pub fn create_new(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode & 0o700).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

/// Replace `path` with `bytes` through an owner-only temporary file in the
/// same directory, so readers see the old or the new content, never a mix.
/// `durable` syncs the file before the rename and the directory after it.
pub fn write_atomic(path: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    write_atomic_mode(path, bytes, 0o600, durable)
}

/// [`write_atomic`] with the owner bits in `mode`, such as `0o700` for an
/// executable installed for the owner only.
pub fn write_atomic_mode(path: &Path, bytes: &[u8], mode: u32, durable: bool) -> io::Result<()> {
    write_atomic_with(path, mode, durable, |file| file.write_all(bytes))
}

/// [`write_atomic_mode`] for content streamed by `write` into the temporary
/// file, so a large payload is never held in memory. When `write` fails the
/// temporary is removed and `path` keeps its old content.
pub fn write_atomic_with<T>(
    path: &Path,
    mode: u32,
    durable: bool,
    write: impl FnOnce(&mut File) -> io::Result<T>,
) -> io::Result<T> {
    let temporary = unique_sibling(path, "tmp")?;
    let result = (|| {
        let mut file = create_new(&temporary, mode)?;
        let written = write(&mut file)?;
        if durable {
            file.sync_all()?;
        }
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(written)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    // The rename already happened; a directory that cannot be synced does not
    // undo it, so this step is best effort.
    if durable && let Some(parent) = path.parent() {
        let _ = sync_dir(parent);
    }
    result
}

/// Flush the directory `path` to disk, so an entry created, renamed or removed
/// in it survives a crash. The one owner of this durability rule: callers map
/// the error to their own. A no-op where a directory cannot be opened for
/// syncing (non-Unix).
pub fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// A hidden, unpredictable sibling name: `.{name}-{128-bit hex}.{suffix}`.
pub fn unique_sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| io::Error::other("Cannot create a file name."))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    Ok(path.with_file_name(format!(".{name}-{}.{suffix}", hex::encode(nonce))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_syncs_and_a_missing_one_is_reported_on_unix() {
        let dir = tempfile::tempdir().unwrap();
        sync_dir(dir.path()).unwrap();
        #[cfg(unix)]
        assert!(sync_dir(&dir.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_new_private_directory_is_owner_only_and_nested_levels_too() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache").join("deep");
        create_private_dir_all(&path).unwrap();
        for level in [dir.path().join("cache"), path] {
            assert_eq!(
                std::fs::metadata(&level).unwrap().permissions().mode() & 0o077,
                0
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_directory_keeps_its_mode_when_others_cannot_write_it() {
        // A user-chosen cache directory is not ours to re-permission.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared-looking");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private_dir_all(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_group_writable_directory_from_a_umask_002_system_is_accepted() {
        // umask 002 (user-private groups) creates 0775 directories; an older
        // build left such directories behind, and they must keep working.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lsp");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o775)).unwrap();
        create_private_dir_all(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o775
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world-writable");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();
        let error = create_private_dir_all(&path).expect_err("writable by others");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_a_private_directory_path_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = create_private_dir_all(&link).expect_err("symlink");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn creating_an_existing_private_directory_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        create_private_dir_all(dir.path()).unwrap();
        create_private_dir_all(dir.path()).unwrap();
    }

    #[test]
    fn atomic_write_replaces_owner_only_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "old").unwrap();
        for durable in [false, true] {
            write_atomic(&path, b"new", durable).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn a_failed_streamed_write_keeps_the_old_file_and_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool");
        std::fs::write(&path, "old").unwrap();
        let error = write_atomic_with(&path, 0o700, false, |file| {
            file.write_all(b"partial")?;
            Err::<(), _>(io::Error::other("source ended early"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "source ended early");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let written = write_atomic_with(&path, 0o700, false, |file| {
            io::copy(&mut &b"streamed"[..], file)
        })
        .unwrap();
        assert_eq!(written, 8);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "streamed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
}
