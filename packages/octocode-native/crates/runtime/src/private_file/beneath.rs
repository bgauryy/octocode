#[cfg(not(any(unix, windows)))]
use super::open_no_follow;
#[cfg(windows)]
use super::windows;
use super::{names_beneath, not_regular};
use std::ffi::OsStr;
#[cfg(not(any(unix, windows)))]
use std::fs;
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, Write};
#[cfg(unix)]
use std::path::Component;
use std::path::Path;
#[cfg(not(any(unix, windows)))]
use std::path::PathBuf;

/// A directory at or below a boundary, reached without following any
/// symlink, whose entries are then opened, created, renamed and removed by
/// name relative to it. On Unix it holds the directory's descriptor, so a
/// path component swapped for a link afterwards cannot redirect those
/// operations: they stay on the directory that was walked.
pub(crate) struct BeneathDir {
    #[cfg(unix)]
    directory: File,
    /// Windows retains the walked chain and executes child operations relative
    /// to its final handle; sharing flags additionally prevent renames.
    #[cfg(windows)]
    guards: Vec<File>,
    #[cfg(not(any(unix, windows)))]
    directory: PathBuf,
}

impl BeneathDir {
    /// The directory `path` (`boundary` itself or beneath it, no `..`).
    pub(crate) fn open(boundary: &Path, path: &Path) -> io::Result<Self> {
        Self::walk(boundary, &names_beneath(boundary, path)?)
    }

