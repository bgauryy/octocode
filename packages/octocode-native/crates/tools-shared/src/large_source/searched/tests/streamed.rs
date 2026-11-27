//! A line longer than a window, judged and served from the source in pieces:
//! what a match runs over is the text a read serves, whatever the line's
//! length, charset or break, while the judgment and the served text hold a
//! few windows, never the line, and a cancel ends either within one read.
//! Every value is fake.
use super::super::{Patched, changed_lines};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::cancel::{CancellationCheck, NeverCancel};
use octocode_runtime_base::security::SourceLimits;
use octocode_runtime_base::security::scan::ContentScan;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counts the bytes each thread holds, and the most it held since a reset.
struct Counting;

thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn note(delta: isize) {
    let _ = LIVE.try_with(|live| {
        let now = live.get() + delta;
        live.set(now);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(now)));
    });
}

// SAFETY: every call is the system allocator's, with the same arguments; the
// counts only read the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract, passed on.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            note(layout.size().cast_signed());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract, passed on.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            note(layout.size().cast_signed());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract, passed on.
        unsafe { System.dealloc(ptr, layout) };
        note(-layout.size().cast_signed());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's contract, passed on.
        let moved = unsafe { System.realloc(ptr, layout, new_size) };
        if !moved.is_null() {
            // Both blocks are held while the bytes move.
            note(new_size.cast_signed());
            note(-layout.size().cast_signed());
        }
        moved
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// What `work` returns, with the most bytes this thread held at once while
/// it ran, beyond what it held before.
fn held_while<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = LIVE.with(Cell::get);
    PEAK.with(|peak| peak.set(before));
    let out = work();
    let peak = PEAK.with(Cell::get);
    (out, usize::try_from(peak - before).unwrap_or(0))
}

/// Judges in small windows and hides each `SECRET` as `<hidden>`, as a
/// placeholder no pattern knows.
struct Hiding;

const WINDOW: usize = 64 * 1024;

impl ContentScan for Hiding {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.replace("SECRET", "<hidden>"), Vec::new()))
    }
    fn limits(&self) -> SourceLimits {
        SourceLimits {
            whole_source: WINDOW,
            window: WINDOW,
            key_scan: WINDOW as u64,
        }
    }
}

/// One line of at least `len` bytes of `unit` repeated, with `SECRET`
/// between two units every `every` bytes (none when 0), then `end`.
fn line(len: usize, unit: &[u8], every: usize, end: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(len + unit.len() + end.len() + 6);
    let mut next = every;
    while line.len() < len {
        if every > 0 && line.len() >= next {
            line.extend_from_slice(b"SECRET");
            next += every;
        }
        line.extend_from_slice(unit);
    }
    line.extend_from_slice(end);
    line
}

fn source(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    file.write_all(bytes).expect("write");
    file.flush().expect("flush");
    file
}

/// What a match of `file` runs over: its bytes with each changed line as a
/// read serves it, read through a small buffer.
fn served(file: &tempfile::NamedTempFile, cancel: &impl CancellationCheck) -> Vec<u8> {
    let handle = file.as_file();
    let meta = handle.metadata().expect("meta");
    let (changed, _) = changed_lines(file.path(), handle, &meta, &Hiding, cancel)
        .expect("judged")
        .expect("a text source");
    (&mut &*handle).rewind().expect("rewind");
    let mut patched = Patched::new(std::io::BufReader::new(handle), &changed);
    let mut out = Vec::new();
    let mut piece = vec![0u8; 16 * 1024];
    loop {
        let read = patched.read(&mut piece).expect("read");
        if read == 0 {
            return out;
        }
        out.extend_from_slice(&piece[..read]);
    }
}

/// A line many windows long is served with each secret hidden and every
/// other byte once, its break kept, whatever charset the line decodes as;
/// the short lines around it are served as they are.
#[test]
fn a_long_line_is_served_as_a_read_judges_it_in_every_charset() {
    let len = 40 * WINDOW + 1234;
    // UTF-8 with multibyte chars across every read and window edge.
    let utf8 = line(len, "aé€😀 ".as_bytes(), 9_973, b"\r\n");
    // 8-bit text: no valid multibyte sequence.
    let latin1 = line(len, b"caf\xe9 na\xefve ", 7_919, b"\n");
    // Mostly UTF-8 with stray bytes: each invalid run shows as U+FFFD.
    let lossy = line(
        len,
        b"\xc3\xa9\xe2\x82\xac \xf0\x9f\x98\x80\xff ",
        8_191,
        b"",
    );
    let lossy = {
        let mut lossy = lossy;
        for at in (5_000..lossy.len()).step_by(70_001) {
            lossy[at] = 0xff;
        }
        lossy
    };
    for (name, raw) in [("utf8", utf8), ("latin1", latin1), ("lossy", lossy)] {
        let mut bytes = b"short SECRET line\n".to_vec();
        bytes.extend_from_slice(&raw);
        if raw.ends_with(b"\n") {
            bytes.extend_from_slice(b"after SECRET\n");
        }
        let file = source(&bytes);
        let decoded = octocode_engine::text::Decoded::interior(&raw).text;
        let text = decoded.trim_end_matches(['\n', '\r']);
        let mut want = b"short <hidden> line\n".to_vec();
        want.extend_from_slice(text.replace("SECRET", "<hidden>").as_bytes());
        if raw.ends_with(b"\r\n") {
            want.extend_from_slice(b"\r\n");
        } else if raw.ends_with(b"\n") {
            want.extend_from_slice(b"\n");
        }
        if raw.ends_with(b"\n") {
            want.extend_from_slice(b"after <hidden>\n");
        }
        let got = served(&file, &NeverCancel);
        assert!(
            got == want,
            "{name}: served {} bytes, a read serves {}",
            got.len(),
            want.len()
        );
    }
}

