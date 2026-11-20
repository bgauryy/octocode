//! File freshness proof: a source's size and change times, the settle
//! window after which they can stand in for its bytes, the one [`Proof`]
//! that what a kept result read has not changed, and one request's held
//! proofs ([`observing`]), so a path one of them stamped costs no second stat.
//!
//! The settle window is twice the granularity of the clock behind a stamp's
//! last write, at least twice [`SourceStamp::COARSE_CLOCK_TICK`]; a stamp of
//! whole seconds waits [`SourceStamp::SETTLE`]. Two times are one write when
//! the coarser lies within its own granule below the finer (a whole second
//! counts as 2 s, since a 2 s clock also stamps whole seconds): the coarse
//! one then decides, as on vfat (modification time truncated to 2 s, change
//! time at its creation). A coarse time further below is a restored modification
//! time, and the finer one decides.
//!
//! Known limits. A network file system stamps with the server's clock, a
//! virtual machine's timer can stall, and a clock skew between the window
//! and 2 s defeats the window: such a write can settle too early, and no
//! local rule detects it. On Windows the change field holds file attributes
//! (small values), never later than the modification time, so the
//! modification time decides.
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod observed;
mod proof;
pub use observed::{Held, Observed, held, observation, observing};
pub use proof::{Checked, Link, Proof, cache_stamp, exists};
#[cfg(any(test, feature = "test-hooks"))]
pub use proof::{stamp_stats, stamp_stats_of};

/// A source file's size and change times when its content was hashed. A
/// later read compares these before hashing again; equal stamps prove the
/// bytes are unchanged without reading them.
///
/// Git's stat cache carries the same race: a file rewritten within the clock
/// tick of the read keeps its time. [`SourceStamp::settled`] therefore
/// records a stamp only for a file untouched for [`SourceStamp::settle_for`],
/// so any later write moves its time past the stored one. On Unix the status
/// change time and inode also change on a write that restores the old
/// modification time, or on a replace by rename. Windows has neither without a
/// handle per file: its file attributes stand in, so `attrib +r` moves the stamp
/// though a same-size edit that restores its time does not.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceStamp {
    pub size: u64,
    /// Modification time, nanoseconds since the Unix epoch.
    pub modified_ns: u64,
    /// Status change time in nanoseconds (Unix), or the file attributes
    /// (Windows), else 0.
    pub changed_ns: u64,
    /// Inode number (Unix), else 0.
    pub inode: u64,
}

impl SourceStamp {
    /// The longest settle window any stamp needs: two ticks of the coarsest
    /// common file time (FAT keeps whole seconds, so a stamp of whole seconds
    /// waits two).
    pub const SETTLE: Duration = Duration::from_secs(2);

