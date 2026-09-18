pub(crate) fn is_alive(pid: u32) -> bool {
    platform::is_alive(pid)
}

#[cfg(unix)]
mod platform {
    pub(super) fn is_alive(pid: u32) -> bool {
        if pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: signal zero checks existence/permission without mutating the process.
        let result = unsafe { libc::kill(pid as i32, 0) };
        result == 0
            || std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub(super) fn is_alive(pid: u32) -> bool {
        // SAFETY: the returned handle is owned here and closed exactly once.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0;
        // SAFETY: handle is valid and exit_code points to writable storage.
        let queried = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0;
        // SAFETY: handle is non-null and owned by this function.
        unsafe { CloseHandle(handle) };
        queried && exit_code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    pub(super) fn is_alive(_pid: u32) -> bool {
        true
    }
}
