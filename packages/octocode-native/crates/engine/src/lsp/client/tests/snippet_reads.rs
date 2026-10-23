//! Location snippets: range slicing, position parsing, and the read policy.

use super::*;
use crate::lsp::client::snippets::{
    CachedSource, MAX_SNIPPET_SOURCE_BYTES, SnippetContentCache, parse_position,
    read_bounded_regular_file, slice_range_content, snippets_from_locations,
};
use crate::lsp::types::{LspExactPosition, LspRange};
use crate::lsp::uri::path_to_uri;
use crate::text::LineIndex;
use serde_json::json;

#[test]
fn location_links_select_the_symbol_and_preserve_definition_context() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let file_path = tmp.0.join("octocode-engine-location-link");
        std::fs::write(&file_path, "// context\nfunction target() {\n  return 1;\n}\n").unwrap();
        let uri = path_to_uri(&file_path.to_string_lossy()).unwrap();
        let selection = json!({"start":{"line":1,"character":9},"end":{"line":1,"character":15}});
        let locations = snippets_from_locations(&json!([
            {"targetUri":uri,"targetRange":{"start":{"line":0,"character":0},"end":{"line":4,"character":0}},"targetSelectionRange":selection},
            {"uri":uri,"range":selection}
        ]), &SnippetReadPolicy::default()).await.unwrap();
        std::fs::remove_file(file_path).unwrap();
        assert_eq!(locations.len(), 2);
        for location in &locations {
            assert_eq!(location.range.start.line, 1);
            assert_eq!(location.range.start.character, 9);
            assert_eq!(location.range.end.character, 15);
        }
        assert_eq!(locations[0].content, "// context\nfunction target() {\n  return 1;\n}");
        assert_eq!(locations[1].content, "function target() {");
        assert_eq!(locations[0].display_range, Some(json!({"startLine":1,"endLine":4})));
        assert!(locations[1].display_range.is_none());
    });
}

#[test]
fn snippet_content_cache_reuses_file_content_for_later_ranges() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let tmp = TmpDir::new();
        let file_path = tmp.0.join("octocode-engine-snippet-cache");
        std::fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write fixture");
        let file_path = file_path.to_string_lossy().into_owned();
        let mut cache = SnippetContentCache::new(SnippetReadPolicy::default());

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
fn range(start_line: u32, end_line: u32) -> LspRange {
    LspRange {
        start: LspExactPosition {
            line: start_line,
            character: 0,
        },
        end: LspExactPosition {
            line: end_line + 1,
            character: 0,
        },
    }
}

#[test]
fn parse_position_requires_numeric_line_and_character() {
    let position = parse_position(&json!({"line": 3, "character": 7})).expect("position");
    assert_eq!((position.line, position.character), (3, 7));
    assert!(parse_position(&json!({"line": 3})).is_err());
    assert!(parse_position(&json!({"character": 7})).is_err());
    assert!(parse_position(&json!({"line": "x", "character": 1})).is_err());
}

#[test]
fn parse_position_rejects_values_beyond_u32_instead_of_wrapping() {
    let too_big = u64::from(u32::MAX) + 1;
    let error = parse_position(&json!({"line": too_big, "character": 0}))
        .expect_err("line overflow must not wrap to 0");
    assert!(error.reason.contains("line"), "{}", error.reason);
    assert!(parse_position(&json!({"line": 0, "character": too_big})).is_err());
    let max =
        parse_position(&json!({"line": u32::MAX, "character": u32::MAX})).expect("u32::MAX fits");
    assert_eq!((max.line, max.character), (u32::MAX, u32::MAX));
}

fn cached(content: &str) -> CachedSource {
    CachedSource {
        content: content.to_owned(),
        lines: LineIndex::new(content),
    }
}