    /// The longest tick of the coarse clocks that stamp a write, which is the
    /// most a stamp can lag the real time of its write: Linux stamps files
    /// from `CLOCK_REALTIME_COARSE`, one jiffy (10 ms at the lowest configured
    /// `HZ` of 100); Windows system time ticks 15.625 ms; APFS and other
    /// nanosecond file systems stamp from a fine clock. Rounded up to 20 ms.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "a property of the platforms' file clocks, not a work budget: no limit or deadline derives it"
        )
    )]
    pub const COARSE_CLOCK_TICK: Duration = Duration::from_millis(20);

    /// The later of the modification and status change times.
    #[must_use]
    pub fn last_write_ns(&self) -> u64 {
        self.modified_ns.max(self.changed_ns)
    }

    /// The granularity of the clock behind this stamp's last write: the
    /// largest power of ten, up to one second, that divides a time. Of the
    /// two times, the later one decides, except when the earlier one is
    /// coarser and lies within its own granule below it (2 s for a whole
    /// second, which a 2 s clock also stamps): then it is the
    /// truncation of the same write, and the coarse granularity decides
    /// (vfat keeps the modification time to 2 s and the change time at its
    /// creation, which a reload after inode eviction restores). An earlier
    /// coarse time further below is another clock's, such as a modification
    /// time restored by `cp -p`, `tar` or `git`, and says nothing about this
    /// write.
    fn granularity_ns(&self) -> u64 {
        let unit = |mut nanos: u64| {
            let mut unit = 1;
            while unit < NANOS_PER_SECOND && nanos.is_multiple_of(10) {
                nanos /= 10;
                unit *= 10;
            }
            unit
        };
        let (later, earlier) = (
            self.modified_ns.max(self.changed_ns),
            self.modified_ns.min(self.changed_ns),
        );
        let (fine, coarse) = (unit(later), unit(earlier));
        // A whole-second time may come from a 2 s clock (FAT), whose granule
        // the digits cannot show: the same write can lie up to SETTLE above it.
        let span = if coarse == NANOS_PER_SECOND {
            u64::try_from(Self::SETTLE.as_nanos()).unwrap_or(u64::MAX)
        } else {
            coarse
        };
        if coarse > fine && later - earlier < span {
            coarse
        } else {
            fine
        }
    }

    /// The settle window this stamp needs: twice the larger of its own
    /// granularity ([`SourceStamp::granularity_ns`]) and
    /// [`SourceStamp::COARSE_CLOCK_TICK`]. A later write gets a time at most
    /// one granule or tick behind its real time, so a stamp older than two
    /// of them cannot hide it. A whole-second stamp gets
    /// [`SourceStamp::SETTLE`]. The one rule for every settle check.
    #[must_use]
    pub fn settle_for(&self) -> Duration {
        let tick = u64::try_from(Self::COARSE_CLOCK_TICK.as_nanos()).unwrap_or(u64::MAX);
        Duration::from_nanos(2 * self.granularity_ns().max(tick))
    }

    /// Whether this stamp's file was last written at least its
    /// [`SourceStamp::settle_for`] window before `now_ns` (nanoseconds since
    /// the epoch).
    #[must_use]
    pub fn settled_by(&self, now_ns: u64) -> bool {
        // A thread that called `settle_written_so_far` reads a clock of at
        // least that call plus SETTLE and judges by the longest window: what
        // was written before the call is settled, and what is written after
        // it settles only once the real time passes the window, so tests
        // are deterministic about writes after the call.
        #[cfg(any(test, feature = "test-hooks"))]
        if SETTLED_THROUGH.with(std::cell::Cell::get).is_some() {
            let window = u64::try_from(Self::SETTLE.as_nanos()).unwrap_or(u64::MAX);
            return self.last_write_ns().saturating_add(window) <= now_ns;
        }
        let window = u64::try_from(self.settle_for().as_nanos()).unwrap_or(u64::MAX);
        self.last_write_ns().saturating_add(window) <= now_ns
    }

    /// The stamp of `meta`, or `None` when the platform reports no
    /// modification time after the epoch.
    #[must_use]
    pub fn of(meta: &std::fs::Metadata) -> Option<Self> {
        let modified_ns = epoch_nanos(meta.modified().ok()?)?;
        #[cfg(unix)]
        let (changed_ns, inode) = {
            use std::os::unix::fs::MetadataExt as _;
            let changed = u64::try_from(meta.ctime())
                .ok()
                .zip(u64::try_from(meta.ctime_nsec()).ok())
                .and_then(|(secs, nanos)| secs.checked_mul(1_000_000_000)?.checked_add(nanos));
            (changed?, meta.ino())
        };
        #[cfg(windows)]
        let (changed_ns, inode) = {
            use std::os::windows::fs::MetadataExt as _;
            (u64::from(meta.file_attributes()), 0)
        };
        #[cfg(not(any(unix, windows)))]
        let (changed_ns, inode) = (0, 0);
        Some(Self {
            size: meta.len(),
            modified_ns,
            changed_ns,
            inode,
        })
    }

    /// [`SourceStamp::of`] when the file was last written at least
    /// [`SourceStamp::settle_for`] before `now` (a [`settle_clock`] reading);
    /// `None` keeps the content hash as the only proof (a racily clean or
    /// future-dated file).
    #[must_use]
    pub fn settled(meta: &std::fs::Metadata, now: SystemTime) -> Option<Self> {
        let stamp = Self::of(meta)?;
        let now_ns = epoch_nanos(now)?;
        stamp.settled_by(now_ns).then_some(stamp)
    }
}

const NANOS_PER_SECOND: u64 = 1_000_000_000;

fn epoch_nanos(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())
}

#[cfg(any(test, feature = "test-hooks"))]
thread_local! {
    /// The instant [`settle_written_so_far`] last ran on this thread.
    static SETTLED_THROUGH: std::cell::Cell<Option<SystemTime>> = const { std::cell::Cell::new(None) };
}

/// The system clock: the module's one reading of it.
fn wall_clock() -> SystemTime {
    SystemTime::now()
}

/// The time a settle check compares against: the system clock. On a test
/// thread that called `settle_written_so_far`, at least that call's time
/// plus [`SourceStamp::SETTLE`] (the longest window any stamp needs), so every file written before the call is
/// settled on any file system, exactly as if the thread had slept through
/// the window. A file written later settles within that window too, so a test
/// that needs an unsettled file dates it ahead. A file whose time is in the future is
/// never settled, which is how a test builds an unsettled file without a sleep.
#[must_use]
pub fn settle_clock() -> SystemTime {
    let now = wall_clock();
    #[cfg(any(test, feature = "test-hooks"))]
    if let Some(through) = SETTLED_THROUGH.with(std::cell::Cell::get) {
        return now.max(through + SourceStamp::SETTLE);
    }
    now
}

/// Settles every file this thread has written so far, for the settle checks
/// this thread's [`settle_clock`] readings decide, without sleeping through
/// the settle window. Scoped to the calling thread, so parallel tests
/// never see each other's clock.
#[cfg(any(test, feature = "test-hooks"))]
pub fn settle_written_so_far() {
    SETTLED_THROUGH.with(|cell| cell.set(Some(wall_clock())));
}

/// The instant [`settle_written_so_far`] last ran on the calling thread, for
/// work it hands to another thread: the worker runs inside
/// [`with_settled_through`] with this value, so its settle checks agree with
/// the caller's, and only the caller's.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn settled_through() -> Option<SystemTime> {
    SETTLED_THROUGH.with(std::cell::Cell::get)
}

/// Runs `work` with `through` as this thread's settle-through instant (see
/// [`settled_through`]), restoring the previous value afterwards.
#[cfg(any(test, feature = "test-hooks"))]
pub fn with_settled_through<T>(through: Option<SystemTime>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<SystemTime>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SETTLED_THROUGH.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(SETTLED_THROUGH.with(|cell| cell.replace(through)));
    work()
}

#[cfg(test)]
mod tests;
