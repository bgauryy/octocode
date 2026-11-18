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
