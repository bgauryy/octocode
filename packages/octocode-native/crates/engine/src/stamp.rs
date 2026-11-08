//! File freshness proof: a source's size and change times, the settle
//! window after which they can stand in for its bytes, and the one [`Proof`]
//! that what a kept result read has not changed.
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

mod proof;
#[cfg(any(test, feature = "test-hooks"))]
pub use proof::stamp_stats;
pub use proof::{Link, Proof, cache_stamp, exists};

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
/// thread that called [`settle_written_so_far`], at least that call's time
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
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000_000;

    /// A file written now, and the same file dated an hour ahead: the second
    /// is unsettled on any clock reading before then, with no sleep.
    fn write_dated(path: &std::path::Path, hours_ahead: bool) -> std::fs::Metadata {
        std::fs::write(path, "export const a = 1;").expect("write fixture");
        if hours_ahead {
            let file = std::fs::File::options()
                .write(true)
                .open(path)
                .expect("open");
            file.set_modified(settle_clock() + Duration::from_secs(3600))
                .expect("future mtime");
        }
        std::fs::metadata(path).expect("meta")
    }

    const fn stamp_at(modified_ns: u64, changed_ns: u64) -> SourceStamp {
        SourceStamp {
            size: 1,
            modified_ns,
            changed_ns,
            inode: 0,
        }
    }

    /// The window follows the granularity of the stamp's own times: twice the
    /// larger of that and the clock tick, so whole seconds wait two, and a
    /// nanosecond stamp waits twice the tick.
    #[test]
    fn the_window_is_twice_the_coarser_of_granularity_and_tick() {
        let tick = SourceStamp::COARSE_CLOCK_TICK;
        assert_eq!(
            stamp_at(10 * SECOND, 10 * SECOND).settle_for(),
            SourceStamp::SETTLE
        );
        assert_eq!(stamp_at(0, 0).settle_for(), SourceStamp::SETTLE);
        assert_eq!(
            stamp_at(10 * SECOND + 7, 10 * SECOND + 7).settle_for(),
            tick * 2
        );
        let exfat = 10 * SECOND + 10_000_000;
        assert_eq!(
            stamp_at(exfat, exfat).settle_for(),
            tick * 2,
            "10 ms is under the tick"
        );
        let tenth = 10 * SECOND + 500_000_000;
        assert_eq!(
            stamp_at(tenth, tenth).settle_for(),
            Duration::from_millis(200),
            "a granule of 100 ms, twice over"
        );

        assert!(SourceStamp::SETTLE >= tick * 2);
    }

    /// Two times are one write only when the coarse one is the truncation of
    /// the fine one: it lies within its own granule below it. Then the coarse
    /// granularity sets the window (vfat keeps mtime to 2 s and ctime at its
    /// creation: reloaded after the inode is evicted, ctime goes back while
    /// the mtime stays truncated). A coarse time further below is another
    /// clock's (a restored mtime), and the fine one decides.
    #[test]
    fn a_coarse_time_truncating_the_fine_one_sets_the_window() {
        let tick = SourceStamp::COARSE_CLOCK_TICK;
        let coarse = SourceStamp::SETTLE;
        let ten = 10 * SECOND;
        let window = |modified, changed| stamp_at(modified, changed).settle_for();
        assert_eq!(
            window(ten, ten + 370_000_000),
            coarse,
            "vfat: 10.0 and 10.37"
        );
        assert_eq!(
            window(ten + 370_000_000, ten),
            coarse,
            "ctime the truncated one"
        );
        assert_eq!(window(ten, ten + 999_999_999), coarse, "inside the granule");
        assert_eq!(
            window(ten, ten + SECOND + 370_000_000),
            coarse,
            "vfat's 2 s granule: created at 11.37, written at 11.9, mtime 10.0"
        );
        assert_eq!(
            window(ten, ten + 2 * SECOND + 1),
            tick * 2,
            "past the 2 s granule: fine"
        );
        assert_eq!(
            window(ten, ten + 86_400 * SECOND + 7),
            tick * 2,
            "restored mtime"
        );
        assert_eq!(
            window(ten + 100_000 * SECOND + 7, ten),
            tick * 2,
            "fine mtime, much older whole-second ctime"
        );
        assert_eq!(
            window(ten + 7, ten),
            coarse,
            "a fine time within the granule above a whole-second one is one write"
        );
        assert_eq!(
            window(ten + 2 * SECOND + 7, ten),
            tick * 2,
            "older coarse time past its granule"
        );
        assert_eq!(
            window(ten + SECOND, ten + 370_000_000),
            coarse,
            "coarse newer than fine"
        );
        assert_eq!(window(ten, ten), coarse, "equal whole seconds");
        assert_eq!(window(ten + 7, ten + 7), tick * 2, "equal fine times");
    }

    /// A stamp is settled exactly at its window, no earlier.
    #[test]
    fn a_stamp_settles_at_its_window() {
        let fine = stamp_at(10 * SECOND + 7, 10 * SECOND + 7);
        let window = fine.settle_for().as_nanos() as u64;
        assert!(!fine.settled_by(fine.last_write_ns() + window - 1));
        assert!(fine.settled_by(fine.last_write_ns() + window));
        let coarse = stamp_at(10 * SECOND, 10 * SECOND);
        let window = SourceStamp::SETTLE.as_nanos() as u64;
        assert!(!coarse.settled_by(10 * SECOND + window - 1));
        assert!(coarse.settled_by(10 * SECOND + window));
    }

    /// The window is a property of the host's clock: a stamp never lags the
    /// real time of its write by more than its own window, so a later write
    /// cannot keep a settled stamp. Written repeatedly, every stamp is at
    /// most one window behind the instant before its write.
    #[test]
    fn a_stamp_lags_its_write_by_less_than_its_window() {
        let fixture = tempfile::tempdir().expect("fixture");
        let path = fixture.path().join("a.ts");
        for round in 0..200 {
            let before = epoch_nanos(settle_clock()).expect("clock");
            std::fs::write(&path, format!("export const a = {round};")).expect("write fixture");
            let stamp = SourceStamp::of(&std::fs::metadata(&path).expect("meta")).expect("stamp");
            let window = stamp.settle_for().as_nanos() as u64;
            assert!(
                stamp.last_write_ns() + window >= before,
                "round {round}: stamp {} lags {before} by more than {window}",
                stamp.last_write_ns()
            );
        }
    }

    /// The test clock settles what was written before the call, and only
    /// that: a file dated ahead is never settled, whatever the clock says,
    /// and another thread keeps the real clock.
    #[test]
    fn the_test_clock_settles_earlier_writes_on_its_own_thread() {
        let fixture = tempfile::tempdir().expect("fixture");
        let before = fixture.path().join("before.ts");
        let meta = |path: &std::path::Path| std::fs::metadata(path).expect("meta");
        write_dated(&before, false);
        write_dated(&fixture.path().join("ahead.ts"), true);
        let ahead = fixture.path().join("ahead.ts");
        settle_written_so_far();
        assert!(SourceStamp::settled(&meta(&before), settle_clock()).is_some());
        assert_eq!(
            SourceStamp::settled(&meta(&ahead), settle_clock()),
            None,
            "a file written inside its window is never settled"
        );
        let other = std::thread::spawn(|| SETTLED_THROUGH.with(std::cell::Cell::get));
        assert_eq!(other.join().expect("thread"), None, "thread scoped");
        let through = settled_through();
        assert!(through.is_some());
        let worker = std::thread::spawn(move || {
            with_settled_through(through, || SETTLED_THROUGH.with(std::cell::Cell::get))
        });
        assert_eq!(
            worker.join().expect("thread"),
            through,
            "handed to a worker"
        );
    }
}
