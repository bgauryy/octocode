//! Windows handle-relative filesystem boundary. Every child open uses a pinned
//! RootDirectory and FILE_OPEN_REPARSE_POINT; no mutation uses a Win32 path.
//! NT layouts/constants follow Microsoft's NtCreateFile, FILE_RENAME_INFORMATION
//! and FILE_DISPOSITION_INFORMATION documentation. Windows ACLs, not Unix mode
//! bits, govern access. Directory-entry durability is not claimed on Windows.
use super::{check_cancelled, FsResult, Receipt, Snapshot};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::{offset_of, size_of};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::IO::DeviceIoControl;

const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
const DELETE_ACCESS: u32 = 0x0001_0000;
const READ_CONTROL_ACCESS: u32 = 0x0002_0000;
const FILE_OPEN: u32 = 1;
const FILE_CREATE: u32 = 2;
const FILE_OPEN_IF: u32 = 3;
const FILE_DIRECTORY_FILE: u32 = 1;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xa000_000c;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}
#[repr(C)]
#[derive(Default)]
struct IoStatusBlock {
    // The NTSTATUS/pointer union occupies a pointer-sized slot on every target.
    status: usize,
    information: usize,
}
#[repr(C)]
struct RenameInformation {
    flags: u32,
    root_directory: HANDLE,
    file_name_length: u32,
    file_name: [u16; 1],
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        handle: *mut HANDLE,
        access: u32,
        attributes: *mut ObjectAttributes,
        status: *mut IoStatusBlock,
        allocation_size: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_length: u32,
    ) -> i32;
    fn NtSetInformationFile(
        handle: HANDLE,
        status: *mut IoStatusBlock,
        information: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}

fn io(error: std::io::Error) -> String {
    format!("IO_FAILURE: {error}")
}
fn changed() -> String {
    "PRECONDITION_FAILED: File or parent changed after preflight".into()
}
fn too_large(maximum: usize) -> String {
    format!("TOO_LARGE: File exceeds maximum {maximum} bytes")
}
fn hash(bytes: &[u8]) -> String {
    crate::digest_hex::lower_hex(Sha256::digest(bytes))
}
fn nt_error(status: i32) -> std::io::Error {
    // SAFETY: pure status-code conversion, no pointers.
    std::io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
}
fn invalid_path() -> String {
    "INVALID_PATH: Expected a canonical absolute drive or UNC file path without aliases, streams or traversal".into()
}

struct PathParts {
    root: String,
    components: Vec<String>,
}

fn valid_component(value: &str) -> bool {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.ends_with(['.', ' '])
        || value.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
    {
        return false;
    }
    let base = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) && !(base.starts_with("COM") || base.starts_with("LPT"))
        .then(|| &base[3..])
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

