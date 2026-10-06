//! Owner-only files: no-follow opens, bounded reads, and atomic replacement.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Open a regular file without following a final symlink. Nonblocking, so a
/// FIFO planted at `path` fails instead of hanging; `create` makes it 0600.
pub(crate) fn open_no_follow(path: &Path, create: bool) -> io::Result<File> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(not_regular());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    if create {
        options.write(true).create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(not_regular());
    }
    Ok(file)
}

fn not_regular() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Expected a regular file, not a symlink, directory, or device.",
    )
}

/// Read at most `limit` bytes; a larger input is an error, never a prefix.
pub(crate) fn read_limited(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("File exceeds {} KiB.", limit / 1024),
        ));
    }
    Ok(bytes)
}

/// Create `path` exclusively, never through a symlink, with at most the
/// owner bits of `mode` (Unix), so no other user can read it from its first
/// byte.
pub(crate) fn create_new(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode & 0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

/// Replace `path` with `bytes` through an owner-only temporary file in the
/// same directory, so readers see the old or the new content, never a mix.
/// `durable` syncs the file before the rename and the directory after it.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    let temporary = unique_sibling(path, "tmp")?;
    let result = (|| {
        let mut file = create_new(&temporary, 0o600)?;
        file.write_all(bytes)?;
        if durable {
            file.sync_all()?;
        }
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    // The rename already happened; a directory that cannot be synced does not
    // undo it, so this step is best effort.
    #[cfg(unix)]
    if durable && let Some(parent) = path.parent() {
        let _ = File::open(parent).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

/// A hidden, unpredictable sibling name: `.{name}-{128-bit hex}.{suffix}`.
fn unique_sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
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
    fn atomic_write_replaces_owner_only_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, "old").unwrap();
        for durable in [false, true] {
            write_atomic(&path, b"new", durable).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn bounded_read_rejects_oversize_instead_of_truncating() {
        assert_eq!(read_limited(&b"abcd"[..], 4).unwrap(), b"abcd");
        assert!(read_limited(&b"abcde"[..], 4).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn no_follow_open_rejects_symlinks_and_fifos() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, "x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(open_no_follow(&link, false).is_err());
        assert!(open_no_follow(&link, true).is_err());
        let fifo = dir.path().join("fifo");
        let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(open_no_follow(&fifo, false).is_err());
        assert!(open_no_follow(&target, false).is_ok());
    }
}
