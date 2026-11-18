//! Fixtures shared by the engine's unit tests.
use std::path::{Path, PathBuf};

pub(crate) mod clock;

/// A fresh temporary directory, removed (with everything under it) on drop.
/// `.0` is its path; it also derefs to a [`Path`].
#[derive(Debug)]
pub(crate) struct TmpDir(
    pub PathBuf,
    #[expect(dead_code, reason = "held only to remove the directory on drop")] tempfile::TempDir,
);

impl std::ops::Deref for TmpDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TmpDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl TmpDir {
    pub(crate) fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("octocode-engine-test-")
            .tempdir()
            .expect("temp dir");
        Self(dir.path().to_path_buf(), dir)
    }

    /// Write `content` to `relative` under the root, creating its parents.
    pub(crate) fn write(&self, relative: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, content).expect("write file");
        path
    }

    /// The root as a string, as engine options take paths.
    pub(crate) fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

/// Whether `pid` is a running process (Windows; Unix tests use `kill(pid, 0)`).
#[cfg(windows)]
pub(crate) fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the handle is owned here and closed exactly once.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let queried = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        queried && i32::try_from(code) == Ok(STILL_ACTIVE)
    }
}

/// Polls until `pid` is gone; `false` when it is still running after `within`.
#[cfg(windows)]
pub(crate) fn process_gone_within(pid: u32, within: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    while process_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    !process_alive(pid)
}