fn parts(path: &str) -> FsResult<PathParts> {
    if path.encode_utf16().count() > 32760 || path.contains('\0') || path.contains('/') {
        return Err(invalid_path());
    }
    let normalized = path.strip_prefix(r"\\?\").unwrap_or(path);
    let (root, tail) = if normalized.as_bytes().get(1) == Some(&b':')
        && normalized
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        && normalized.as_bytes().get(2) == Some(&b'\\')
    {
        (format!(r"\??\{}\", &normalized[..2]), &normalized[3..])
    } else {
        let unc = normalized
            .strip_prefix(r"UNC\")
            .filter(|_| path.starts_with(r"\\?\"))
            .or_else(|| normalized.strip_prefix(r"\\"))
            .ok_or_else(invalid_path)?;
        let mut values = unc.splitn(3, '\\');
        let server = values.next().ok_or_else(invalid_path)?;
        let share = values.next().ok_or_else(invalid_path)?;
        let tail = values.next().ok_or_else(invalid_path)?;
        if !valid_component(server) || !valid_component(share) {
            return Err(invalid_path());
        }
        (format!(r"\??\UNC\{server}\{share}\"), tail)
    };
    let components: Vec<_> = tail.split('\\').map(str::to_owned).collect();
    if !components.iter().all(|value| valid_component(value)) {
        return Err(invalid_path());
    }
    Ok(PathParts { root, components })
}

fn open_relative(
    parent: Option<&File>,
    name: &str,
    access: u32,
    disposition: u32,
    directory: bool,
) -> std::io::Result<File> {
    open_with_security(parent, name, access, disposition, directory, null_mut())
}

fn open_with_security(
    parent: Option<&File>,
    name: &str,
    access: u32,
    disposition: u32,
    directory: bool,
    security_descriptor: *mut c_void,
) -> std::io::Result<File> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    let length = u16::try_from(wide.len() * 2)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path is too long"))?;
    let mut unicode = UnicodeString {
        length,
        maximum_length: length,
        buffer: wide.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.map_or(null_mut(), |file| file.as_raw_handle()),
        object_name: &mut unicode,
        attributes: 0x40, // OBJ_CASE_INSENSITIVE, the conventional Windows namespace.
        security_descriptor,
        security_quality_of_service: null_mut(),
    };
    let mut handle = null_mut();
    let mut status = IoStatusBlock::default();
    // SAFETY: all native structures have documented C layout and live storage.
    // The only multi-component open is the validated drive/UNC root. Child opens
    // use one validated name relative to the live parent handle and bypass reparses.
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            access | SYNCHRONIZE_ACCESS | FILE_READ_ATTRIBUTES,
            &mut attributes,
            &mut status,
            null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            FILE_OPEN_REPARSE_POINT
                | FILE_SYNCHRONOUS_IO_NONALERT
                | if directory { FILE_DIRECTORY_FILE } else { 0 },
            null(),
            0,
        )
    };
    if result < 0 {
        return Err(nt_error(result));
    }
    // SAFETY: successful NtCreateFile returned one fresh owned handle.
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    volume: u32,
    index: u64,
    attributes: u32,
    size: u64,
    created: u64,
    modified: u64,
    changed: i64,
    tag: u32,
}

fn identity(file: &File) -> FsResult<Identity> {
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    let mut basic = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let mut tag = std::mem::MaybeUninit::<FILE_ATTRIBUTE_TAG_INFO>::uninit();
    // SAFETY: live handle and correctly sized writable native structures.
    unsafe {
        if GetFileType(file.as_raw_handle()) != FILE_TYPE_DISK {
            return Err("NOT_REGULAR_FILE: Expected a disk file".into());
        }
        if GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) == 0
            || GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileBasicInfo,
                basic.as_mut_ptr().cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            ) == 0
            || GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileAttributeTagInfo,
                tag.as_mut_ptr().cast(),
                size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            ) == 0
        {
            return Err(io(std::io::Error::last_os_error()));
        }
        let info = info.assume_init();
        let basic = basic.assume_init();
        let tag = tag.assume_init();
        Ok(Identity {
            volume: info.dwVolumeSerialNumber,
            index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
            attributes: info.dwFileAttributes,
            size: ((info.nFileSizeHigh as u64) << 32) | info.nFileSizeLow as u64,
            created: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
                | info.ftCreationTime.dwLowDateTime as u64,
            modified: ((info.ftLastWriteTime.dwHighDateTime as u64) << 32)
                | info.ftLastWriteTime.dwLowDateTime as u64,
            changed: basic.ChangeTime,
            tag: tag.ReparseTag,
        })
    }
}

struct Parent {
    // Retain all ancestors for the entire operation, including the trusted root.
    ancestors: Vec<File>,
    name: String,
    chain: String,
    missing_parent: bool,
}
impl Parent {
    fn directory(&self) -> &File {
        self.ancestors.last().expect("root is retained")
    }
}
fn directory_id(directory: &File) -> FsResult<String> {
    let id = identity(directory)?;
    if id.attributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || id.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("UNSAFE_PATH: Directory traversal through a reparse point is forbidden".into());
    }
    Ok(format!("{}:{};", id.volume, id.index))
}
fn open_parent(path: &str, create: Option<u32>, cancelled: &AtomicBool) -> FsResult<Parent> {
    let path = parts(path)?;
    let security = if create == Some(0o700) {
        Some(private_security()?)
    } else {
        None
    };
    let root = open_relative(None, &path.root, FILE_TRAVERSE, FILE_OPEN, true).map_err(io)?;
    let mut parent = Parent {
        chain: directory_id(&root)?,
        ancestors: vec![root],
        name: path
            .components
            .last()
            .expect("validated nonempty components")
            .clone(),
        missing_parent: false,
    };
    for component in &path.components[..path.components.len() - 1] {
        check_cancelled(cancelled)?;
        let next = match open_with_security(
            Some(parent.directory()),
            component,
            FILE_TRAVERSE,
            if create.is_some() {
                FILE_OPEN_IF
            } else {
                FILE_OPEN
            },
            true,
            security.as_ref().map_or(null_mut(), |value| value.0),
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create.is_none() => {
                parent.missing_parent = true;
                return Ok(parent);
            }
            Err(error) => return Err(io(error)),
        };
        parent.chain.push_str(&directory_id(&next)?);
        parent.ancestors.push(next);
    }
    Ok(parent)
}

fn open_leaf(parent: &Parent, access: u32) -> FsResult<Option<File>> {
    if parent.missing_parent {
        return Ok(None);
    }
    match open_relative(
        Some(parent.directory()),
        &parent.name,
        access,
        FILE_OPEN,
        false,
    ) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

fn read_symlink(file: &File, maximum: usize) -> FsResult<Vec<u8>> {
    let mut buffer = [0u8; 16 * 1024];
    let mut length = 0;
    // SAFETY: synchronous handle, initialized writable buffer and output length.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            null(),
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut length,
            null_mut(),
        )
    } == 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    let data = &buffer[..length as usize];
    if data.len() < 20
        || u32::from_le_bytes(data[0..4].try_into().unwrap()) != IO_REPARSE_TAG_SYMLINK
    {
        return Err("UNSAFE_PATH: Only symbolic-link leaves may be inspected for deletion".into());
    }
    // REPARSE_DATA_BUFFER's symbolic-link PathBuffer starts at byte 20.
    let offset = u16::from_le_bytes(data[8..10].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(data[10..12].try_into().unwrap()) as usize;
    let target = data
        .get(20 + offset..20 + offset + count)
        .filter(|_| count.is_multiple_of(2))
        .ok_or("IO_FAILURE: Invalid symlink reparse buffer")?;
    let wide: Vec<_> = target
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&value| u16::from_le_bytes(value))
        .collect();
    let text =
        String::from_utf16(&wide).map_err(|_| "IO_FAILURE: Symlink target is not Unicode")?;
    if text.len() > maximum {
        return Err(too_large(maximum));
    }
    Ok(text.into_bytes())
}

