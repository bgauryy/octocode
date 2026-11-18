//! Thread CPU time for complexity regression tests.
//!
//! A wall-clock bound fails under parallel test load (other processes take the
//! core), while the work the code under test does is unchanged. The calling
//! thread's CPU time measures that work, so a quadratic regression still trips
//! the bound and a busy machine does not.

use std::time::Duration;

/// CPU time the calling thread has used so far.
#[cfg(unix)]
pub(crate) fn thread_cpu_time() -> Duration {
    let mut spec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `spec` is a valid, writable timespec, and CLOCK_THREAD_CPUTIME_ID
    // is supported on every unix target this crate builds for.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut spec) };
    assert_eq!(status, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(spec.tv_sec as u64, spec.tv_nsec as u32)
}

/// Non-unix hosts have no per-thread clock here: fall back to a monotonic
/// clock, a wall-clock measure.
#[cfg(not(unix))]
pub(crate) fn thread_cpu_time() -> Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// CPU time `work` takes on the calling thread, with its result.
pub(crate) fn cpu_time_of<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let started = thread_cpu_time();
    let value = work();
    (value, thread_cpu_time().saturating_sub(started))
}

#[cfg(all(test, unix))]
mod tests;