#[test]
fn slice_range_breaks_lines_on_crlf_and_lone_cr_like_the_server() {
    for content in ["a0\r\na1\r\na2\r\n", "a0\ra1\ra2\r", "a0\na1\r\na2\r"] {
        let source = cached(content);
        assert_eq!(
            slice_range_content(&source, &range(1, 1)),
            "a1",
            "{content:?}"
        );
        assert_eq!(
            slice_range_content(&source, &range(0, 2)),
            "a0\na1\na2",
            "{content:?}"
        );
        // Past the last content line: nothing, and no trailing empty line.
        assert_eq!(
            slice_range_content(&source, &range(3, 3)),
            "",
            "{content:?}"
        );
        assert_eq!(
            slice_range_content(&source, &range(2, 9)),
            "a2",
            "{content:?}"
        );
    }
    let source = cached("x\r😀 é\ry");
    let emoji_line = LspRange {
        start: LspExactPosition {
            line: 1,
            character: 3,
        },
        end: LspExactPosition {
            line: 1,
            character: 4,
        },
    };
    assert_eq!(slice_range_content(&source, &emoji_line), "😀 é");
}

#[test]
fn slice_range_excludes_end_line_when_end_character_is_zero() {
    // LSP end-exclusive: {start:{0,0}, end:{2,0}} covers lines 0–1 only.
    let content = "line0\nline1\nline2\nline3\n";
    let r = LspRange {
        start: LspExactPosition {
            line: 0,
            character: 0,
        },
        end: LspExactPosition {
            line: 2,
            character: 0,
        },
    };
    assert_eq!(slice_range_content(&cached(content), &r), "line0\nline1");
}

#[test]
fn slice_range_includes_end_line_when_end_character_positive() {
    // A single-line range keeps the whole line (snippet context), not just
    // the [start.character, end.character) span.
    let content = "alpha\nbeta\n";
    let r = LspRange {
        start: LspExactPosition {
            line: 1,
            character: 2,
        },
        end: LspExactPosition {
            line: 1,
            character: 4,
        },
    };
    assert_eq!(slice_range_content(&cached(content), &r), "beta");
}

#[cfg(unix)]
fn make_fifo(path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid NUL-terminated path for the call's duration.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0, "mkfifo");
}

#[cfg(unix)]
#[test]
fn snippet_reads_reject_fifos_and_devices_without_blocking() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let fifo = tmp.0.join("octocode-engine-snippet-fifo");
        make_fifo(&fifo);
        for path in [fifo.to_string_lossy().into_owned(), "/dev/zero".to_owned()] {
            let mut cache = SnippetContentCache::new(SnippetReadPolicy::default());
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                cache.read_range_content(&path, &range(0, 0)),
            )
            .await
            .expect("a non-regular file must be refused, not read or opened");
            let error = result.expect_err("non-regular file rejected");
            assert!(
                error.reason.contains("not a regular file"),
                "{path}: {}",
                error.reason
            );
        }
        let _ = std::fs::remove_file(fifo);
    });
}

