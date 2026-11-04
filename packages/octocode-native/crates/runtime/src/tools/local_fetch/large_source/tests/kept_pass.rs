//! A later page reads only its window, and is the same page a full pass gives.
use super::*;

const LINE: usize = 80;
const LINES: usize = 40_000;

/// Numbered lines, distinct per `seed`: the store is keyed by digest and shared
/// by tests running at once.
fn lines(seed: u8) -> Vec<u8> {
    let mut source: Vec<u8> = (0..LINES)
        .flat_map(|index| format!("{index:0width$}\n", width = LINE - 1).into_bytes())
        .collect();
    source[0] = seed;
    source
}

/// The first page of a read, and the page its continuation names.
fn pages(file: &tempfile::NamedTempFile) -> (LocalFetchResult, LocalFetchQuery) {
    let first = fetch(file, &query(serde_json::json!({"path":"f"})), Some(2_000));
    let next = first
        .next
        .as_ref()
        .and_then(|next| next.continue_window.as_ref())
        .expect("a later page")
        .query
        .clone();
    (first, next)
}

#[test]
fn a_later_page_of_a_settled_file_reads_only_its_window() {
    let file = temp(&lines(b'a'));
    octocode_engine::stamp::settle_written_so_far();
    let (_, next) = pages(&file);
    let before = source_bytes_read();
    let kept = fetch(&file, &next, Some(2_000));
    let read = source_bytes_read() - before;
    assert!(
        read < 2 * 1024 * 1024,
        "reads about one checkpoint and window of {} bytes, read {read}",
        LINE * LINES
    );
    let whole = {
        let unsettled = temp(&lines(b'a'));
        let (_, next) = pages(&unsettled);
        fetch(&unsettled, &next, Some(2_000))
    };
    assert_eq!(kept.content, whole.content);
    assert_eq!(kept.source_sha256, whole.source_sha256);
    assert_eq!(kept.total_lines, whole.total_lines);
    assert_eq!(kept.start_line, whole.start_line);
    assert_eq!(kept.end_line, whole.end_line);
}

#[test]
fn a_file_changed_after_its_first_page_is_stale_not_served() {
    let file = temp(&lines(b'b'));
    octocode_engine::stamp::settle_written_so_far();
    let (_, next) = pages(&file);
    let changed = lines(b'x');
    std::fs::write(file.path(), &changed).expect("rewrite");
    let page = fetch(&file, &next, Some(2_000));
    assert_eq!(
        page.error_code,
        Some(ErrorCode::StaleSnapshot),
        "a changed file restarts the read"
    );
}

#[test]
fn a_file_written_just_now_is_proved_again_on_every_page() {
    let file = temp(&lines(b'c'));
    let (_, next) = pages(&file);
    let before = source_bytes_read();
    fetch(&file, &next, Some(2_000));
    assert!(
        source_bytes_read() - before >= LINE * LINES,
        "no stamp stands in for a file written within the settle window"
    );
}

/// Rewrites `path` in place, same size, and restores its modification time: a
/// write only the full stamp can see.
#[cfg(unix)]
fn write_restoring_time(path: &Path) {
    let before = std::fs::metadata(path).expect("meta");
    let modified = before.modified().expect("modified");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("reopen");
    std::io::Write::write_all(&mut file, b"Z").expect("write");
    file.set_modified(modified).expect("restore time");
    let after = std::fs::metadata(path).expect("meta");
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().expect("modified"), modified);
}

/// A writer that runs while the read holds the source open.
#[cfg(unix)]
struct WriterDuringRead<'a> {
    path: &'a Path,
    written: std::sync::atomic::AtomicBool,
}