    /// Create missing descendants and hold the final directory without following
    /// links at any descendant component.
    pub(crate) fn create_all(boundary: &Path, path: &Path) -> io::Result<Self> {
        let names = names_beneath(boundary, path)?;
        let mut directory = Self::open(boundary, boundary)?;
        for name in names {
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                let component = c_name(name)?;
                // SAFETY: the held descriptor is a directory and component is
                // one NUL-terminated name. Existing entries are opened no-follow.
                if unsafe {
                    libc::mkdirat(directory.directory.as_raw_fd(), component.as_ptr(), 0o700)
                } != 0
                {
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::AlreadyExists {
                        return Err(error);
                    }
                }
                directory.directory = open_at(
                    &directory.directory,
                    name,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                    0,
                )?;
            }
            #[cfg(windows)]
            {
                let child = windows::open_child_directory(
                    directory.guards.last().ok_or_else(not_regular)?,
                    name,
                    true,
                )?;
                directory.guards.push(child);
            }
            #[cfg(not(any(unix, windows)))]
            {
                let child = directory.directory.join(name);
                match fs::create_dir(&child) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                if !fs::symlink_metadata(&child)?.is_dir() {
                    return Err(not_regular());
                }
                directory.directory = child;
            }
        }
        Ok(directory)
    }

    /// Open or create a regular writable lock file without following a leaf link.
    pub(crate) fn open_lock(&self, name: &OsStr) -> io::Result<File> {
        #[cfg(unix)]
        let file = match open_at(
            &self.directory,
            name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NONBLOCK,
            0o600,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                open_at(&self.directory, name, libc::O_RDWR | libc::O_NONBLOCK, 0)?
            }
            Err(error) => return Err(error),
        };
        #[cfg(windows)]
        let file = windows::open_child_file(
            self.guards.last().ok_or_else(not_regular)?,
            name,
            windows::FileOpen::Lock,
        )?;
        #[cfg(not(any(unix, windows)))]
        let file = open_no_follow(&self.directory.join(name), true)?;
        if !file.metadata()?.is_file() {
            return Err(not_regular());
        }
        Ok(file)
    }

    /// Publish complete bytes relative to the held directory. Renaming replaces
    /// a leaf link itself, never writes to its target.
    pub(crate) fn write_atomic(&self, name: &OsStr, bytes: &[u8]) -> io::Result<()> {
        let temporary = octocode_io::atomic::unique_sibling(Path::new(name), "tmp")?;
        let temporary = temporary.file_name().ok_or_else(not_regular)?;
        let result = (|| {
            let mut file = self.create_new(temporary, 0o600)?;
            file.write_all(bytes)?;
            drop(file);
            self.rename(temporary, name)
        })();
        if result.is_err() {
            let _ = self.remove_if_present(temporary);
        }
        result
    }

    /// Whether walking from `boundary` to `path` again still reaches this
    /// directory. Unix compares device/inode; Windows compares handle identity.
    pub(crate) fn still_at(&self, boundary: &Path, path: &Path) -> io::Result<bool> {
        let again = Self::open(boundary, path)?;
        #[cfg(unix)]
        let same = {
            use std::os::unix::fs::MetadataExt;
            let (now, held) = (again.directory.metadata()?, self.directory.metadata()?);
            now.dev() == held.dev() && now.ino() == held.ino()
        };
        #[cfg(windows)]
        let same = windows::identity(self.guards.last().ok_or_else(not_regular)?)?
            == windows::identity(again.guards.last().ok_or_else(not_regular)?)?;
        #[cfg(not(any(unix, windows)))]
        let same = {
            let _ = again;
            true
        };
        Ok(same)
    }

    /// Create the entry `name` exclusively, never through a symlink, with at
    /// most the owner bits of `mode` (Unix).
    pub(crate) fn create_new(&self, name: &OsStr, mode: u32) -> io::Result<File> {
        #[cfg(unix)]
        let file = open_at(
            &self.directory,
            name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            (mode & 0o600) as libc::mode_t,
        );
        #[cfg(windows)]
        let file = {
            let _ = mode;
            windows::open_child_file(
                self.guards.last().ok_or_else(not_regular)?,
                name,
                windows::FileOpen::Create,
            )
        };
        #[cfg(not(any(unix, windows)))]
        let file = octocode_io::atomic::create_new(&self.directory.join(name), mode);
        file
    }

    /// Open the regular file `name` for reading without following a link;
    /// nonblocking, so a FIFO planted there fails instead of hanging.
    pub(crate) fn open_file(&self, name: &OsStr) -> io::Result<File> {
        #[cfg(unix)]
        let file = open_at(&self.directory, name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        #[cfg(windows)]
        let file = windows::open_child_file(
            self.guards.last().ok_or_else(not_regular)?,
            name,
            windows::FileOpen::Read,
        )?;
        #[cfg(not(any(unix, windows)))]
        let file = open_no_follow(&self.directory.join(name), false)?;
        if !file.metadata()?.is_file() {
            return Err(not_regular());
        }
        Ok(file)
    }

    /// Rename the entry `from` to `to`, both in this directory, replacing
    /// `to`.
    pub(crate) fn rename(&self, from: &OsStr, to: &OsStr) -> io::Result<()> {
        #[cfg(unix)]
        let result = {
            use std::os::fd::AsRawFd;
            let (from, to) = (c_name(from)?, c_name(to)?);
            let fd = self.directory.as_raw_fd();
            // SAFETY: `fd` is an open directory and both names are single
            // NUL-terminated path components.
            if unsafe { libc::renameat(fd, from.as_ptr(), fd, to.as_ptr()) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        };
        #[cfg(windows)]
        let result = windows::rename(self.guards.last().ok_or_else(not_regular)?, from, to);
        #[cfg(not(any(unix, windows)))]
        let result = fs::rename(self.directory.join(from), self.directory.join(to));
        result
    }

    /// Remove the entry `name` (the link itself if it is one); an absent
    /// entry is `Ok`.
    pub(crate) fn remove_if_present(&self, name: &OsStr) -> io::Result<()> {
        #[cfg(unix)]
        let result = {
            use std::os::fd::AsRawFd;
            let name = c_name(name)?;
            // SAFETY: an open directory and one NUL-terminated component.
            if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        };
        #[cfg(windows)]
        let result = windows::remove(self.guards.last().ok_or_else(not_regular)?, name);
        #[cfg(not(any(unix, windows)))]
        let result = fs::remove_file(self.directory.join(name));
        match result {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// Unix: each component is an `openat` relative to its parent's
    /// descriptor with `O_NOFOLLOW`, so no path is resolved twice.
    #[cfg(unix)]
    pub(super) fn walk(boundary: &Path, names: &[&OsStr]) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY)
            .open(boundary)?;
        for name in names {
            directory = open_at(&directory, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        }
        Ok(Self { directory })
    }

    /// Windows pins each no-reparse component until all operations finish.
    #[cfg(windows)]
    pub(super) fn walk(boundary: &Path, names: &[&OsStr]) -> io::Result<Self> {
        let mut guards = vec![windows::open_directory(boundary)?];
        for name in names {
            let child =
                windows::open_child_directory(guards.last().ok_or_else(not_regular)?, name, false)?;
            guards.push(child);
        }
        Ok(Self { guards })
    }

    /// Other targets retain the checked-path fallback.
    #[cfg(not(any(unix, windows)))]
    pub(super) fn walk(boundary: &Path, names: &[&OsStr]) -> io::Result<Self> {
        let mut directory = boundary.to_path_buf();
        for name in names {
            directory.push(name);
            if !fs::symlink_metadata(&directory)?.is_dir() {
                return Err(not_regular());
            }
        }
        Ok(Self { directory })
    }
}

#[cfg(unix)]
fn c_name(name: &OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(component)) if component == name)
        || components.next().is_some()
    {
        return Err(not_regular());
    }
    std::ffi::CString::new(name.as_bytes()).map_err(|_| not_regular())
}

/// `openat(directory, name)` that never follows a link at `name`; a link or
/// non-directory where a directory is required is `InvalidInput`.
#[cfg(unix)]
fn open_at(
    directory: &File,
    name: &OsStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = c_name(name)?;
    // SAFETY: `directory` is an open descriptor and `name` is one
    // NUL-terminated path component; `mode` is read only with `O_CREAT`.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            libc::c_uint::from(mode),
        )
    };
    if fd < 0 {
        let error = io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(libc::ELOOP | libc::ENOTDIR) => not_regular(),
            _ => error,
        });
    }
    // SAFETY: `fd` is a fresh descriptor that nothing else owns.
    Ok(unsafe { File::from_raw_fd(fd) })
}