#[test]
fn snippet_reads_reject_oversized_files_before_reading() {
    let tmp = TmpDir::new();
    let path = tmp.0.join("octocode-engine-snippet-oversized");
    let file = std::fs::File::create(&path).unwrap();
    // Sparse: the size check must refuse it from metadata alone.
    file.set_len(MAX_SNIPPET_SOURCE_BYTES + 1).unwrap();
    drop(file);
    let error = read_bounded_regular_file(&path, MAX_SNIPPET_SOURCE_BYTES)
        .expect_err("oversized file rejected");
    assert!(error.reason.contains("too large"), "{}", error.reason);
    // The same bound applies through `take`, not only through metadata.
    std::fs::write(&path, "abcdef").unwrap();
    assert!(read_bounded_regular_file(&path, 5).is_err());
    assert_eq!(read_bounded_regular_file(&path, 6).unwrap(), "abcdef");
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[test]
fn unauthorized_snippet_paths_are_never_touched() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // A FIFO would block any open or read: reaching it proves the policy
        // was consulted first.
        let tmp = TmpDir::new();
        let fifo = tmp.0.join("octocode-engine-snippet-unauthorized");
        make_fifo(&fifo);
        let tmp = TmpDir::new();
        let allowed = tmp.0.join("octocode-engine-snippet-authorized");
        std::fs::write(&allowed, "allowed line\n").unwrap();
        let consulted = Arc::new(AtomicUsize::new(0));
        let policy = {
            let consulted = Arc::clone(&consulted);
            let allowed = allowed.clone();
            SnippetReadPolicy::with_authorizer(move |path| {
                consulted.fetch_add(1, Ordering::SeqCst);
                (path == allowed).then(|| path.to_path_buf())
            })
        };
        let fifo_uri = path_to_uri(&fifo.to_string_lossy()).unwrap();
        let allowed_uri = path_to_uri(&allowed.to_string_lossy()).unwrap();
        let at = json!({"start":{"line":0,"character":0},"end":{"line":0,"character":3}});
        let locations = tokio::time::timeout(
            Duration::from_secs(10),
            snippets_from_locations(
                &json!([
                    {"uri": fifo_uri, "range": at},
                    {"uri": fifo_uri, "range": at},
                    {"uri": allowed_uri, "range": at}
                ]),
                &policy,
            ),
        )
        .await
        .expect("an unauthorized path is never opened")
        .expect("locations");
        assert_eq!(
            locations.len(),
            3,
            "refused locations are kept, without content"
        );
        assert_eq!(locations[0].content, SNIPPET_CONTENT_WITHHELD);
        assert_eq!(locations[1].content, SNIPPET_CONTENT_WITHHELD);
        assert_eq!(locations[2].content, "allowed line");
        assert_eq!(consulted.load(Ordering::SeqCst), 2, "one decision per file");
        let _ = std::fs::remove_file(fifo);
        let _ = std::fs::remove_file(allowed);
    });
}

#[test]
fn shared_reads_authorize_and_read_each_file_once_across_responses() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let file = tmp.0.join("octocode-engine-snippet-shared");
        std::fs::write(&file, "alpha\nbeta\n").unwrap();
        let uri = path_to_uri(&file.to_string_lossy()).unwrap();
        let consulted = Arc::new(AtomicUsize::new(0));
        let authorizer = {
            let consulted = Arc::clone(&consulted);
            move |path: &Path| {
                consulted.fetch_add(1, Ordering::SeqCst);
                Some(path.to_path_buf())
            }
        };
        let policy = SnippetReadPolicy::with_authorizer(authorizer.clone()).shared_reads();
        let response = |line: u32| {
            json!([{"uri": uri, "range": {"start":{"line":line,"character":0},"end":{"line":line,"character":4}}}])
        };
        let first = snippets_from_locations(&response(0), &policy).await.unwrap();
        // The file is gone: a later response under the same policy (or a
        // clone of it) is served from the request's shared read.
        std::fs::remove_file(&file).unwrap();
        let second = snippets_from_locations(&response(1), &policy.clone())
            .await
            .unwrap();
        assert_eq!(first[0].content, "alpha");
        assert_eq!(second[0].content, "beta");
        assert_eq!(consulted.load(Ordering::SeqCst), 1, "one decision per file");
        // A policy without shared reads (or a new request) reads again.
        let fresh = snippets_from_locations(
            &response(1),
            &SnippetReadPolicy::with_authorizer(authorizer),
        )
        .await
        .unwrap();
        assert!(fresh[0].content.starts_with("[content unavailable"), "{}", fresh[0].content);
        assert_eq!(consulted.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn snippet_policy_reads_the_path_the_authorizer_returns() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let real = tmp.0.join("octocode-engine-snippet-canonical");
        std::fs::write(&real, "canonical content\n").unwrap();
        let target = real.clone();
        let policy = SnippetReadPolicy::with_authorizer(move |_| Some(target.clone()));
        let mut cache = SnippetContentCache::new(policy);
        let content = cache
            .read_range_content("/server/said/elsewhere.rs", &range(0, 0))
            .await
            .unwrap();
        assert_eq!(content, "canonical content");
        std::fs::remove_file(real).unwrap();
    });
}