#[cfg(unix)]
impl<'a> WriterDuringRead<'a> {
    fn on(path: &'a Path) -> Self {
        Self {
            path,
            written: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn wrote(&self) -> bool {
        self.written.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(unix)]
impl CancellationCheck for WriterDuringRead<'_> {
    fn check(&self) -> Result<(), String> {
        if !self.written.swap(true, std::sync::atomic::Ordering::SeqCst) {
            write_restoring_time(self.path);
        }
        Ok(())
    }
}

/// The windows of a proved source hold what its digest proves: a write during
/// their read refuses them, even with the size and modification time restored.
#[cfg(unix)]
#[test]
fn a_write_restoring_its_time_during_a_window_read_refuses_the_windows() {
    let file = temp(&lines(b'd'));
    let handle = opened(file.path());
    let pass = super::super::stream::stream_pass(&handle, &[], &NeverCancel)
        .expect("pass")
        .pass;
    let writer = WriterDuringRead::on(file.path());
    let windows = pass.proven.windows(
        &handle,
        &[Want::lines(LINES - 10, LINES)],
        pass.total_lines,
        &writer,
    );
    assert!(writer.wrote(), "the write ran during the read");
    let error = windows.err().expect("the changed source is refused");
    assert!(error.contains("changed while it was read"), "{error}");
}

/// A match page's windows come from its scan's proof, and hold to it the same way.
#[cfg(unix)]
#[test]
fn a_write_restoring_its_time_during_a_match_page_refuses_the_page() {
    let file = temp(&lines(b'e'));
    let q = query(serde_json::json!({"path":"f","matchString":["0000000000000000012345"]}));
    let writer = WriterDuringRead::on(file.path());
    let page = fetch_window(
        &q,
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        None,
        Some(2_000),
        &crate::security::ContentSecurity::new(),
        &writer,
    );
    assert!(writer.wrote(), "the write ran during the read");
    assert_eq!(page.error_code, Some(ErrorCode::FileReadFailed), "{page:?}");
}

/// A pass the source changed under is not kept: its stamp no longer stands in
/// for the bytes it hashed, whatever the write restored.
#[cfg(unix)]
#[test]
fn a_write_restoring_its_time_after_a_pass_leaves_it_not_unchanged() {
    let file = temp(&lines(b'f'));
    let handle = opened(file.path());
    let meta = std::fs::metadata(file.path()).expect("meta");
    let pass = super::super::stream::stream_pass(&handle, &[], &NeverCancel)
        .expect("pass")
        .pass;
    assert!(pass.proven.unchanged(&handle, &meta));
    write_restoring_time(file.path());
    assert!(!pass.proven.unchanged(&handle, &meta));
}

/// Secret blocks stay replaced when a window starts from a kept checkpoint.
#[test]
fn a_kept_pass_keeps_the_key_block_state_of_its_checkpoints() {
    let mut source = b"-----BEGIN RSA PRIVATE KEY-----\n".to_vec();
    for index in 0..30_000 {
        source.extend_from_slice(format!("SECRETLINE{index:069}\n").as_bytes());
    }
    source.extend_from_slice(b"-----END RSA PRIVATE KEY-----\nafter\n");
    let file = temp(&source);
    octocode_engine::stamp::settle_written_so_far();
    let (_, next) = pages(&file);
    let page = fetch(&file, &next, Some(2_000));
    assert!(
        !page
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("SECRETLINE"),
        "{:?}",
        page.content
    );
}

/// Cancellation checks run during a pass on its own thread: this one moves that
/// thread's settle clock on, as a pass lasting past the settle window does.
struct ClockMovedDuringPass;

impl CancellationCheck for ClockMovedDuringPass {
    fn check(&self) -> Result<(), String> {
        octocode_engine::stamp::settle_written_so_far();
        Ok(())
    }
}

/// Settling is judged by the clock from before the pass: a write in the pass
/// can leave the stamp its source had before, so a file not yet settled when
/// the pass began is not kept, however long the pass took.
#[test]
fn a_file_settling_only_during_its_pass_is_not_kept() {
    let file = temp(&lines(b'g'));
    let first = fetch_window(
        &query(serde_json::json!({"path":"f"})),
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        None,
        Some(2_000),
        &crate::security::ContentSecurity::new(),
        &ClockMovedDuringPass,
    );
    let next = first
        .next
        .as_ref()
        .and_then(|next| next.continue_window.as_ref())
        .expect("a later page")
        .query
        .clone();
    let before = source_bytes_read();
    fetch(&file, &next, Some(2_000));
    assert!(
        source_bytes_read() - before >= LINE * LINES,
        "the later page proves the bytes again"
    );
}
