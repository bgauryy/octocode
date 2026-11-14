//! A later page reads only its window, and is the same page a full pass gives.
use super::*;

const LINE: usize = 80;
const LINES: usize = 40_000;

/// Numbered lines, distinct per `seed` (the store keeps each file's pass under
/// its own path, so equal bytes would not collide either).
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
fn a_fresh_range_of_a_settled_file_reads_only_its_window() {
    let file = temp(&lines(b'r'));
    octocode_engine::stamp::settle_written_so_far();
    let (first, _) = pages(&file);
    let q = query(serde_json::json!({"path":"f","ranges":[{"startLine":30000,"endLine":30000}]}));
    let before = source_bytes_read();
    let kept = fetch(&file, &q, Some(2_000));
    let read = source_bytes_read() - before;
    assert_eq!(kept.status, ReadStatus::Success, "{kept:?}");
    assert_eq!(kept.source_sha256, first.source_sha256);
    let bound =
        super::super::proven::CHECKPOINT_BYTES as usize + super::super::proven::READ_BYTES + LINE;
    assert!(
        read <= bound,
        "fresh range read {read} bytes; window bound {bound}"
    );
    let whole = fetch(&temp(&lines(b'r')), &q, Some(2_000));
    assert_eq!(kept.content, whole.content);
    assert_eq!(kept.total_lines, whole.total_lines);
    assert_eq!(kept.source_sha256, whole.source_sha256);
}

#[test]
fn a_kept_pass_is_not_reused_by_a_different_scanner_policy() {
    let file = temp(&lines(b'p'));
    octocode_engine::stamp::settle_written_so_far();
    let (_, next) = pages(&file);
    let before = source_bytes_read();
    let page = fetch_window(
        &next,
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        Some(2_000),
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    );
    assert_eq!(page.status, ReadStatus::Success, "{page:?}");
    assert!(
        source_bytes_read() - before >= LINE * LINES,
        "different limits require a new whole-source proof"
    );
}

#[test]
fn a_custom_scanner_does_not_reuse_a_real_scanners_pass() {
    let file = temp(&lines(b's'));
    octocode_engine::stamp::settle_written_so_far();
    let (_, next) = pages(&file);
    let before = source_bytes_read();
    let page = fetch_window(
        &next,
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        Some(2_000),
        &crate::security::scan::Passthrough,
        &NeverCancel,
    );
    assert_eq!(page.status, ReadStatus::Success, "{page:?}");
    assert!(
        source_bytes_read() - before >= LINE * LINES,
        "a custom scanner must prove the whole source itself"
    );
}

#[test]
fn a_real_scanner_does_not_reuse_a_custom_scanners_pass() {
    let file = temp(&lines(b'w'));
    octocode_engine::stamp::settle_written_so_far();
    let first = fetch_window(
        &query(serde_json::json!({"path":"f"})),
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        Some(2_000),
        &crate::security::scan::Passthrough,
        &NeverCancel,
    );
    let next = &first
        .next
        .as_ref()
        .expect("next")
        .continue_window
        .as_ref()
        .expect("window")
        .query;
    let before = source_bytes_read();
    let page = fetch(&file, next, Some(2_000));
    assert_eq!(page.status, ReadStatus::Success, "{page:?}");
    assert!(
        source_bytes_read() - before >= LINE * LINES,
        "the real scanner must prove a custom scanner's source itself"
    );
}

/// Two files with the same bytes at different paths keep their passes side by
/// side: reading the second does not evict the first's, so a later page of
/// the first still reads only its window. (The store was keyed by digest
/// alone, and each file's pass replaced the other's.)
#[test]
fn identical_files_at_two_paths_keep_their_passes_apart() {
    let one = temp(&lines(b'i'));
    let two = temp(&lines(b'i'));
    octocode_engine::stamp::settle_written_so_far();
    let (first, next) = pages(&one);
    let (other, _) = pages(&two);
    assert_eq!(first.source_sha256, other.source_sha256, "the same bytes");
    let before = source_bytes_read();
    let page = fetch(&one, &next, Some(2_000));
    let read = source_bytes_read() - before;
    assert_eq!(page.status, ReadStatus::Success, "{page:?}");
    assert!(
        read < 2 * 1024 * 1024,
        "reads about one checkpoint and window of {} bytes, read {read}",
        LINE * LINES
    );
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

#[cfg(unix)]
#[test]
fn a_fresh_range_reproves_same_size_edits_and_replacements() {
    let file = temp(&lines(b't'));
    octocode_engine::stamp::settle_written_so_far();
    let (first, _) = pages(&file);
    let q = query(serde_json::json!({"path":"f","ranges":[{"startLine":30000,"endLine":30000}]}));
    write_restoring_time(file.path());
    let before = source_bytes_read();
    let edited = fetch(&file, &q, Some(2_000));
    assert_eq!(edited.status, ReadStatus::Success, "{edited:?}");
    assert_ne!(edited.source_sha256, first.source_sha256);
    assert!(source_bytes_read() - before >= LINE * LINES);
    octocode_engine::stamp::settle_written_so_far();
    pages(&file);
    let replacement = temp(&lines(b'u'));
    replacement
        .as_file()
        .set_modified(
            std::fs::metadata(file.path())
                .expect("meta")
                .modified()
                .expect("modified"),
        )
        .expect("restore replacement time");
    std::fs::rename(replacement.path(), file.path()).expect("replace");
    let before = source_bytes_read();
    let replaced = fetch(&file, &q, Some(2_000));
    assert_eq!(replaced.status, ReadStatus::Success, "{replaced:?}");
    assert_ne!(replaced.source_sha256, edited.source_sha256);
    assert!(source_bytes_read() - before >= LINE * LINES);
}

#[test]
fn a_fresh_range_reproves_key_changes_outside_its_window() {
    let begin = ["-----BEGIN ", "RSA PRIVATE KEY-----\n"].concat();
    let end = ["-----END ", "RSA PRIVATE KEY-----\n"].concat();
    let mut text = vec![b'x'; begin.len()];
    *text.last_mut().expect("newline") = b'\n';
    text.extend(lines(b'v'));
    text.extend_from_slice(end.as_bytes());
    let file = temp(&text);
    octocode_engine::stamp::settle_written_so_far();
    pages(&file);
    text[..begin.len()].copy_from_slice(begin.as_bytes());
    std::fs::write(file.path(), &text).expect("new key boundary");
    let q = query(serde_json::json!({"path":"f","ranges":[{"startLine":30000,"endLine":30000}]}));
    let before = source_bytes_read();
    let page = fetch(&file, &q, Some(2_000));
    assert_eq!(page.status, ReadStatus::Success, "{page:?}");
    assert!(source_bytes_read() - before >= text.len());
    let whole = fetch(&temp(&text), &q, Some(2_000));
    assert_eq!(page.content, whole.content);
    assert_eq!(page.source_sha256, whole.source_sha256);
    assert!(
        !page
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("0000000000000000029998")
    );
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
    let pass = super::super::stream::stream_pass(
        &handle,
        file.path(),
        &[],
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("pass");
    let writer = WriterDuringRead::on(file.path());
    let windows = pass.proven.windows(
        &handle,
        &[Want::lines(LINES - 10, LINES)],
        pass.total_lines,
        &writer,
    );
    assert!(writer.wrote(), "the write ran during the read");
    let error = windows
        .err()
        .expect("the changed source is refused")
        .to_string();
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
    let pass = super::super::stream::stream_pass(
        &handle,
        file.path(),
        &[],
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
    .expect("pass");
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