fn snapshot_at(
    parent: &Parent,
    path: &str,
    maximum: usize,
    content: bool,
    symlink: bool,
    cancelled: &AtomicBool,
) -> FsResult<Snapshot> {
    check_cancelled(cancelled)?;
    let Some(mut file) = open_leaf(parent, FILE_READ_DATA)? else {
        return Ok(Snapshot {
            exists: false,
            kind: "missing".into(),
            version: hash(format!("w1\0{path}\0{}\0missing", parent.chain).as_bytes()),
            digest: None,
            content: None,
            mode: None,
            size: 0,
        });
    };
    let before = identity(&file)?;
    let reparse = before.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    if (!reparse && before.attributes & FILE_ATTRIBUTE_DIRECTORY != 0)
        || (reparse && (!symlink || before.tag != IO_REPARSE_TAG_SYMLINK))
    {
        return Err(
            "NOT_REGULAR_FILE: Expected a regular file or explicitly permitted file symlink".into(),
        );
    }
    let mut bytes = content.then(Vec::new);
    let mut hasher = Sha256::new();
    let mut count = 0usize;
    if reparse {
        let target = read_symlink(&file, maximum)?;
        count = target.len();
        hasher.update(&target);
        if let Some(bytes) = &mut bytes {
            bytes.extend_from_slice(&target);
        }
    } else {
        if before.size > maximum as u64 {
            return Err(too_large(maximum));
        }
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check_cancelled(cancelled)?;
            let limit = maximum
                .saturating_sub(count)
                .saturating_add(1)
                .min(buffer.len());
            let length = file.read(&mut buffer[..limit]).map_err(io)?;
            if length == 0 {
                break;
            }
            count += length;
            if count > maximum {
                return Err(too_large(maximum));
            }
            hasher.update(&buffer[..length]);
            if let Some(bytes) = &mut bytes {
                bytes.extend_from_slice(&buffer[..length]);
            }
        }
    }
    if identity(&file)? != before
        || open_leaf(parent, 0)?
            .map(|f| identity(&f))
            .transpose()?
            .as_ref()
            != Some(&before)
    {
        return Err(changed());
    }
    let digest = crate::digest_hex::lower_hex(hasher.finalize());
    Ok(Snapshot {
        exists: true,
        kind: if reparse { "symlink" } else { "file" }.into(),
        version: hash(format!("w1\0{path}\0{}\0{before:?}\0{digest}", parent.chain).as_bytes()),
        digest: Some(digest),
        content: bytes,
        mode: None,
        size: count as u64,
    })
}

