//! Process identity: a pid paired with the OS start time that tells a process
//! apart from a later one the kernel hands the same pid, and signal delivery
//! that reaches only the process a tree walk saw.

/// One process as a tree walk saw it. A pid alone is not an identity: once
/// the process exits and its parent reaps it, the kernel may give the pid to
/// an unrelated process. `start` was read in the same OS record as the
/// parent link that made the process a descendant, so the pair names exactly
/// that process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Process {
    pub(crate) pid: u32,
    /// OS start time: microseconds since the epoch on macOS, clock ticks
    /// since boot on Linux. Only compared for equality.
    pub(crate) start: u64,
}

/// What tree teardown needs from the OS. [`LiveTable`] is the real table;
/// tests script one to replay a pid being recycled mid-teardown.
#[cfg(unix)]
pub(super) trait ProcessTable {
    /// A hold on whichever process has a pid right now.
    type Handle;
    /// Every descendant of `root`, as [`super::descendants`] reports it.
    fn descendants(&self, root: u32) -> Vec<Process>;
    /// Hold the process that has `pid` now; `None` when no process has it.
    fn pin(&self, pid: u32) -> Option<Self::Handle>;
    /// Start time of the process that has `pid` now.
    fn start(&self, pid: u32) -> Option<u64>;
    /// Signal the held process.
    fn send(&self, handle: &Self::Handle, signal: libc::c_int);
    /// Signal a pid, or a process group when negative, that the caller keeps
    /// from being recycled: its own child, not yet reaped.
    fn signal_unreaped(&self, target: libc::pid_t, signal: libc::c_int);
}

/// Deliver `signal` to `process` only while its pid still names it, and say
/// whether it was delivered. The process is held first and its start time
/// compared after: on Linux the hold is a pidfd, so the signal reaches the
/// held process or no one. Elsewhere the hold is the pid itself, and a pid
/// recycled between the comparison and the signal (one `kill` call later)
/// is the one remaining window; a stopped process cannot exit on its own, so
/// the kill of a frozen tree has none.
#[cfg(unix)]
pub(super) fn signal_process<T: ProcessTable>(
    table: &T,
    process: Process,
    signal: libc::c_int,
) -> bool {
    // 0 and 1 are never descendants; `kill(0, …)` would hit our own group.
    if process.pid <= 1 {
        return false;
    }
    let Some(handle) = table.pin(process.pid) else {
        return false;
    };
    if table.start(process.pid) != Some(process.start) {
        return false;
    }
    table.send(&handle, signal);
    true
}

/// The operating system's process table.
#[cfg(unix)]
pub(super) struct LiveTable;

#[cfg(unix)]
impl ProcessTable for LiveTable {
    #[cfg(target_os = "linux")]
    type Handle = linux::Pin;
    #[cfg(not(target_os = "linux"))]
    type Handle = libc::pid_t;

    fn descendants(&self, root: u32) -> Vec<Process> {
        super::descendants(root)
    }

    #[cfg(target_os = "linux")]
    fn pin(&self, pid: u32) -> Option<Self::Handle> {
        linux::pin(pid)
    }

    #[cfg(not(target_os = "linux"))]
    fn pin(&self, pid: u32) -> Option<Self::Handle> {
        libc::pid_t::try_from(pid).ok()
    }

    #[cfg(target_os = "linux")]
    fn start(&self, pid: u32) -> Option<u64> {
        super::proc_table::read_proc_stat(std::path::Path::new("/proc"), pid).map(|stat| stat.start)
    }

    #[cfg(target_os = "macos")]
    fn start(&self, pid: u32) -> Option<u64> {
        bsd_record(pid).map(|(_, start)| start)
    }

    /// No start-time reader here (and no tree walk), so no descendant is
    /// ever signaled by pid.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn start(&self, _pid: u32) -> Option<u64> {
        None
    }

    #[cfg(target_os = "linux")]
    fn send(&self, handle: &Self::Handle, signal: libc::c_int) {
        linux::send(handle, signal);
    }

    #[cfg(not(target_os = "linux"))]
    fn send(&self, handle: &Self::Handle, signal: libc::c_int) {
        self.signal_unreaped(*handle, signal);
    }

    fn signal_unreaped(&self, target: libc::pid_t, signal: libc::c_int) {
        // SAFETY: plain signal delivery; the caller chose the target.
        unsafe {
            libc::kill(target, signal);
        }
    }
}

/// `(parent pid, start time in microseconds)` of `pid`, read from one
/// `proc_bsdinfo` record; `None` when the process is gone.
#[cfg(target_os = "macos")]
pub(super) fn bsd_record(pid: u32) -> Option<(u32, u64)> {
    let raw = libc::c_int::try_from(pid).ok()?;
    // SAFETY: `proc_bsdinfo` is plain data; zero is a valid initial value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer is a live `proc_bsdinfo` of exactly `size` bytes,
    // the layout `PROC_PIDTBSDINFO` writes.
    let written =
        unsafe { libc::proc_pidinfo(raw, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    (written == size && info.pbi_pid == pid).then(|| {
        let micros = info
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbi_start_tvusec);
        (info.pbi_ppid, micros)
    })
}

/// Linux holds a process with a pidfd, which keeps naming that process after
/// it exits, so a signal through it cannot reach a later holder of the pid.
#[cfg(target_os = "linux")]
pub(super) mod linux {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    pub(crate) enum Pin {
        Fd(OwnedFd),
        /// The kernel refused a pidfd (before 5.3, a seccomp filter, or no
        /// free descriptor): the start-time comparison alone guards the pid.
        Pid(libc::pid_t),
    }

    pub(super) fn pin(pid: u32) -> Option<Pin> {
        let raw = libc::pid_t::try_from(pid).ok()?;
        // SAFETY: `pidfd_open(pid, 0)` returns a new descriptor or -1.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, raw, 0) };
        if let Ok(fd) = libc::c_int::try_from(fd)
            && fd >= 0
        {
            // SAFETY: the syscall just returned this descriptor; we own it.
            return Some(Pin::Fd(unsafe { OwnedFd::from_raw_fd(fd) }));
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => None,
            _ => Some(Pin::Pid(raw)),
        }
    }

    pub(super) fn send(pin: &Pin, signal: libc::c_int) {
        match pin {
            // SAFETY: `pidfd_send_signal(fd, sig, NULL, 0)` on a live pidfd.
            Pin::Fd(fd) => unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            },
            // SAFETY: plain signal delivery to a positive pid.
            Pin::Pid(pid) => unsafe {
                libc::kill(*pid, signal);
            },
        }
    }
}
