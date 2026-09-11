//! Descriptor-relative filesystem algorithms adapted from Octocode Agent's
//! fs_service.rs (MIT). No service, host, checkpoint, or policy dependencies.
use super::{check_cancelled, FsResult, Receipt, Snapshot};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn io(error: std::io::Error) -> String {
    format!("IO_FAILURE: {error}")
}
fn last_error() -> String {
    io(std::io::Error::last_os_error())
}
fn changed() -> String {
    "PRECONDITION_FAILED: File or parent changed after preflight".into()
}
fn create_publication_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        changed()
    } else {
        io(error)
    }
}
fn too_large(maximum: usize) -> String {
    format!("TOO_LARGE: File exceeds maximum {maximum} bytes")
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
// mode_t is u16 on macOS and u32 on Linux; normalize it for stored mode values.
#[allow(clippy::unnecessary_cast)]
fn mode_bits(value: libc::mode_t) -> u32 {
    value as u32
}
fn cname(name: &str) -> FsResult<CString> {
    CString::new(name).map_err(|_| "INVALID_PATH: Path contains NUL".into())
}

fn parts(path: &str) -> FsResult<Vec<&str>> {
    if !path.starts_with('/') || path.ends_with('/') || path.len() > 32768 {
        return Err("INVALID_PATH: Expected an absolute canonical file path".into());
    }
    let values: Vec<_> = path[1..].split('/').collect();
    if values
        .iter()
        .any(|v| v.is_empty() || *v == "." || *v == ".." || v.contains('\0'))
    {
        return Err(
            "INVALID_PATH: Expected an absolute canonical file path without traversal".into(),
        );
    }
    Ok(values)
}

fn owned_fd(fd: libc::c_int) -> FsResult<File> {
    if fd < 0 {
        Err(last_error())
    } else {
        // SAFETY: successful open/openat returns a fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn open_dir(parent: &File, name: &CString) -> std::io::Result<File> {
    // SAFETY: live directory descriptor and NUL-terminated component.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: successful openat returned a fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

struct Parent {
    directory: File,
    name: CString,
    chain: String,
    missing_parent: bool,
}

fn open_parent(path: &str, create: Option<u32>, cancelled: &AtomicBool) -> FsResult<Parent> {
    let components = parts(path)?;
    let root = cname("/")?;
    // SAFETY: literal root path is NUL terminated; open creates an owned directory descriptor.
    let mut directory = owned_fd(unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    let mut identities = String::new();
    for component in &components[..components.len() - 1] {
        check_cancelled(cancelled)?;
        let metadata = directory.metadata().map_err(io)?;
        identities.push_str(&format!("{}:{};", metadata.dev(), metadata.ino()));
        let name = cname(component)?;
        match open_dir(&directory, &name) {
            Ok(next) => directory = next,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create.is_none() {
                    return Ok(Parent {
                        directory,
                        name: cname(components.last().expect("nonempty path"))?,
                        chain: identities,
                        missing_parent: true,
                    });
                }
                // SAFETY: descriptor-relative single component; mkdir never follows the leaf.
                let result = unsafe {
                    libc::mkdirat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        create.unwrap_or(0o777) as libc::mode_t,
                    )
                };
                if result < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(last_error());
                }
                let next = open_dir(&directory, &name).map_err(io)?;
                // Make each newly reachable parent durable before committing a file below it.
                directory.sync_all().map_err(io)?;
                next.sync_all().map_err(io)?;
                directory = next;
            }
            Err(error) if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
                return Err(format!(
                    "UNSAFE_PATH: Cannot traverse directory without following symlinks: {error}"
                ))
            }
            Err(error) => return Err(io(error)),
        }
    }
    let metadata = directory.metadata().map_err(io)?;
    identities.push_str(&format!("{}:{};", metadata.dev(), metadata.ino()));
    Ok(Parent {
        directory,
        name: cname(components.last().expect("nonempty path"))?,
        chain: identities,
        missing_parent: false,
    })
}

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
    size: i64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}

