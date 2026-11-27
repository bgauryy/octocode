use super::not_regular;
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io,
    mem::MaybeUninit,
    os::windows::{
        ffi::OsStrExt,
        fs::MetadataExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
    GetFileInformationByHandle, SYNCHRONIZE,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_DISPOSITION_INFORMATION, FILE_INFORMATION_CLASS,
            FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT,
            FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FILE_SYNCHRONOUS_IO_NONALERT,
            FileDispositionInformation, FileRenameInformation, NtCreateFile, NtSetInformationFile,
        },
    },
    Win32::{
        Foundation::{
            OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, RtlNtStatusToDosError, UNICODE_STRING,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

pub(super) fn no_follow(options: &mut OpenOptions) {
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

pub(super) fn reject_reparse(file: &File) -> io::Result<()> {
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(not_regular());
    }
    Ok(())
}

pub(super) fn open_directory(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        // Prevent rename and data-write opens while the chain is held. Attribute
        // opens bypass sharing restrictions; child operations therefore use NtCreateFile.
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    reject_reparse(&file)?;
    if !file.metadata()?.is_dir() {
        return Err(not_regular());
    }
    Ok(file)
}

pub(super) enum FileOpen {
    Read,
    Create,
    Lock,
}

pub(super) fn open_child_directory(parent: &File, name: &OsStr, create: bool) -> io::Result<File> {
    let file = open_at(
        parent,
        name,
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        if create { FILE_OPEN_IF } else { FILE_OPEN },
        FILE_DIRECTORY_FILE,
    )?;
    reject_reparse(&file)?;
    if !file.metadata()?.is_dir() {
        return Err(not_regular());
    }
    Ok(file)
}

pub(super) fn open_child_file(parent: &File, name: &OsStr, mode: FileOpen) -> io::Result<File> {
    let (access, disposition) = match mode {
        FileOpen::Read => (FILE_GENERIC_READ, FILE_OPEN),
        FileOpen::Create => (FILE_GENERIC_WRITE, FILE_CREATE),
        FileOpen::Lock => (FILE_GENERIC_READ | FILE_GENERIC_WRITE, FILE_OPEN_IF),
    };
    let file = open_at(parent, name, access, disposition, FILE_NON_DIRECTORY_FILE)?;
    reject_reparse(&file)?;
    if !file.metadata()?.is_file() {
        return Err(not_regular());
    }
    Ok(file)
}

fn component(name: &OsStr) -> io::Result<Vec<u16>> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(part)) if part == name)
        || parts.next().is_some()
    {
        return Err(not_regular());
    }
    let encoded: Vec<_> = name.encode_wide().collect();
    // A component is not an NTFS alternate stream or a NUL-terminated alias.
    if encoded.contains(&0) || encoded.contains(&u16::from(b':')) {
        return Err(not_regular());
    }
    Ok(encoded)
}

fn open_at(
    parent: &File,
    name: &OsStr,
    access: u32,
    disposition: u32,
    kind: u32,
) -> io::Result<File> {
    let mut name = component(name)?;
    let length = u16::try_from(name.len().checked_mul(2).ok_or_else(not_regular)?)
        .map_err(|_| not_regular())?;
    let name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        ..Default::default()
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    let share = if kind == FILE_DIRECTORY_FILE {
        FILE_SHARE_READ
    } else {
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    };
    // SAFETY: parent owns a live directory handle; the counted UTF-16 buffer and
    // object attributes remain alive for the synchronous call. Both outputs are valid.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            0,
            share,
            disposition,
            kind | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    nt_result(status)?;
    // SAFETY: successful NtCreateFile returned a new handle owned only here.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn nt_result(status: i32) -> io::Result<()> {
    if status >= 0 {
        return Ok(());
    }
    // SAFETY: this pure translation accepts any NTSTATUS value.
    let code = unsafe { RtlNtStatusToDosError(status) };
    Err(io::Error::from_raw_os_error(code as i32))
}

/// # Safety
/// `information` must be aligned, readable for `length` bytes, and contain the
/// complete SDK structure selected by `kind` until the synchronous call returns.
unsafe fn set_information(
    file: &File,
    information: *const std::ffi::c_void,
    length: u32,
    kind: FILE_INFORMATION_CLASS,
) -> io::Result<()> {
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: callers supply a live, correctly aligned SDK structure of length
    // bytes; file is synchronous and its handle stays alive through the call.
    nt_result(unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status_block,
            information,
            length,
            kind,
        )
    })
}

pub(super) fn rename(parent: &File, from: &OsStr, to: &OsStr) -> io::Result<()> {
    let file = open_at(
        parent,
        from,
        DELETE | SYNCHRONIZE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE,
    )?;
    let name = component(to)?;
    let bytes = name.len().checked_mul(2).ok_or_else(not_regular)?;
    let length = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName)
        .checked_add(bytes)
        .ok_or_else(not_regular)?;
    // usize storage has the SDK structure's alignment and enough trailing space
    // for its variable-length UTF-16 FileName (not a wire-query type).
    let mut storage = vec![0_usize; length.div_ceil(std::mem::size_of::<usize>())];
    let information = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let name_bytes = u32::try_from(bytes).map_err(|_| not_regular())?;
    // SAFETY: storage is aligned and large enough for the header and complete
    // filename; source/destination do not overlap and storage lives through the syscall.
    unsafe {
        (*information).Anonymous = FILE_RENAME_INFORMATION_0 {
            Flags: windows_sys::Wdk::Storage::FileSystem::FILE_RENAME_REPLACE_IF_EXISTS,
        };
        (*information).RootDirectory = parent.as_raw_handle();
        (*information).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*information).FileName).cast::<u16>(),
            name.len(),
        );
    }
    let length = u32::try_from(length).map_err(|_| not_regular())?;
    // SAFETY: storage owns the aligned, initialized SDK header and the complete
    // filename; it remains alive for this synchronous rename request.
    unsafe { set_information(&file, information.cast(), length, FileRenameInformation) }
}

pub(super) fn remove(parent: &File, name: &OsStr) -> io::Result<()> {
    let file = open_at(
        parent,
        name,
        DELETE | SYNCHRONIZE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE,
    )?;
    let information = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
    // SAFETY: information is the initialized SDK structure for this class and
    // its reference remains live through the synchronous delete request.
    unsafe {
        set_information(
            &file,
            std::ptr::from_ref(&information).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
            FileDispositionInformation,
        )
    }
}

pub(super) fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: file owns a live handle and information has space for the API's
    // complete output. The output is read only after a successful return.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful GetFileInformationByHandle initialized every field.
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        information.nFileIndexHigh,
        information.nFileIndexLow,
    ))
}

#[cfg(test)]
mod tests;
