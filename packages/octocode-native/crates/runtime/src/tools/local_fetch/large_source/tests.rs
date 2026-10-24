use super::held::Want;
use super::index::{VIEW_POINT_BYTES, read_windows};
use super::stream::stream;
use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_contract_types::LocalFetchQuery;
use sha2::{Digest, Sha256};

fn opened(path: &Path) -> std::fs::File {
    std::fs::File::open(path).expect("open")
}

fn temp(content: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("temp");
    std::io::Write::write_all(&mut file, content).expect("write");
    file
}

fn stream_one(path: &Path, first: usize, last: usize) -> Streamed {
    stream(&opened(path), &[Want::lines(first, last)], &NeverCancel)
        .expect("stream")
        .0
        .pop()
        .expect("window")
}

/// Counts the copy blocks a stream writes (one check each).
struct Blocks(std::sync::atomic::AtomicUsize);
impl CancellationCheck for Blocks {
    fn check(&self) -> Result<(), String> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

/// `query` read through [`fetch_window`] from `file`'s own handle, with the
/// row `share`.
fn fetch(
    file: &tempfile::NamedTempFile,
    query: &LocalFetchQuery,
    share: Option<usize>,
) -> LocalFetchResult {
    fetch_window(
        query,
        file.path(),
        &opened(file.path()),
        &std::fs::metadata(file.path()).expect("meta"),
        None,
        share,
        &crate::security::ContentSecurity::new(),
        &NeverCancel,
    )
}

/// A query from wire JSON.
fn query(json: serde_json::Value) -> LocalFetchQuery {
    serde_json::from_value(json).expect("query")
}

mod byte_pages;
mod reads;

/// One stream re-reads its handle from the start, wherever an earlier
/// read left it.
#[test]
fn a_stream_reads_its_handle_from_the_start() {
    let file = temp(b"a\nb\n");
    let source = opened(file.path());
    std::io::Read::read(&mut &source, &mut [0_u8; 3]).expect("sample");
    let (windows, ..) = stream(&source, &[Want::lines(1, 2)], &NeverCancel).expect("stream");
    assert_eq!(windows[0].window, b"a\nb\n");
}

/// One pass holds every window, each with the file's digest and totals,
/// and key-block lines replaced in each window they reach.
#[test]
fn one_stream_holds_every_requested_window() {
    let file =
        temp(b"a\n-----BEGIN RSA PRIVATE KEY-----\nSECRET\n-----END RSA PRIVATE KEY-----\nb\nc\n");
    let (windows, ..) = stream(
        &opened(file.path()),
        &[Want::lines(1, 1), Want::lines(3, 3), Want::lines(5, 6)],
        &NeverCancel,
    )
    .expect("stream");
    let held = windows
        .iter()
        .map(|window| String::from_utf8(window.window.clone()).expect("utf8"))
        .collect::<Vec<_>>();
    assert_eq!(held[0], "a\n");
    assert!(!held[1].contains("SECRET"), "{}", held[1]);
    assert_eq!(held[2], "b\nc\n");
    assert_eq!(windows[1].key_lines, 1);
    assert!(
        windows
            .iter()
            .all(|window| window.total_lines == 6 && window.digest == windows[0].digest)
    );
    assert_eq!(
        windows
            .iter()
            .map(|w| (w.first, w.last))
            .collect::<Vec<_>>(),
        vec![(1, 1), (3, 3), (5, 6)]
    );
}

#[test]
fn streams_hash_totals_and_window() {
    let file = temp(b"a\nb\nc\nd");
    let streamed = stream_one(file.path(), 2, 3);
    assert_eq!(streamed.total_lines, 4);
    assert_eq!(streamed.window, b"b\nc\n");
    assert_eq!((streamed.first, streamed.last), (2, 3));
    assert_eq!(streamed.digest, hex::encode(Sha256::digest(b"a\nb\nc\nd")));
}

/// A cancel ends the hashing copy with the cancel's own reason.
#[test]
fn a_cancelled_stream_stops_with_the_cancel_reason() {
    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("deadline reached".into())
        }
    }
    let file = temp(b"a\nb\n");
    assert_eq!(
        stream(&opened(file.path()), &[Want::lines(1, 2)], &Cancelled)
            .map(drop)
            .err()
            .as_deref(),
        Some("deadline reached")
    );
}

/// Lines split across copy blocks count once, and an unterminated last
/// line is a line: the totals agree with the whole-text line count.
#[test]
fn lines_across_copy_blocks_count_like_the_whole_text() {
    let mut text = "x".repeat(70 * 1024);
    text.push('\n');
    text.push_str("tail without newline");
    let file = temp(text.as_bytes());
    let streamed = stream_one(file.path(), 2, 2);
    assert_eq!(streamed.total_lines, 2);
    assert_eq!(streamed.window, b"tail without newline");
    assert_eq!(
        streamed.digest,
        hex::encode(Sha256::digest(text.as_bytes()))
    );
}

