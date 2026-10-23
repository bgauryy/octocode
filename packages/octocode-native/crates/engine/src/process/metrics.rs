//! Resident memory and CPU time of a process and its tree.

#[cfg(target_os = "linux")]
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
use super::descendants;
#[cfg(target_os = "linux")]
use super::proc_table::{read_proc_cpu_ticks, read_proc_rss};

/// Kernel task info of `pid`; `None` when the process is gone.
#[cfg(target_os = "macos")]
fn task_info(pid: u32) -> Option<libc::proc_taskinfo> {
    let pid = libc::c_int::try_from(pid).ok()?;
    // SAFETY: `proc_taskinfo` is plain data; zero is a valid initial value.
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: the buffer is a live `proc_taskinfo` of exactly `size` bytes,
    // the layout `PROC_PIDTASKINFO` writes.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    (written == size).then_some(info)
}

/// Resident set size of `pid` in bytes; `None` when the process is gone.
#[cfg(target_os = "macos")]
pub(super) fn rss_bytes(pid: u32) -> Option<u64> {
    task_info(pid).map(|info| info.pti_resident_size)
}

/// `mach_timebase_info_data_t` from `<mach/mach_time.h>`.
#[cfg(target_os = "macos")]
#[repr(C)]
struct MachTimebase {
    numer: u32,
    denom: u32,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    // `libc` deprecates its binding in favour of the `mach2` crate, which
    // this one call does not justify as a dependency.
    fn mach_timebase_info(info: *mut MachTimebase) -> libc::c_int;
}

/// User plus system CPU time of `pid`, `None` when the process is gone.
/// `proc_taskinfo` reports mach absolute-time ticks, which are nanoseconds
/// only on Intel; the timebase converts them everywhere.
#[cfg(target_os = "macos")]
fn cpu_time(pid: u32) -> Option<Duration> {
    use std::sync::OnceLock;
    static TIMEBASE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
    let (numer, denom) = (*TIMEBASE.get_or_init(|| {
        let mut base = MachTimebase { numer: 0, denom: 0 };
        // SAFETY: `base` is a live out-parameter of the C struct's layout.
        let status = unsafe { mach_timebase_info(&raw mut base) };
        (status == 0 && base.denom != 0).then_some((base.numer, base.denom))
    }))?;
    let info = task_info(pid)?;
    let ticks = u128::from(info.pti_total_user) + u128::from(info.pti_total_system);
    let nanos = ticks * u128::from(numer) / u128::from(denom);
    Some(Duration::from_nanos(
        u64::try_from(nanos).unwrap_or(u64::MAX),
    ))
}

#[cfg(target_os = "linux")]
pub(super) fn rss_bytes(pid: u32) -> Option<u64> {
    // SAFETY: `sysconf` has no preconditions.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    read_proc_rss(Path::new("/proc"), pid, page_size)
}

#[cfg(target_os = "linux")]
fn cpu_time(pid: u32) -> Option<Duration> {
    // SAFETY: `sysconf` has no preconditions.
    let ticks_per_second = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
        .ok()
        .filter(|&ticks| ticks > 0)?;
    let ticks = read_proc_cpu_ticks(Path::new("/proc"), pid)?;
    Some(Duration::from_nanos(
        ticks.saturating_mul(1_000_000_000 / ticks_per_second),
    ))
}

/// Cumulative CPU time of a server's process tree (off macOS and Linux, of
/// the server process alone), comparable between reads of the same clock.
///
/// A process that exits takes its own counters with it, so the clock keeps
/// the last CPU time it saw of every process that disappeared: the total
/// never falls when a busy helper (`cargo`, a proc-macro server) finishes.
/// On Linux a reaped child's time also reaches its parent's `cutime`, so it
/// can be counted twice, once; the clock only has to grow when work was done.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub(crate) struct TreeCpuClock {
    root: u32,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    seen: std::sync::Mutex<SeenCpu>,
}

/// The CPU time last seen per live process, and the sum of what the
/// processes that have exited used.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Default)]
struct SeenCpu {
    live: std::collections::HashMap<u32, Duration>,
    exited: Duration,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl TreeCpuClock {
    pub(crate) fn new(root: u32) -> Self {
        Self {
            root,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            seen: std::sync::Mutex::default(),
        }
    }

    /// `None` when the root process itself cannot be read (it exited).
    #[cfg(windows)]
    pub(crate) fn read(&self) -> Option<Duration> {
        cpu_time(self.root)
    }

    /// `None` when the root process itself cannot be read (it exited).
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn read(&self) -> Option<Duration> {
        let mut now = std::collections::HashMap::from([(self.root, cpu_time(self.root)?)]);
        for pid in descendants(self.root) {
            if let Some(time) = cpu_time(pid) {
                now.insert(pid, time);
            }
        }
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let gone = seen
            .live
            .iter()
            .filter(|(pid, _)| !now.contains_key(pid))
            .map(|(_, time)| *time)
            .fold(Duration::ZERO, Duration::saturating_add);
        seen.exited = seen.exited.saturating_add(gone);
        seen.live = now;
        Some(
            seen.live
                .values()
                .fold(seen.exited, |total, time| total.saturating_add(*time)),
        )
    }
}

/// CPU time used by the process `root` (no tree walk off macOS and Linux);
/// `None` when it cannot be opened.
#[cfg(windows)]
fn cpu_time(root: u32) -> Option<Duration> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let filetime_100ns =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    // SAFETY: a plain open by pid; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, root) };
    if process.is_null() {
        return None;
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `process` is a live handle and the four out-parameters are
    // live `FILETIME`s.
    let read = unsafe {
        GetProcessTimes(
            process,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    } != 0;
    // SAFETY: `process` was opened above and is closed exactly once.
    unsafe { CloseHandle(process) };
    read.then(|| {
        Duration::from_nanos(
            filetime_100ns(kernel)
                .saturating_add(filetime_100ns(user))
                .saturating_mul(100),
        )
    })
}

/// Resident memory of `root` plus every descendant, in bytes; `None` when
/// `root` itself cannot be read (it exited).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn tree_rss_bytes(root: u32) -> Option<u64> {
    let own = rss_bytes(root)?;
    Some(
        descendants(root)
            .into_iter()
            .filter_map(rss_bytes)
            .fold(own, u64::saturating_add),
    )
}