// libc field widths differ across the supported Unix targets.
#[allow(clippy::unnecessary_cast)]
fn identity(status: libc::stat) -> Identity {
    Identity {
        dev: status.st_dev as u64,
        ino: status.st_ino as u64,
        mode: status.st_mode as u32,
        size: status.st_size,
        mtime: status.st_mtime,
        mtime_nsec: status.st_mtime_nsec,
        ctime: status.st_ctime,
        ctime_nsec: status.st_ctime_nsec,
    }
}

fn stat_at(parent: &Parent) -> FsResult<Option<Identity>> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live descriptor/name and writable output; NOFOLLOW preserves leaf symlink identity.
    let result = unsafe {
        libc::fstatat(
            parent.directory.as_raw_fd(),
            parent.name.as_ptr(),
            status.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(io(error))
        }
    } else {
        // SAFETY: successful fstatat initialized the structure.
        Ok(Some(identity(unsafe { status.assume_init() })))
    }
}

fn stat_file(file: &File) -> FsResult<Identity> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live owned descriptor and writable output.
    if unsafe { libc::fstat(file.as_raw_fd(), status.as_mut_ptr()) } < 0 {
        return Err(last_error());
    }
    // SAFETY: successful fstat initialized the structure.
    Ok(identity(unsafe { status.assume_init() }))
}

