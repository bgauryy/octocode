use super::*;
use std::path::PathBuf;

#[test]
fn content_modified_detected_by_error_code() {
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-32801,\"message\":\"content modified\"}".to_owned(),
    );
    assert!(is_content_modified_error(&error));
}

#[test]
fn content_modified_detected_with_spaced_code() {
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\" : -32801 , \"message\":\"x\"}".to_owned(),
    );
    assert!(is_content_modified_error(&error));
}

#[test]
fn content_modified_not_triggered_by_phrase_in_payload() {
    // A hover/result payload that merely mentions the phrase, with a
    // different (or no) error code, must NOT be treated as ContentModified.
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-32603,\"message\":\"docs say: content modified by user\"}"
            .to_owned(),
    );
    assert!(!is_content_modified_error(&error));
}

#[test]
fn content_modified_not_triggered_by_substring_in_other_code() {
    // The digits -32801 appearing inside a larger number must not match.
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-328011,\"message\":\"x\"}".to_owned(),
    );
    assert!(!is_content_modified_error(&error));
}

#[test]
fn stderr_ring_keeps_only_recent_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    for index in 0..(STDERR_RING_CAPACITY + 5) {
        push_stderr_line(&lines, format!("line-{index}"));
    }

    let lines = lines.lock().expect("stderr ring lock");
    assert_eq!(lines.len(), STDERR_RING_CAPACITY);
    assert_eq!(lines.front().map(String::as_str), Some("line-5"));
    assert_eq!(
        lines.back().map(String::as_str),
        Some(format!("line-{}", STDERR_RING_CAPACITY + 4).as_str())
    );
}

#[test]
fn stderr_ring_truncates_very_long_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    push_stderr_line(&lines, "x".repeat(STDERR_LINE_MAX_CHARS + 10));

    let line = lines
        .lock()
        .expect("stderr ring lock")
        .front()
        .cloned()
        .expect("stderr line");
    assert_eq!(line.chars().count(), STDERR_LINE_MAX_CHARS + 3);
    assert!(line.ends_with("..."));
}

#[test]
fn snippet_content_cache_reuses_file_content_for_later_ranges() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let file_path = temp_file("octocode-engine-snippet-cache");
        std::fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write fixture");
        let file_path = file_path.to_string_lossy().into_owned();
        let mut cache = SnippetContentCache::default();

        let first = cache
            .read_range_content(&file_path, &range(0, 0))
            .await
            .expect("first range");
        std::fs::remove_file(&file_path).expect("remove fixture");
        let second = cache
            .read_range_content(&file_path, &range(1, 2))
            .await
            .expect("second range");

        assert_eq!(first, "alpha");
        assert_eq!(second, "beta\ngamma");
        assert_eq!(cache.files.len(), 1);
    });
}

/// Build a range covering whole lines `start_line..=end_line` *inclusive*.
/// LSP ranges are end-exclusive, so the end is the start of the line after
/// `end_line`.
fn range(start_line: u32, end_line: u32) -> JsRange {
    JsRange {
        start: JsExactPosition {
            line: start_line,
            character: 0,
        },
        end: JsExactPosition {
            line: end_line + 1,
            character: 0,
        },
    }
}

#[test]
fn extract_position_encoding_reads_server_choice() {
    // Server echoes the negotiated encoding.
    let result = json!({ "capabilities": { "positionEncoding": "utf-16" } });
    assert_eq!(
        extract_position_encoding(&result).as_deref(),
        Some("utf-16")
    );

    // A non-conformant server that ignored our utf-16-only advertisement.
    let result = json!({ "capabilities": { "positionEncoding": "utf-8" } });
    assert_eq!(extract_position_encoding(&result).as_deref(), Some("utf-8"));

    // Omitted ⇒ None (spec default is utf-16).
    let result = json!({ "capabilities": {} });
    assert_eq!(extract_position_encoding(&result), None);

    // No capabilities at all.
    assert_eq!(extract_position_encoding(&json!({})), None);
}

#[test]
fn parse_position_requires_numeric_line_and_character() {
    assert!(parse_position(&json!({"line": 3, "character": 7})).is_ok());
    assert!(parse_position(&json!({"line": 3})).is_err());
    assert!(parse_position(&json!({"character": 7})).is_err());
    assert!(parse_position(&json!({"line": "x", "character": 1})).is_err());
}

#[test]
fn slice_range_excludes_end_line_when_end_character_is_zero() {
    // LSP end-exclusive: {start:{0,0}, end:{2,0}} covers lines 0–1 only.
    let content = "line0\nline1\nline2\nline3\n";
    let r = JsRange {
        start: JsExactPosition {
            line: 0,
            character: 0,
        },
        end: JsExactPosition {
            line: 2,
            character: 0,
        },
    };
    assert_eq!(slice_range_content(content, &r), "line0\nline1");
}

#[test]
fn slice_range_includes_end_line_when_end_character_positive() {
    // A single-line range keeps the whole line (snippet context), not just
    // the [start.character, end.character) span.
    let content = "alpha\nbeta\n";
    let r = JsRange {
        start: JsExactPosition {
            line: 1,
            character: 2,
        },
        end: JsExactPosition {
            line: 1,
            character: 4,
        },
    };
    assert_eq!(slice_range_content(content, &r), "beta");
}

fn temp_file(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