/// Judging a line, and serving it, holds a few windows whatever the line's
/// length: a line four times as long holds no more, and neither holds a
/// fraction of the shorter line.
#[test]
fn judging_and_serving_a_long_line_hold_windows_not_the_line() {
    let peaks = [64 * WINDOW, 256 * WINDOW].map(|len| {
        let file = source(&line(len, b"lorem ipsum dolor ", 50_021, b"\n"));
        let handle = file.as_file();
        let meta = handle.metadata().expect("meta");
        let (judged, judging) =
            held_while(|| changed_lines(file.path(), handle, &meta, &Hiding, &NeverCancel));
        let (changed, _) = judged.expect("judged").expect("a text source");
        assert!(!changed.is_empty(), "the line holds secrets");
        (&mut &*handle).rewind().expect("rewind");
        let (_, serving) = held_while(|| {
            let mut patched = Patched::new(std::io::BufReader::new(handle), &changed);
            let mut piece = vec![0u8; 16 * 1024];
            while patched.read(&mut piece).expect("read") > 0 {}
        });
        (len, judging, serving)
    });
    let [
        (short, judged_short, served_short),
        (_, judged_long, served_long),
    ] = peaks;
    for held in [judged_short, served_short, judged_long, served_long] {
        assert!(
            held < short / 4,
            "held {held} bytes at once for lines of {short} and more bytes: {peaks:?}"
        );
    }
    assert!(
        judged_long <= judged_short + WINDOW && served_long <= served_short + WINDOW,
        "a longer line holds more: {peaks:?}"
    );
}

/// Stops once the source's read position passes `past` (after the source
/// was read to its end `after_end` times), and keeps where it was then.
struct StopPast<'f> {
    file: &'f std::fs::File,
    len: u64,
    past: u64,
    after_end: u64,
    ends: AtomicU64,
    stopped_at: AtomicU64,
}

impl<'f> StopPast<'f> {
    fn new(file: &'f std::fs::File, past: u64, after_end: u64) -> Self {
        Self {
            file,
            len: file.metadata().expect("meta").len(),
            past,
            after_end,
            ends: AtomicU64::new(0),
            stopped_at: AtomicU64::new(u64::MAX),
        }
    }

    fn position(&self) -> u64 {
        (&mut &*self.file).stream_position().expect("position")
    }
}

impl CancellationCheck for StopPast<'_> {
    fn check(&self) -> Result<(), String> {
        if self.stopped_at.load(Ordering::Relaxed) != u64::MAX {
            return Err("stopped".to_owned());
        }
        let at = self.position();
        if at >= self.len {
            self.ends.fetch_add(1, Ordering::Relaxed);
        } else if self.ends.load(Ordering::Relaxed) >= self.after_end && at >= self.past {
            self.stopped_at.store(at, Ordering::Relaxed);
            return Err("stopped".to_owned());
        }
        Ok(())
    }
}

/// A cancel while the source is planned, or while its long line is judged,
/// ends the judgment within one read of the source: it does not read the
/// line to its end first.
#[test]
fn a_cancel_mid_line_ends_the_judgment_within_one_read() {
    let len = 256 * WINDOW;
    let file = source(&line(len, b"lorem ipsum dolor ", 50_021, b"\n"));
    let past = (len / 8) as u64;
    // Within one read of the source after the stop, never the line's end.
    let bound = past + (len / 16) as u64;
    for (phase, after_end) in [("planning", 0), ("judging", 1)] {
        let handle = file.as_file();
        let meta = handle.metadata().expect("meta");
        let cancel = StopPast::new(handle, past, after_end);
        let judged = changed_lines(file.path(), handle, &meta, &Hiding, &cancel);
        let ended_at = cancel.position();
        assert!(judged.is_err(), "{phase}: the cancelled judgment answered");
        assert!(
            ended_at < bound,
            "{phase}: read to byte {ended_at} of {len} after a stop at byte {past}"
        );
    }
}