fn snapshot_at(
    parent: &Parent,
    path: &str,
    maximum: usize,
    include_content: bool,
    allow_symlink: bool,
    cancelled: &AtomicBool,
) -> FsResult<Snapshot> {
    check_cancelled(cancelled)?;
    let status = if parent.missing_parent {
        None
    } else {
        stat_at(parent)?
    };
    let Some(before) = status else {
        return Ok(Snapshot {
            exists: false,
            kind: "missing".into(),
            version: digest(format!("v1\0{path}\0{}\0missing", parent.chain).as_bytes()),
            digest: None,
            content: None,
            mode: None,
            size: 0,
        });
    };
    let kind = before.mode & mode_bits(libc::S_IFMT);
    let mut bytes = if include_content {
        Some(Vec::new())
    } else {
        None
    };
    let mut hasher = Sha256::new();
    let size;
    let is_symlink = kind == mode_bits(libc::S_IFLNK);
    if is_symlink && allow_symlink {
        let mut target = vec![0u8; maximum.min(32768).saturating_add(1)];
        // SAFETY: valid directory/name and initialized writable buffer; readlinkat never follows leaf.
        let length = unsafe {
            libc::readlinkat(
                parent.directory.as_raw_fd(),
                parent.name.as_ptr(),
                target.as_mut_ptr().cast(),
                target.len(),
            )
        };
        if length < 0 {
            return Err(last_error());
        }
        if length as usize >= target.len() {
            return Err(too_large(maximum));
        }
        target.truncate(length as usize);
        size = target.len() as u64;
        hasher.update(&target);
        if let Some(bytes) = &mut bytes {
            bytes.extend_from_slice(&target);
        }
    } else {
        if kind != mode_bits(libc::S_IFREG) {
            return Err("NOT_REGULAR_FILE: File path must identify a regular file (symlinks require explicit leaf mode)".into());
        }
        if before.size < 0 || before.size as u64 > maximum as u64 {
            return Err(too_large(maximum));
        }
        // NONBLOCK prevents a regular-file-to-FIFO race from hanging open before fstat validates it.
        // SAFETY: valid directory/name; openat no-follow returns a fresh owned descriptor.
        let mut file = owned_fd(unsafe {
            libc::openat(
                parent.directory.as_raw_fd(),
                parent.name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
        if stat_file(&file)? != before {
            return Err(changed());
        }
        let mut count = 0usize;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check_cancelled(cancelled)?;
            let remaining = maximum
                .saturating_sub(count)
                .saturating_add(1)
                .min(buffer.len());
            let read = file.read(&mut buffer[..remaining]).map_err(io)?;
            if read == 0 {
                break;
            }
            count += read;
            if count > maximum {
                return Err(too_large(maximum));
            }
            hasher.update(&buffer[..read]);
            if let Some(bytes) = &mut bytes {
                bytes.extend_from_slice(&buffer[..read]);
            }
        }
        if stat_file(&file)? != before {
            return Err(changed());
        }
        size = count as u64;
    }
    if stat_at(parent)?.as_ref() != Some(&before) {
        return Err(changed());
    }
    let hash = format!("{:x}", hasher.finalize());
    let version = digest(format!("v1\0{path}\0{}\0{before:?}\0{hash}", parent.chain).as_bytes());
    Ok(Snapshot {
        exists: true,
        kind: if is_symlink { "symlink" } else { "file" }.into(),
        version,
        digest: Some(hash),
        content: bytes,
        mode: Some(before.mode & 0o777),
        size,
    })
}

/// A pinned evidence source. Uses the same descriptor-relative no-follow traversal as mutations.
pub(crate) struct EvidenceFile {
    pub file: File,
    pub size: u64,
    pub mode: u32,
    before: Identity,
    parent: Parent,
}

pub(crate) fn ensure_private_directory(path: &str, cancelled: &AtomicBool) -> FsResult<()> {
    let parent = open_parent(
        &format!("{path}/.directory-boundary"),
        Some(0o700),
        cancelled,
    )?;
    parent.directory.sync_all().map_err(io)
}

impl EvidenceFile {
    pub fn flush(&self, path: &str, cancelled: &AtomicBool) -> FsResult<bool> {
        self.recheck(path, cancelled)?;
        self.file.sync_all().map_err(io)?;
        self.parent.directory.sync_all().map_err(io)?;
        // Flush each ancestor entry too: old callers may have created these directories without syncing them.
        let components = parts(path)?;
        let mut directory = File::open("/").map_err(io)?;
        for component in &components[..components.len() - 1] {
            check_cancelled(cancelled)?;
            directory.sync_all().map_err(io)?;
            directory = open_dir(&directory, &cname(component)?).map_err(io)?;
        }
        directory.sync_all().map_err(io)?;
        self.recheck(path, cancelled)?;
        Ok(true)
    }

    pub fn open(path: &str, cancelled: &AtomicBool) -> FsResult<Self> {
        check_cancelled(cancelled)?;
        let parent = open_parent(path, None, cancelled)?;
        if parent.missing_parent {
            return Err("source_missing".into());
        }
        let before = stat_at(&parent)?.ok_or("source_missing")?;
        let kind = before.mode & mode_bits(libc::S_IFMT);
        if kind == mode_bits(libc::S_IFLNK) {
            return Err("symlink_source".into());
        }
        if kind != mode_bits(libc::S_IFREG) {
            return Err("not_regular_file".into());
        }
        // SAFETY: pinned directory and single validated leaf; no-follow and nonblocking defeat link/FIFO races.
        let file = owned_fd(unsafe {
            libc::openat(
                parent.directory.as_raw_fd(),
                parent.name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
        if stat_file(&file)? != before {
            return Err(changed());
        }
        let output = Self {
            file,
            size: before.size as u64,
            mode: before.mode & 0o777,
            before,
            parent,
        };
        output.recheck(path, cancelled)?;
        Ok(output)
    }

    pub fn recheck(&self, path: &str, cancelled: &AtomicBool) -> FsResult<()> {
        check_cancelled(cancelled)?;
        let current = open_parent(path, None, cancelled)?;
        if current.missing_parent
            || current.chain != self.parent.chain
            || stat_file(&self.file)? != self.before
            || stat_at(&current)?.as_ref() != Some(&self.before)
        {
            return Err(changed());
        }
        Ok(())
    }
}

pub(super) fn snapshot(
    path: &str,
    maximum: usize,
    content: bool,
    symlink: bool,
    cancelled: &AtomicBool,
) -> FsResult<Snapshot> {
    let parent = open_parent(path, None, cancelled)?;
    let result = snapshot_at(&parent, path, maximum, content, symlink, cancelled)?;
    if open_parent(path, None, cancelled)?.chain != parent.chain {
        return Err(changed());
    }
    Ok(result)
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Temporary<'a> {
    parent: &'a File,
    name: CString,
    present: bool,
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if self.present {
            // SAFETY: live owned parent/name; only created temporaries construct this guard.
            unsafe {
                libc::unlinkat(self.parent.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}

fn temporary_name() -> FsResult<CString> {
    cname(&format!(
        ".octocode-{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn create_temporary_named<'a>(
    parent: &'a File,
    name: CString,
    mode: u32,
) -> FsResult<(Temporary<'a>, File)> {
    // SAFETY: O_EXCL ensures ownership; NOFOLLOW also rejects leaf links. Initial mode precedes any write.
    let file = owned_fd(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    })?;
    Ok((
        Temporary {
            parent,
            name,
            present: true,
        },
        file,
    ))
}

fn receipt(parent: &File, bytes: u64, warnings: Vec<String>) -> Receipt {
    let mut receipt = Receipt {
        committed: true,
        durable: true,
        warnings,
        bytes,
    };
    if let Err(error) = parent.sync_all() {
        receipt.durable = false;
        receipt.warnings.push(format!(
            "File committed, but parent directory sync failed: {error}"
        ));
    }
    receipt
}

pub(super) fn replace(
    path: &str,
    bytes: &[u8],
    expected: &str,
    maximum: usize,
    create_mode: Option<u32>,
    cancelled: &AtomicBool,
    parent_mode: Option<u32>,
) -> FsResult<Receipt> {
    if bytes.len() > maximum {
        return Err(too_large(maximum));
    }
    if create_mode.is_some_and(|mode| mode > 0o7777) {
        return Err("INVALID_MODE: Mode must be between 0 and 4095".into());
    }
    let before_parent = open_parent(path, None, cancelled)?;
    let before = snapshot_at(&before_parent, path, maximum, false, false, cancelled)?;
    if before.version != expected {
        return Err(changed());
    }
    let parent = open_parent(path, Some(parent_mode.unwrap_or(0o777)), cancelled)?;
    if !parent.chain.starts_with(&before_parent.chain) {
        return Err(changed());
    }
    let after_parent_creation =
        if parent.chain == before_parent.chain && !before_parent.missing_parent {
            None
        } else {
            Some(snapshot_at(
                &parent, path, maximum, false, false, cancelled,
            )?)
        };
    let prepared = after_parent_creation.as_ref().unwrap_or(&before);
    if before.exists && prepared.version != before.version || !before.exists && prepared.exists {
        return Err(changed());
    }
    let mode = create_mode.or(before.mode).unwrap_or(0o666);
    let (mut temporary, mut file) =
        create_temporary_named(&parent.directory, temporary_name()?, mode)?;
    if create_mode.is_some() || before.mode.is_some() {
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(io)?;
    }
    for chunk in bytes.chunks(64 * 1024) {
        check_cancelled(cancelled)?;
        file.write_all(chunk).map_err(io)?;
    }
    file.sync_all().map_err(io)?;
    check_cancelled(cancelled)?;
    let latest_parent = open_parent(path, None, cancelled)?;
    if latest_parent.chain != parent.chain
        || snapshot_at(&parent, path, maximum, false, false, cancelled)?.version != prepared.version
    {
        return Err(changed());
    }
    check_cancelled(cancelled)?;
    let mut warnings = Vec::new();
    if before.exists {
        // SAFETY: both component names are relative to the same live pinned directory.
        if unsafe {
            libc::renameat(
                parent.directory.as_raw_fd(),
                temporary.name.as_ptr(),
                parent.directory.as_raw_fd(),
                parent.name.as_ptr(),
            )
        } < 0
        {
            return Err(last_error());
        }
        temporary.present = false;
    } else {
        // SAFETY: flags=0 links the temporary itself and fails if the target appeared concurrently.
        if unsafe {
            libc::linkat(
                parent.directory.as_raw_fd(),
                temporary.name.as_ptr(),
                parent.directory.as_raw_fd(),
                parent.name.as_ptr(),
                0,
            )
        } < 0
        {
            return Err(create_publication_error(std::io::Error::last_os_error()));
        }
        // Commit happened at linkat; cleanup failure is now a truthful warning, never a failed write.
        // SAFETY: the temporary name belongs to this operation in the pinned directory.
        if unsafe { libc::unlinkat(parent.directory.as_raw_fd(), temporary.name.as_ptr(), 0) } < 0 {
            warnings.push(format!(
                "File committed, but temporary cleanup failed: {}",
                std::io::Error::last_os_error()
            ));
        } else {
            temporary.present = false;
        }
    }
    Ok(receipt(&parent.directory, bytes.len() as u64, warnings))
}

pub(super) fn delete(
    path: &str,
    expected: &str,
    maximum: usize,
    cancelled: &AtomicBool,
) -> FsResult<Receipt> {
    let parent = open_parent(path, None, cancelled)?;
    let before = snapshot_at(&parent, path, maximum, false, true, cancelled)?;
    if !before.exists || before.version != expected {
        return Err(changed());
    }
    if open_parent(path, None, cancelled)?.chain != parent.chain
        || snapshot_at(&parent, path, maximum, false, true, cancelled)?.version != before.version
    {
        return Err(changed());
    }
    check_cancelled(cancelled)?;
    // SAFETY: unlinkat removes the leaf itself; no symlink target traversal.
    if unsafe { libc::unlinkat(parent.directory.as_raw_fd(), parent.name.as_ptr(), 0) } < 0 {
        return Err(last_error());
    }
    Ok(receipt(&parent.directory, before.size, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_only_publication_collision_is_a_precondition_failure() {
        assert!(
            create_publication_error(std::io::Error::from_raw_os_error(libc::EEXIST))
                .starts_with("PRECONDITION_FAILED:")
        );
        assert!(
            create_publication_error(std::io::Error::from_raw_os_error(libc::EACCES))
                .starts_with("IO_FAILURE:")
        );
    }
    #[test]
    fn collision_never_owns_or_removes_foreign_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let parent = File::open(dir.path()).unwrap();
        let path = dir.path().join("collision");
        std::fs::write(&path, b"foreign").unwrap();
        assert!(create_temporary_named(&parent, cname("collision").unwrap(), 0o600).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"foreign");
    }
    #[test]
    fn private_mode_is_applied_when_temporary_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let parent = File::open(dir.path()).unwrap();
        let (_guard, file) =
            create_temporary_named(&parent, cname("private").unwrap(), 0o600).unwrap();
        assert_eq!(file.metadata().unwrap().mode() & 0o777, 0o600);
    }
    #[test]
    fn postcommit_receipt_does_not_consult_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let parent = File::open(dir.path()).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(check_cancelled(&cancelled).is_err());
        assert!(receipt(&parent, 0, Vec::new()).committed);
    }
    #[test]
    fn failed_postcommit_sync_preserves_commit_and_reports_nondurability() {
        // /dev/null supplies a real descriptor whose fsync fails on Unix,
        // exercising the postcommit failure branch without mutating global state.
        let unsyncable = File::open("/dev/null").unwrap();
        let result = receipt(&unsyncable, 17, Vec::new());
        assert!(result.committed);
        assert!(!result.durable);
        assert_eq!(result.bytes, 17);
        assert!(result.warnings[0].contains("File committed"));
    }
}
