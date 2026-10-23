//! Size guards: UTF-16 decoding, the streaming ceiling, the in-memory
//! guard, and waiting on another row's preparation.

use super::*;
use crate::tools::source_view::prepare::{PREPARED_MIN_BYTES, PREPARING, prepared};

#[test]
fn utf16_fetch_reads_lines_and_keeps_raw_digest() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("source.txt");
    let paths = Paths(root.path().into());
    let source = format!("{}😀\r\nsecond\n", "a".repeat(5000));
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in source.encode_utf16() {
            bytes.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        fs::write(&file, &bytes).expect("write");
        let mut query = q(&file);
        query.ranges = vec!["2-2".parse().expect("range")];
        let result = fetch(&query, &paths);
        assert_ne!(result.status, ReadStatus::Error, "{result:?}");
        assert!(
            result
                .content
                .as_deref()
                .is_some_and(|text| text.contains("second"))
        );
        assert_eq!(
            result.source_sha256.as_deref(),
            Some(octocode_engine::digest::sha256(&bytes).as_str())
        );
        bytes.extend([0, 0]);
        fs::write(&file, &bytes).expect("binary");
        assert_eq!(
            fetch(&query, &paths).error_code,
            Some(ErrorCode::BinaryFileUnsupported)
        );
    }
}

#[test]
fn utf16_past_whole_file_limit_reports_decoding_limit_not_binary() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("large.txt");
    let mut head = vec![0xff, 0xfe];
    for unit in "a".repeat(BINARY_SAMPLE_BYTES).encode_utf16() {
        head.extend(unit.to_le_bytes());
    }
    fs::write(&path, head).expect("head");
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open")
        .set_len(MAX_SOURCE_BYTES + 2)
        .expect("length");
    let result = fetch(&q(&path), &Paths(root.path().into()));
    assert_eq!(result.error_code, Some(ErrorCode::FileTooLarge));
    assert_eq!(result.terminal_limit, Some(true));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("UTF-16"))
    );
    assert!(result.next.is_none());
}

/// A cancelled row stops waiting for another row's preparation of the
/// same large source instead of waiting until it lands.
#[test]
fn a_cancelled_row_stops_waiting_for_another_rows_preparation() {
    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    let bytes = vec![b'x'; PREPARED_MIN_BYTES];
    let path = std::path::Path::new("/flight/cancelled-row.txt");
    let sha256 = octocode_engine::digest::sha256(&bytes);
    let holder = PREPARING.join((path.to_path_buf(), sha256.clone(), false));
    let held = holder.blocking_turn();
    let started = std::time::Instant::now();
    let waited = prepared(&bytes, &sha256, false, path, &Passthrough, &Cancelled);
    assert_eq!(waited.err().as_deref(), Some("cancelled"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    drop(held);
    drop(holder);
}

#[test]
fn source_above_the_streaming_ceiling_returns_file_too_large() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("huge.txt");
    // Sparse file just past the streaming ceiling — the size guard fires
    // before any bytes are read, so this stays cheap.
    let file = fs::File::create(&path).expect("create file");
    let len = crate::tools::local_fetch::large_source::MAX_STREAM_SOURCE_BYTES + 1;
    file.set_len(len).expect("grow file");
    drop(file);

    let req = q(&path);
    let result = fetch(&req, &Paths(dir.clone()));

    assert_eq!(result.status, ReadStatus::Error);
    assert_eq!(result.error_code, Some(ErrorCode::FileTooLarge));
    assert_eq!(result.source_bytes, Some(len as usize));
    assert_eq!(result.terminal_limit, Some(true));
    assert!(
        result
            .error
            .as_deref()
            .expect("error")
            .contains("1073741824 bytes / 1 GiB")
    );
    assert!(!result.error.as_deref().expect("error").contains("10 MiB"));
    assert!(!result.hints.is_empty());
    assert!(!result.hints.join(" ").contains("remove matchString"));
    assert!(result.next.is_none());
}

#[test]
fn growing_in_memory_source_stops_at_guard_with_specific_recovery() {
    let source = vec![b'x'; (MAX_SOURCE_BYTES + 100) as usize];
    let Err(octocode_engine::text::ReadError::TooLarge(len)) = read_rest(
        std::io::Cursor::new(source),
        Vec::new(),
        MAX_SOURCE_BYTES,
        &crate::execution::cancel::NeverCancel,
    ) else {
        panic!("a source past the guard stops there");
    };
    assert_eq!(len, MAX_SOURCE_BYTES + 1);
    let result = source_too_large("growing.txt", len, SourceSizeLimit::InMemoryGrowth);
    assert_eq!(result.error_code, Some(ErrorCode::FileTooLarge));
    assert!(
        result
            .error
            .as_deref()
            .expect("error")
            .contains("in-memory read guard")
    );
    assert!(result.hints.join(" ").contains("bounded line window"));
    assert!(!result.hints.join(" ").contains("remove matchString"));
    assert_eq!(result.terminal_limit, Some(true));
}

#[test]
fn at_limit_plain_read_is_allowed() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("ok.txt");
    fs::write(&path, "hello\nworld\n").expect("write file");

    let req = q(&path);
    let result = fetch(&req, &Paths(dir.clone()));

    assert_eq!(result.status, ReadStatus::Success);
    assert_eq!(result.error_code, None);
}
