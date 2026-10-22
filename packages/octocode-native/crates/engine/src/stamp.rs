//! File freshness proof: a source's size and change times, and the settle
//! window after which they can stand in for its bytes.
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A source file's size and change times when its content was hashed. A
/// later read compares these before hashing again; equal stamps prove the
/// bytes are unchanged without reading them.
///
/// Git's stat cache carries the same race: a file rewritten within the clock
/// tick of the read keeps its time. [`SourceStamp::settled`] therefore
/// records a stamp only for a file untouched for [`SourceStamp::SETTLE`], so
/// any later write moves its time past the stored one. On Unix the status
/// change time and inode also change on a write that restores the old
/// modification time, or on a replace by rename.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceStamp {
    pub size: u64,
    /// Modification time, nanoseconds since the Unix epoch.
    pub modified_ns: u64,
    /// Status change time in nanoseconds (Unix), else 0.
    pub changed_ns: u64,
    /// Inode number (Unix), else 0.
    pub inode: u64,
}

impl SourceStamp {
    /// How long a file must be untouched before its stamp can stand in for
    /// its content: two clock ticks of the coarsest common file time (FAT).
    pub const SETTLE: Duration = Duration::from_secs(2);

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
        #[cfg(not(unix))]
        let (changed_ns, inode) = (0, 0);
        Some(Self {
            size: meta.len(),
            modified_ns,
            changed_ns,
            inode,
        })
    }

    /// [`SourceStamp::of`] when the file was last written at least
    /// [`SourceStamp::SETTLE`] before `now` (a [`settle_clock`] reading);
    /// `None` keeps the content hash as the only proof (a racily clean or
    /// future-dated file).
    #[must_use]
    pub fn settled(meta: &std::fs::Metadata, now: SystemTime) -> Option<Self> {
        let stamp = Self::of(meta)?;
        let now_ns = epoch_nanos(now)?;
        let settle = u64::try_from(Self::SETTLE.as_nanos()).unwrap_or(u64::MAX);
        let last_write = stamp.modified_ns.max(stamp.changed_ns);
        (last_write.saturating_add(settle) <= now_ns).then_some(stamp)
    }
}

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

/// The time a settle check compares against: the system clock. On a test
/// thread that called [`settle_written_so_far`], at least that call's time
/// plus [`SourceStamp::SETTLE`], so every file written before the call is
/// settled and every later write still waits out the window, exactly as if
/// the thread had slept through it.
#[must_use]
pub fn settle_clock() -> SystemTime {
    let now = SystemTime::now();
    #[cfg(any(test, feature = "test-hooks"))]
    if let Some(through) = SETTLED_THROUGH.with(std::cell::Cell::get) {
        return now.max(through + SourceStamp::SETTLE);
    }
    now
}

/// Settles every file this thread has written so far, for the settle checks
/// this thread's [`settle_clock`] readings decide, without sleeping through
/// [`SourceStamp::SETTLE`]. Scoped to the calling thread, so parallel tests
/// never see each other's clock.
#[cfg(any(test, feature = "test-hooks"))]
pub fn settle_written_so_far() {
    SETTLED_THROUGH.with(|cell| cell.set(Some(SystemTime::now())));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stamp stands in for content only once the file has been untouched
    /// for the settle window.
    #[test]
    fn a_source_stamp_settles_two_seconds_after_the_last_write() {
        let fixture = tempfile::tempdir().expect("fixture");
        let file = fixture.path().join("a.ts");
        std::fs::write(&file, "export const a = 1;").expect("write fixture");
        let meta = std::fs::metadata(&file).expect("meta");
        let stamp = SourceStamp::of(&meta).expect("stamp");
        assert_eq!(stamp.size, 19);
        let last_write = stamp.modified_ns.max(stamp.changed_ns);
        let at = |nanos: u64| UNIX_EPOCH + Duration::from_nanos(nanos);
        let settle = SourceStamp::SETTLE.as_nanos() as u64;
        assert_eq!(
            SourceStamp::settled(&meta, at(last_write + settle - 1)),
            None
        );
        assert_eq!(SourceStamp::settled(&meta, at(last_write)), None);
        assert_eq!(
            SourceStamp::settled(&meta, at(last_write + settle)),
            Some(stamp)
        );
    }

    /// The test clock settles what was written before the call, and only
    /// that: a later write waits out the window again, and another thread
    /// keeps the real clock.
    #[test]
    fn the_test_clock_settles_earlier_writes_on_its_own_thread() {
        let fixture = tempfile::tempdir().expect("fixture");
        let before = fixture.path().join("before.ts");
        std::fs::write(&before, "export const a = 1;").expect("write fixture");
        let meta = || std::fs::metadata(&before).expect("meta");
        assert_eq!(SourceStamp::settled(&meta(), settle_clock()), None);
        settle_written_so_far();
        assert!(SourceStamp::settled(&meta(), settle_clock()).is_some());
        let other = std::thread::spawn(move || {
            SourceStamp::settled(&std::fs::metadata(&before).expect("meta"), settle_clock())
        });
        assert_eq!(other.join().expect("thread"), None, "thread scoped");
        let after = fixture.path().join("after.ts");
        std::fs::write(&after, "export const b = 2;").expect("write fixture");
        let after = std::fs::metadata(&after).expect("meta");
        assert_eq!(SourceStamp::settled(&after, settle_clock()), None);
        settle_written_so_far();
        assert!(SourceStamp::settled(&after, settle_clock()).is_some());
    }
}