pub(crate) struct EvidenceFile {
    pub file: File,
    pub size: u64,
    pub mode: u32,
    before: Identity,
    parent: Parent,
}

pub(crate) fn ensure_private_directory(path: &str, cancelled: &AtomicBool) -> FsResult<()> {
    open_parent(
        &format!("{path}\\.directory-boundary"),
        Some(0o700),
        cancelled,
    )?;
    Ok(())
}

impl EvidenceFile {
    pub fn flush(&self, path: &str, cancelled: &AtomicBool) -> FsResult<bool> {
        self.recheck(path, cancelled)?;
        let writable = open_leaf(&self.parent, FILE_WRITE_DATA)?.ok_or("source_missing")?;
        if identity(&writable)? != self.before {
            return Err(changed());
        }
        writable.sync_all().map_err(io)?;
        self.recheck(path, cancelled)?;
        // File data is flushed; the Windows API offers no portable parent-entry fsync.
        Ok(false)
    }

    pub fn open(path: &str, cancelled: &AtomicBool) -> FsResult<Self> {
        check_cancelled(cancelled)?;
        let parent = open_parent(path, None, cancelled)?;
        let file = open_leaf(&parent, FILE_READ_DATA)?.ok_or("source_missing")?;
        let before = identity(&file)?;
        if before.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("symlink_source".into());
        }
        if before.attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err("not_regular_file".into());
        }
        // libuv/Node stat mode on Windows: read bits for all, plus write bits unless readonly.
        // Keep this evidence-only value out of the public Snapshot Unix permissions contract.
        let mode = if before.attributes & FILE_ATTRIBUTE_READONLY != 0 {
            0o444
        } else {
            0o666
        };
        let output = Self {
            file,
            size: before.size,
            mode,
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
            || identity(&self.file)? != self.before
            || open_leaf(&current, 0)?
                .map(|file| identity(&file))
                .transpose()?
                .as_ref()
                != Some(&self.before)
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

fn disposition(file: &File) -> FsResult<()> {
    let mut delete: u8 = 1;
    let mut status = IoStatusBlock::default();
    // SAFETY: FileDispositionInformation=13 accepts a BOOLEAN; the live handle
    // has DELETE access. No subsequent IO is performed before closing this handle.
    let result = unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&mut delete as *mut u8).cast(),
            1,
            13,
        )
    };
    if result < 0 {
        Err(io(nt_error(result)))
    } else {
        Ok(())
    }
}

struct Temporary {
    file: File,
    present: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if self.present {
            let _ = disposition(&self.file);
        }
    }
}
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct SecurityDescriptor(*mut c_void);
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: successful security APIs allocate these buffers with LocalAlloc.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn private_security() -> FsResult<SecurityDescriptor> {
    let mut token = null_mut();
    // SAFETY: live current-process pseudo handle and output token storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    let mut size = 0;
    // SAFETY: zero-length query obtains the required TOKEN_USER allocation.
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size);
    }
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    // SAFETY: aligned output has the queried size and token remains live.
    let success = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    let error = std::io::Error::last_os_error();
    // SAFETY: owned process-token handle is released exactly once.
    unsafe {
        CloseHandle(token);
    }
    if success == 0 {
        return Err(io(error));
    }
    let mut sid_text = null_mut();
    // SAFETY: successful GetTokenInformation initialized TOKEN_USER and its SID.
    if unsafe {
        ConvertSidToStringSidW(
            (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid,
            &mut sid_text,
        )
    } == 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    let sid_owner = SecurityDescriptor(sid_text.cast());
    let mut length = 0;
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated string.
    unsafe {
        while *sid_text.add(length) != 0 {
            length += 1;
        }
    }
    // SAFETY: length was bounded by the API-owned terminated allocation.
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(sid_text, length) })
        .map_err(|_| "IO_FAILURE: Invalid current-user SID")?;
    drop(sid_owner);
    // Protected DACL: only the actual caller user SID and SYSTEM receive access.
    // Unlike chmod(0600), this is enforced by Windows at initial file creation.
    let sddl: Vec<u16> = format!("D:P(A;;FA;;;SY)(A;;FA;;;{sid})\0")
        .encode_utf16()
        .collect();
    let mut descriptor = null_mut();
    // SAFETY: terminated SDDL input and writable LocalAlloc-owned output.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(SecurityDescriptor(descriptor))
}

