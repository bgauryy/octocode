//! Size guards: UTF-16 decoding, the streaming ceiling, the in-memory
//! guard, and waiting on another row's preparation.

use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::source_view::prepare::prepared;

#[test]
fn source_preparation_is_owned_by_each_request() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CountPreparations(AtomicUsize);
    impl octocode_runtime_base::security::scan::ContentScan for CountPreparations {
        fn sanitize(
            &self,
            text: &str,
            _: &std::path::Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok((text.to_owned(), Vec::new()))
        }
        fn redact_key_blocks<'a>(&self, text: &'a str) -> (std::borrow::Cow<'a, str>, bool) {
            self.0.fetch_add(1, Ordering::SeqCst);
            (std::borrow::Cow::Borrowed(text), false)
        }
    }
    let bytes = vec![b'x'; MAX_SOURCE_BYTES as usize / 2];
    let scanner = CountPreparations(AtomicUsize::new(0));
    for _ in 0..2 {
        prepared(&bytes, false, &scanner, &NeverCancel).expect("prepare source");
    }
    assert_eq!(scanner.0.load(Ordering::SeqCst), 2);
}

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
        local_fetch_query::set_line_span(&mut query, 2, 2).expect("range");
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

/// Cancellation is checked before decoding or scanning the source.
#[test]
fn a_cancelled_row_does_not_prepare_source() {
    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    let bytes = vec![b'x'; MAX_SOURCE_BYTES as usize / 2];
    let result = prepared(&bytes, false, &Passthrough, &Cancelled);
    assert_eq!(result.err().as_deref(), Some("cancelled"));
}

#[test]
fn source_above_the_streaming_ceiling_returns_file_too_large() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let path = dir.join("huge.txt");
    // Sparse file just past the streaming ceiling — the size guard fires
    // before any bytes are read, so this stays cheap.
    let file = fs::File::create(&path).expect("create file");
    let len = octocode_tools_shared::large_source::MAX_STREAM_SOURCE_BYTES + 1;
    file.set_len(len).expect("grow file");
    drop(file);

    let req = q(&path);
    let result = fetch(&req, &Paths(dir.clone()));

    assert_eq!(result.status, ReadStatus::Error);
    assert_eq!(result.error_code, Some(ErrorCode::FileTooLarge));
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
        &octocode_runtime_base::execution::cancel::NeverCancel,
    ) else {
        panic!("a source past the guard stops there");
    };
    assert_eq!(len, MAX_SOURCE_BYTES + 1);
    let result = source_too_large(
        "growing.txt",
        len,
        SourceSizeLimit::InMemoryGrowth(MAX_SOURCE_BYTES),
    );
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

/// A manifest's value, fake.
const MANIFEST_VALUE: &str = "ghp_c2VjcmV0LXZhbHVlLW9mLXRoZS1maXh0dXJl";

/// A JSON object holding a token, its data lines from its kind, in `pad`
/// lines of `line` and blanks up to `size` bytes when `size` is set.
fn manifest(names_kind: bool, line: &[u8], pad: usize, size: Option<usize>) -> Vec<u8> {
    let mut text = format!("{{\n\"data\":{{\n\"k\":\"{MANIFEST_VALUE}\"\n}},\n").into_bytes();
    let tail: &[u8] = if names_kind {
        b"\"kind\":\"Secret\"\n}\n"
    } else {
        b"\"kind\":\"Other\"\n}\n"
    };
    let lines = size.map_or(pad, |size| (size - text.len() - tail.len()) / line.len());
    for _ in 0..lines {
        text.extend_from_slice(line);
    }
    if let Some(size) = size {
        text.resize(size - tail.len(), b' ');
    }
    text.extend_from_slice(tail);
    text
}

/// The reads a source is asked for: whole, by a range of its value's line, and
/// by a match on that line.
fn reads(path: &std::path::Path, dir: &std::path::Path) -> Vec<LocalFetchResult> {
    [
        q(path),
        qj(
            path,
            json_fixture(r#"{"ranges": [{"startLine": 1, "endLine": 5}]}"#),
        ),
        qj(path, json_fixture(r#"{"matchString": ["\"k\""]}"#)),
    ]
    .iter()
    .map(|query| {
        execute_local_fetch(
            query,
            &Paths(dir.to_path_buf()),
            &octocode_runtime_base::security::ContentSecurity::new(),
            &NeverCancel,
            None,
            None,
        )
    })
    .collect()
}

/// Every source read whole is judged whole: the read and the scanner stop at
/// one size, so no source reads line by line for lying between two of them.
#[test]
fn a_secret_is_hidden_up_to_the_last_byte_a_source_reads_whole() {
    let temp = Fixture::new();
    let path = temp.0.join("dump.json");
    for size in [MAX_SOURCE_BYTES as usize - 1, MAX_SOURCE_BYTES as usize] {
        let text = manifest(true, b"ordinary words\n", 0, Some(size));
        assert_eq!(text.len(), size);
        fs::write(&path, &text).expect("write");
        for row in reads(&path, &temp.0) {
            assert_ne!(row.status, ReadStatus::Error, "{size}: {:?}", row.error);
            assert!(
                !serde_json::to_string(&row)
                    .expect("row")
                    .contains(MANIFEST_VALUE),
                "{size}"
            );
        }
    }
}

/// Decoding can grow a source past what a read holds at once, after it was read
/// whole. It is judged as any other: the token is hidden, and every way of
/// reading it shows the rest.
#[test]
fn a_source_grown_past_the_bound_is_judged_as_any_other() {
    let temp = Fixture::new();
    let path = temp.0.join("dump.json");
    let mut latin1 = vec![b'\xe9'; 79];
    latin1.push(b'\n');
    let pad = (MAX_SOURCE_BYTES as usize - 1024) / latin1.len();
    let text = manifest(true, &latin1, pad, None);
    assert!(text.len() as u64 <= MAX_SOURCE_BYTES);
    fs::write(&path, &text).expect("write");
    for row in reads(&path, &temp.0) {
        assert_ne!(row.status, ReadStatus::Error, "{:?}", row.error);
        assert!(
            !serde_json::to_string(&row)
                .expect("row")
                .contains(MANIFEST_VALUE)
        );
    }
    // The same size naming no object reads the same.
    fs::write(&path, manifest(false, &latin1, pad, None)).expect("write");
    for row in reads(&path, &temp.0) {
        assert_eq!(row.status, ReadStatus::Success, "{:?}", row.error);
    }
}