#[test]
fn key_block_lines_are_replaced_even_when_markers_are_outside_the_window() {
    let file = temp(
        b"x\n-----BEGIN RSA PRIVATE KEY-----\nSECRET1\nSECRET2\n-----END RSA PRIVATE KEY-----\ny\n",
    );
    let streamed = stream_one(file.path(), 3, 4);
    let window = String::from_utf8(streamed.window).expect("utf8");
    assert!(!window.contains("SECRET"), "{window}");
    assert_eq!(streamed.key_lines, 2);
    let after = stream_one(file.path(), 6, 6);
    assert_eq!(after.window, b"y\n");
}

#[test]
fn unterminated_key_redacts_its_body_run_and_leaves_later_code_readable() {
    let file = temp(
        b"let a = 1;\nconst PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet b = compute(a);\nfn main() {}\n-----BEGIN OPENSSH PRIVATE KEY-----\nSECRET1\nSECRET2\nfn tail() {}\n",
    );
    let streamed = stream_one(file.path(), 1, 8);
    let window = String::from_utf8(streamed.window).expect("utf8");
    assert!(
        !window.contains("SECRET") && !window.contains("BEGIN"),
        "{window}"
    );
    assert!(window.contains("let b = compute(a);"), "{window}");
    assert!(
        window.contains("fn main() {}") && window.contains("fn tail() {}"),
        "{window}"
    );
    assert_eq!(streamed.key_lines, 4);
}

/// A later page of an indexed source streams from the checkpoint before
/// its window, not the whole file, and holds the same window: the
/// digest and totals of the whole pass, and a private-key block open
/// across the checkpoint still redacted.
#[test]
fn an_indexed_source_streams_a_later_window_from_its_checkpoint() {
    let mut text = String::new();
    for line in 1..=40_000 {
        match line {
            4_090 => text.push_str("-----BEGIN RSA PRIVATE KEY-----\n"),
            4_091..=4_110 => text.push_str("SECRETSECRETSECRET\n"),
            4_111 => text.push_str("-----END RSA PRIVATE KEY-----\n"),
            _ => text.push_str(&format!("line {line}\n")),
        }
    }
    let file = temp(text.as_bytes());
    let source = opened(file.path());
    let meta = source.metadata().expect("meta");
    let stamp = crate::tools::source::cache_stamp(&meta);
    assert!(stamp.is_some(), "the platform stamps files");
    let path = file.path().canonicalize().expect("canonical");
    let spans = [Want::lines(4_100, 4_120), Want::lines(39_990, 40_000)];
    let whole = Blocks(Default::default());
    let first = read_windows(&source, &path, stamp, &spans, &whole).expect("first");
    let indexed = Blocks(Default::default());
    let again = read_windows(&source, &path, stamp, &spans, &indexed).expect("again");
    let full = whole.0.into_inner();
    let partial = indexed.0.into_inner();
    assert!(partial < full, "indexed {partial} blocks vs whole {full}");
    for (a, b) in first.iter().zip(&again) {
        assert_eq!(a.window, b.window);
        assert_eq!(
            (a.first, a.last, a.key_lines),
            (b.first, b.last, b.key_lines)
        );
        assert_eq!((&a.digest, a.total_lines), (&b.digest, b.total_lines));
    }
    let held = String::from_utf8(again[0].window.clone()).expect("utf8");
    assert!(!held.contains("SECRET"), "{held}");
    assert!(held.contains("line 4112\n"), "{held}");
    // The body lines in the window and the END marker.
    assert_eq!(again[0].key_lines, 12);
    // Another stamp is another version: the whole file streams again.
    let other = Blocks(Default::default());
    let moved = stamp.map(|stamp| octocode_engine::stamp::SourceStamp {
        size: stamp.size + 1,
        ..stamp
    });
    read_windows(&source, &path, moved, &spans, &other).expect("other");
    assert_eq!(other.0.into_inner(), full);
}

#[test]
fn a_giant_single_line_is_cut_to_a_window_not_buffered() {
    let big = vec![b'z'; WINDOW_BYTES + 10];
    let file = temp(&big);
    let streamed = stream_one(file.path(), 1, 1);
    assert_eq!(streamed.total_lines, 1);
    assert!(streamed.window.len() <= WINDOW_BYTES + 1);
    // The window is a part of the line's bytes: the view is all of them.
    assert_eq!(streamed.origin.base, 0);
    assert_eq!(streamed.origin.total, big.len());
}

#[test]
fn cutting_a_utf8_line_keeps_only_complete_source_characters() {
    let text = format!("€{}", "🙂".repeat(WINDOW_BYTES / 4 + 1));
    let file = temp(text.as_bytes());
    let streamed = stream_one(file.path(), 1, 1);
    let prefix = std::str::from_utf8(&streamed.window).expect("complete UTF-8");
    assert!(text.starts_with(prefix));
    assert_eq!(prefix.len(), WINDOW_BYTES - 1);
    assert_eq!(streamed.origin.total, text.len());
}
