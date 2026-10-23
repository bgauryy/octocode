//! Owner-only files: no-follow opens, bounded reads, and atomic replacement.
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
#[cfg(not(any(unix, windows)))]
use std::path::PathBuf;
use std::path::{Component, Path};

mod beneath;
#[cfg(windows)]
mod windows;
pub(crate) use beneath::BeneathDir;

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
    #[cfg(windows)]
    windows::no_follow(&mut options);
    let file = options.open(path)?;
    #[cfg(windows)]
    windows::reject_reparse(&file)?;
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

/// Open the regular file `path` for reading by walking down from `boundary`
/// one component at a time, never following a symlink, so a file or
/// directory swapped for a link after an earlier check cannot redirect the
/// read outside `boundary`. `path` must lie beneath `boundary` with no `..`;
/// a symlink or non-file anywhere below `boundary` is `InvalidInput`.
pub(crate) fn open_beneath(boundary: &Path, path: &Path) -> io::Result<File> {
    let names = names_beneath(boundary, path)?;
    let Some((file_name, directories)) = names.split_last() else {
        return Err(not_regular());
    };
    BeneathDir::walk(boundary, directories)?.open_file(file_name)
}

/// The components of `path` below `boundary`; `..`, `.` or a root is
/// `InvalidInput`.
fn names_beneath<'a>(boundary: &Path, path: &'a Path) -> io::Result<Vec<&'a OsStr>> {
    let relative = path.strip_prefix(boundary).map_err(|_| not_regular())?;
    relative
        .components()
        .map(|component| match component {
            Component::Normal(name) => Ok(name),
            _ => Err(not_regular()),
        })
        .collect()
}

/// Read at most `limit` bytes; a larger input is an error, never a prefix.
pub(crate) fn read_limited(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    octocode_io::read::read_limited(reader, limit)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("File exceeds {} KiB.", limit / 1024),
        )
    })
}

#[cfg(test)]
mod tests;