fn existing_security(parent: &Parent) -> FsResult<Option<SecurityDescriptor>> {
    let Some(file) = open_leaf(parent, READ_CONTROL_ACCESS)? else {
        return Ok(None);
    };
    let mut descriptor = null_mut();
    // SAFETY: live READ_CONTROL handle and output pointer; result is LocalAlloc-owned.
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io(std::io::Error::from_raw_os_error(error as i32)));
    }
    let descriptor = SecurityDescriptor(descriptor);
    // Preserve effective target ACL without merging broader parent inheritance.
    // SAFETY: valid mutable security descriptor returned by GetSecurityInfo.
    if unsafe { SetSecurityDescriptorControl(descriptor.0, SE_DACL_PROTECTED, SE_DACL_PROTECTED) }
        == 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(Some(descriptor))
}

fn create_temporary(
    parent: &Parent,
    name: &str,
    security: Option<&SecurityDescriptor>,
) -> FsResult<Temporary> {
    // FILE_CREATE is exclusive; never construct a cleanup owner on collision.
    let file = open_with_security(
        Some(parent.directory()),
        name,
        FILE_GENERIC_WRITE | DELETE_ACCESS,
        FILE_CREATE,
        false,
        security.map_or(null_mut(), |descriptor| descriptor.0),
    )
    .map_err(io)?;
    Ok(Temporary {
        file,
        present: true,
    })
}
fn rename(file: &File, parent: &Parent, replace: bool) -> FsResult<()> {
    let name: Vec<u16> = parent.name.encode_utf16().collect();
    let length = size_of::<RenameInformation>() + name.len() * 2;
    // usize storage supplies the alignment needed by HANDLE on both x86 and x64.
    let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
    let pointer = storage.as_mut_ptr().cast::<RenameInformation>();
    let mut status = IoStatusBlock::default();
    // SAFETY: aligned owned allocation includes header, variable UTF-16 filename
    // and documented tail padding. RootDirectory pins the destination directory.
    let result = unsafe {
        (*pointer).flags = u32::from(replace);
        (*pointer).root_directory = parent.directory().as_raw_handle();
        (*pointer).file_name_length = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset_of!(RenameInformation, file_name))
                .cast(),
            name.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            pointer.cast(),
            length as u32,
            10,
        )
    };
    if result < 0 {
        let error = nt_error(result);
        if !replace && error.kind() == std::io::ErrorKind::AlreadyExists {
            Err(changed())
        } else {
            Err(io(error))
        }
    } else {
        Ok(())
    }
}
fn receipt(bytes: u64) -> Receipt {
    Receipt { committed: true, durable: false, bytes,
        warnings: vec!["File committed; Windows does not expose a portable directory-entry durability guarantee".into()] }
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
    if create_mode.is_some_and(|mode| mode != 0o600) {
        return Err("INVALID_MODE: Windows supports explicit private mode 0600 via an owner/SYSTEM DACL; other POSIX modes are unavailable".into());
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
    let prepared = snapshot_at(&parent, path, maximum, false, false, cancelled)?;
    if (before.exists && before.version != prepared.version) || (!before.exists && prepared.exists)
    {
        return Err(changed());
    }
    let security = if create_mode == Some(0o600) {
        Some(private_security()?)
    } else {
        existing_security(&parent)?
    };
    let mut temporary = create_temporary(
        &parent,
        &format!(
            ".octocode-{}-{}.tmp",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ),
        security.as_ref(),
    )?;
    for chunk in bytes.chunks(64 * 1024) {
        check_cancelled(cancelled)?;
        temporary.file.write_all(chunk).map_err(io)?;
    }
    temporary.file.sync_all().map_err(io)?;
    if open_parent(path, None, cancelled)?.chain != parent.chain
        || snapshot_at(&parent, path, maximum, false, false, cancelled)?.version != prepared.version
    {
        return Err(changed());
    }
    check_cancelled(cancelled)?;
    rename(&temporary.file, &parent, before.exists)?;
    temporary.present = false;
    Ok(receipt(bytes.len() as u64))
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
    let target = open_leaf(&parent, DELETE_ACCESS)?.ok_or_else(changed)?;
    let target_id = identity(&target)?;
    if open_parent(path, None, cancelled)?.chain != parent.chain
        || snapshot_at(&parent, path, maximum, false, true, cancelled)?.version != before.version
        || open_leaf(&parent, 0)?
            .map(|file| identity(&file))
            .transpose()?
            .as_ref()
            != Some(&target_id)
    {
        return Err(changed());
    }
    check_cancelled(cancelled)?;
    disposition(&target)?;
    drop(target);
    Ok(receipt(before.size))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cancel() -> AtomicBool {
        AtomicBool::new(false)
    }
    #[test]
    fn canonical_drive_and_unc_paths_parse() {
        for path in [
            r"C:\repo\file",
            r"\\?\C:\repo\file",
            r"\\server\share\file",
            r"\\?\UNC\server\share\file",
        ] {
            assert!(parts(path).is_ok(), "{path}");
        }
    }
    #[test]
    fn traversal_streams_devices_and_aliases_fail_before_io() {
        for path in [
            r"C:file",
            r"\file",
            r"C:\",
            r"C:\x\..\file",
            r"C:\x\file:stream",
            r"C:\x\file.",
            r"C:\x\file ",
            r"C:\x\\file",
            r"C:\NUL.txt",
            r"C:\COM¹",
            r"\\.\C:\file",
            r"\\?\GLOBALROOT\Device\file",
            r"\\server\share",
            r"C:/repo/file",
        ] {
            assert!(parts(path).is_err(), "{path}");
        }
    }
    #[test]
    fn bounded_create_replace_and_delete_have_truthful_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .canonicalize()
            .unwrap()
            .join("nested")
            .join("file");
        let path = path.to_str().unwrap();
        let missing = snapshot(path, 5, true, false, &cancel()).unwrap();
        let created = replace(path, b"hello", &missing.version, 5, None, &cancel(), None).unwrap();
        assert!(created.committed && !created.durable);
        let before = snapshot(path, 5, true, false, &cancel()).unwrap();
        assert_eq!(before.content.unwrap(), b"hello");
        assert!(snapshot(path, 4, true, false, &cancel())
            .unwrap_err()
            .starts_with("TOO_LARGE:"));
        assert!(replace(path, b"again", &missing.version, 5, None, &cancel(), None).is_err());
        assert!(
            replace(path, b"again", &before.version, 5, None, &cancel(), None)
                .unwrap()
                .committed
        );
        let after = snapshot(path, 5, false, false, &cancel()).unwrap();
        assert!(
            delete(path, &after.version, 5, &cancel())
                .unwrap()
                .committed
        );
        assert!(!std::path::Path::new(path).exists());
    }
    #[test]
    fn temporary_collision_never_deletes_foreign_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("target");
        let parent = open_parent(path.to_str().unwrap(), None, &cancel()).unwrap();
        std::fs::write(dir.path().join("collision"), b"foreign").unwrap();
        assert!(create_temporary(&parent, "collision", None).is_err());
        assert_eq!(
            std::fs::read(dir.path().join("collision")).unwrap(),
            b"foreign"
        );
    }

    #[test]
    fn exclusive_commit_preserves_a_target_created_after_preflight() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("target");
        let parent = open_parent(path.to_str().unwrap(), None, &cancel()).unwrap();
        let mut temporary = create_temporary(&parent, "owned-temp", None).unwrap();
        temporary.file.write_all(b"ours").unwrap();
        std::fs::write(&path, b"competitor").unwrap();
        assert!(rename(&temporary.file, &parent, false).is_err());
        drop(temporary);
        assert_eq!(std::fs::read(path).unwrap(), b"competitor");
        assert!(!dir.path().join("owned-temp").exists());
    }

    #[test]
    fn junctions_are_neither_traversed_nor_deleted_as_symbolic_links() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let junction = dir.path().join("junction");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("file"), b"outside").unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let absolute = dir.path().canonicalize().unwrap().join("junction");
        assert!(snapshot(
            absolute.join("file").to_str().unwrap(),
            100,
            false,
            false,
            &cancel()
        )
        .is_err());
        assert!(snapshot(absolute.to_str().unwrap(), 100, false, true, &cancel()).is_err());
        assert_eq!(std::fs::read(target.join("file")).unwrap(), b"outside");
        std::fs::remove_dir(junction).unwrap();
    }
    #[test]
    #[allow(clippy::permissions_set_readonly_false)] // This module is Windows-only.
    fn readonly_and_cancellation_preserve_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("file");
        std::fs::write(&path, b"before").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let before = snapshot(path.to_str().unwrap(), 100, false, false, &cancel()).unwrap();
        assert!(replace(
            path.to_str().unwrap(),
            b"after",
            &before.version,
            100,
            None,
            &cancel(),
            None
        )
        .is_err());
        assert!(delete(path.to_str().unwrap(), &before.version, 100, &cancel()).is_err());
        assert!(replace(
            path.to_str().unwrap(),
            b"after",
            &before.version,
            100,
            None,
            &AtomicBool::new(true),
            None
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"before");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
    }

    #[test]
    fn private_acl_is_present_before_bytes_and_preserved_on_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("private");
        let path = path.to_str().unwrap();
        let parent = open_parent(path, None, &cancel()).unwrap();
        let descriptor = private_security().unwrap();
        let temporary = create_temporary(&parent, "acl-probe", Some(&descriptor)).unwrap();
        assert_eq!(temporary.file.metadata().unwrap().len(), 0);
        let mut actual = null_mut();
        // SAFETY: live owned handle and LocalAlloc-owned output.
        assert_eq!(
            unsafe {
                GetSecurityInfo(
                    temporary.file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    &mut actual,
                )
            },
            0
        );
        let actual = SecurityDescriptor(actual);
        assert_eq!(acl_bytes(&actual), acl_bytes(&descriptor));
        drop(temporary);
        assert!(!dir.path().join("acl-probe").exists());

        let missing = snapshot(path, 100, false, false, &cancel()).unwrap();
        replace(
            path,
            b"secret",
            &missing.version,
            100,
            Some(0o600),
            &cancel(),
            None,
        )
        .unwrap();
        let original = existing_security(&parent).unwrap().unwrap();
        let before = snapshot(path, 100, false, false, &cancel()).unwrap();
        replace(
            path,
            b"still secret",
            &before.version,
            100,
            None,
            &cancel(),
            None,
        )
        .unwrap();
        let after = existing_security(&parent).unwrap().unwrap();
        assert_eq!(acl_bytes(&original), acl_bytes(&after));
        assert_eq!(acl_bytes(&descriptor), acl_bytes(&after));
    }

    fn acl_bytes(descriptor: &SecurityDescriptor) -> Vec<u8> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = null_mut();
        let mut control = 0;
        let mut revision = 0;
        // SAFETY: valid security descriptor and writable outputs; the DACL remains
        // owned by descriptor for the duration of copying its documented length.
        unsafe {
            assert_ne!(
                GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert_ne!(
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted),
                0
            );
            assert_ne!(present, 0);
            assert!(!acl.is_null());
            std::slice::from_raw_parts(acl.cast::<u8>(), (*acl).AclSize as usize).to_vec()
        }
    }

    fn directory_security(path: &str) -> SecurityDescriptor {
        let parent = open_parent(path, None, &cancel()).unwrap();
        let file = open_relative(
            Some(parent.directory()),
            &parent.name,
            READ_CONTROL_ACCESS,
            FILE_OPEN,
            true,
        )
        .unwrap();
        let mut descriptor = null_mut();
        // SAFETY: owned READ_CONTROL directory handle and writable LocalAlloc-owned output.
        assert_eq!(
            unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    &mut descriptor,
                )
            },
            0
        );
        SecurityDescriptor(descriptor)
    }

    fn dacl_state(descriptor: &SecurityDescriptor) -> (u16, Vec<u8>) {
        let (mut present, mut defaulted, mut control, mut revision) = (0, 0, 0, 0);
        let mut acl = null_mut();
        // SAFETY: descriptor owns a valid DACL for the entire read.
        unsafe {
            assert_ne!(
                GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision),
                0
            );
            assert_ne!(
                GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted),
                0
            );
            assert_ne!(present, 0);
            assert!(!acl.is_null());
            (
                control & SE_DACL_PROTECTED,
                std::slice::from_raw_parts(acl.cast::<u8>(), (*acl).AclSize as usize).to_vec(),
            )
        }
    }

    #[test]
    fn private_recursive_parents_use_protected_acl_without_changing_existing_directory_acl() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let existing = root.join("existing");
        std::fs::create_dir(&existing).unwrap();
        let original = directory_security(existing.to_str().unwrap());
        let path = existing.join("private").join("deep").join("file");
        let before = snapshot(path.to_str().unwrap(), 100, false, false, &cancel()).unwrap();
        replace(
            path.to_str().unwrap(),
            b"private",
            &before.version,
            100,
            Some(0o600),
            &cancel(),
            Some(0o700),
        )
        .unwrap();
        let private = private_security().unwrap();
        for directory in [
            existing.join("private"),
            existing.join("private").join("deep"),
        ] {
            let actual = directory_security(directory.to_str().unwrap());
            assert_eq!(acl_bytes(&actual), acl_bytes(&private));
        }
        let after = directory_security(existing.to_str().unwrap());
        assert_eq!(dacl_state(&original), dacl_state(&after));
    }

    #[test]
    fn leaf_symlinks_delete_the_link_and_directory_reparses_never_traverse() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let target = canonical.join("target");
        std::fs::write(&target, b"outside").unwrap();
        let link = canonical.join("link");
        // Windows CI must enable developer mode or have symlink privilege;
        // missing privilege fails this safety test instead of silently skipping it.
        std::os::windows::fs::symlink_file(&target, &link).unwrap();
        assert!(snapshot(link.to_str().unwrap(), 1000, false, false, &cancel()).is_err());
        let before = snapshot(link.to_str().unwrap(), 1000, false, true, &cancel()).unwrap();
        assert_eq!(before.kind, "symlink");
        delete(link.to_str().unwrap(), &before.version, 1000, &cancel()).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"outside");
        assert!(std::fs::symlink_metadata(&link).is_err());
        let directory = canonical.join("directory");
        std::fs::create_dir(&directory).unwrap();
        let dir_link = canonical.join("directory-link");
        std::os::windows::fs::symlink_dir(&directory, &dir_link).unwrap();
        assert!(snapshot(
            dir_link.join("file").to_str().unwrap(),
            100,
            false,
            false,
            &cancel()
        )
        .is_err());
        let before = snapshot(dir_link.to_str().unwrap(), 1000, false, true, &cancel()).unwrap();
        delete(dir_link.to_str().unwrap(), &before.version, 1000, &cancel()).unwrap();
        assert!(directory.is_dir());
    }

    #[test]
    fn changed_ancestor_and_precommit_cancellation_preserve_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let directory = canonical.join("parent");
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("file");
        std::fs::write(&path, b"same").unwrap();
        let before = snapshot(path.to_str().unwrap(), 100, false, false, &cancel()).unwrap();
        std::fs::rename(&directory, canonical.join("old-parent")).unwrap();
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(&path, b"same").unwrap();
        assert!(replace(
            path.to_str().unwrap(),
            b"new",
            &before.version,
            100,
            None,
            &cancel(),
            None
        )
        .is_err());
        let before = snapshot(path.to_str().unwrap(), 100, false, false, &cancel()).unwrap();
        assert!(delete(
            path.to_str().unwrap(),
            &before.version,
            100,
            &AtomicBool::new(true)
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"same");
    }
}
