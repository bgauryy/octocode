//! Paging: redaction across page boundaries, byte pages, and offsets
//! past the end.
use super::*;

/// The private-key note is a fact about the returned window: a page
/// that shows a redacted key block says so, a page elsewhere in the
/// same file does not.
#[test]
fn the_key_block_note_rides_only_on_pages_that_show_the_block() {
    let body = "MIIEpAIBAAKCAQEAsplitKeyBodyLineABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let source = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n{}",
        filler(10),
        filler(100)
    );
    let note = "Redacted private-key block(s) found in the source before selecting the window.";
    let shows = fetch(&source, &lines_query(0, 20));
    assert!(
        shows.warnings.iter().any(|w| w == note),
        "{:?}",
        shows.warnings
    );
    let elsewhere = fetch(&source, &lines_query(50, 20));
    assert!(
        !elsewhere.warnings.iter().any(|w| w == note),
        "{:?}",
        elsewhere.warnings
    );
}

#[test]
fn pem_block_split_across_a_line_page_is_redacted_on_both_pages() {
    let body = "MIIEpAIBAAKCAQEAsplitKeyBodyLineABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let source = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\n{}",
        filler(98),
        filler(20)
    );
    // Page 1 ends on line 100 (first body line); page 2 starts on line 101.
    for offset in [0, 100] {
        let result = fetch(&source, &lines_query(offset, 100));
        let content = result.content.expect("content");
        assert!(
            !content.contains("splitKeyBody"),
            "offset {offset}: {content}"
        );
    }
}

#[test]
fn multi_line_secret_straddling_a_page_boundary_is_redacted() {
    let value = "\"s3cr3tValueThatIsLongEnough42\"";
    let source = format!("{}jwt_secret =\n{value}\n{}", filler(9), filler(5));
    let full = security()
        .sanitize(&source, Path::new("/fixture/app.ts"))
        .expect("scan")
        .0;
    assert!(
        !full.contains("s3cr3t"),
        "fixture must be a real multi-line secret"
    );
    // Line 10 holds the key, line 11 the value; page 2 starts on the value.
    let result = fetch(&source, &lines_query(10, 5));
    let content = result.content.expect("content");
    assert!(!content.contains("s3cr3t"), "{content}");
    // The page scan kept line counts, so the page still anchors to 11-15.
    assert_eq!(
        result.source_line_ranges,
        vec![LineRange { start: 11, end: 15 }]
    );
}

#[test]
fn paged_union_equals_the_whole_view_scan() {
    let source = format!(
        "{}token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n{}password =\n\"another-long-secret-value-0001\"\n{}",
        filler(40),
        filler(37),
        filler(60)
    );
    let expected = security()
        .sanitize(&source, Path::new("/fixture/app.ts"))
        .expect("scan")
        .0;
    let mut q = lines_query(0, 13);
    let mut union = String::new();
    loop {
        let result = fetch(&source, &q);
        union.push_str(result.content.as_deref().expect("content"));
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        q = next.query;
    }
    assert_eq!(union, expected);
}

fn bytes_query(offset: usize, chunk: usize) -> LocalFetchQuery {
    LocalFetchQuery {
        unit: Some(WindowUnit::Bytes),
        ..lines_query(offset, chunk)
    }
}

/// Byte pages walked by `next.continueWindow` reproduce the whole-view scan:
/// every secret is redacted, including ones a raw byte boundary would cut.
#[test]
fn byte_pages_redact_secrets_straddling_the_boundary() {
    let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let source = format!(
        "{}token: {token}\n{}password =\n\"another-long-secret-value-0001\"\n{}",
        filler(40),
        filler(37),
        filler(60)
    );
    let expected = security()
        .sanitize(&source, Path::new("/fixture/app.ts"))
        .expect("scan")
        .0;
    assert!(
        !expected.contains("ghp_aaaa"),
        "fixture holds a real secret"
    );
    let cut = source.find(token).expect("token") + 10;
    for chunk in [cut, 97, 500] {
        let mut q = bytes_query(0, chunk);
        let mut union = String::new();
        let mut pages = 0;
        loop {
            let result = fetch(&source, &q);
            let content = result.content.as_deref().expect("content");
            assert!(!content.contains("ghp_aaaa"), "chunk {chunk}: {content}");
            assert!(
                !content.contains("another-long"),
                "chunk {chunk}: {content}"
            );
            union.push_str(content);
            pages += 1;
            let Some(next) = result.next.and_then(|next| next.continue_window) else {
                break;
            };
            q = next.query;
        }
        assert!(pages > 1, "chunk {chunk}");
        assert_eq!(union, expected, "chunk {chunk}");
    }
}

/// A byte window inside one long line that holds secrets returns that
/// window (redacted), not the whole line from column 0: the text starts
/// where `pagination.offset` says and stays near the asked length.
#[test]
fn a_byte_window_in_a_redacted_long_line_stays_in_its_window() {
    let token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let pad = "x".repeat(30_000);
    let source = format!(
        "{pad} aws_secret_access_key = \"AbCdEfGhIjKlMnOpQrStUvWxYz0123456789AbCd\" {token} needle {pad}\n"
    );
    let expected = security()
        .sanitize(&source, Path::new("/fixture/app.ts"))
        .expect("scan")
        .0;
    assert_ne!(expected, source, "fixture holds a real secret");
    let needle = source.find("needle").expect("needle");
    for offset in [0, needle - 20] {
        let result = fetch(&source, &bytes_query(offset, 100));
        let content = result.content.as_deref().expect("content");
        assert!(!content.contains("ghp_aaaa"), "{offset}: {content}");
        assert!(content.len() < 200, "{offset}: {} bytes", content.len());
        let page = result.pagination.as_ref().expect("pagination");
        assert_eq!(page.offset, offset, "{page:?}");
        assert!(page.has_more, "{page:?}");
        assert!(
            source[offset..].starts_with(&content[..content.len().min(10)])
                || content.starts_with(octocode_engine::security::REDACTION_PREFIX),
            "{offset}: {content}"
        );
    }
    let near = fetch(&source, &bytes_query(needle - 20, 100));
    assert!(
        near.content
            .as_deref()
            .is_some_and(|c| c.contains("needle")),
        "{near:?}"
    );
}

#[test]
fn byte_page_split_inside_a_pem_block_leaks_nothing() {
    let body = "MIIEpAIBAAKCAQEAsplitKeyBodyLineABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let source = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\n{}",
        filler(98),
        filler(20)
    );
    let cut = source.find(body).expect("body") + 20;
    for offset in [0, cut] {
        let result = fetch(&source, &bytes_query(offset, 64));
        let content = result.content.expect("content");
        assert!(
            !content.contains("splitKeyBody"),
            "offset {offset}: {content}"
        );
    }
}

/// The byte page scans a bounded window, not the whole view: the secret
/// scanner sees the page plus its line-aligned margins only.
#[test]
fn byte_page_scans_a_window_not_the_whole_view() {
    struct Counting(ContentSecurity, std::cell::Cell<usize>);
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            path: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            self.1.set(self.1.get() + text.len());
            self.0.sanitize(text, path)
        }
    }
    let source = (0..120_000)
        .map(|i| format!("const value_{i} = compute('abcdefghij', {i}); // key line {i}\n"))
        .collect::<String>();
    let scanner = Counting(security(), std::cell::Cell::new(0));
    let started = std::time::Instant::now();
    let paged = process_fetched_content(
        &bytes_query(source.len() / 2, 8_000),
        source.as_bytes(),
        Path::new("/fixture/app.ts"),
        &SourceFacts::default(),
        &scanner,
        &NeverCancel,
    );
    let page_time = started.elapsed();
    assert!(paged.content.is_some_and(|content| content.len() >= 8_000));
    let scanned = scanner.1.get();
    assert!(
        scanned < 8_000 + 4 * crate::tools::source_view::pagination::PAGE_SCAN_MARGIN_BYTES,
        "scanned {scanned} of {} bytes",
        source.len()
    );
    let started = std::time::Instant::now();
    let _ = security().sanitize(&source, Path::new("/fixture/app.ts"));
    eprintln!(
        "byte page {page_time:?} (scanned {scanned} B) vs whole-view scan {:?} ({} B)",
        started.elapsed(),
        source.len()
    );
}

#[test]
fn offset_past_the_end_is_empty_out_of_range_with_restart() {
    let source = filler(487);
    let result = fetch(&source, &lines_query(487, 100));
    assert_eq!(result.content.as_deref(), Some(""));
    assert!(
        result.source_line_ranges.is_empty(),
        "{:?}",
        result.source_line_ranges
    );
    assert!(result.out_of_range);
    assert_eq!(result.is_partial, None);
    let next = result.next.clone().expect("next");
    assert!(next.continue_window.is_none());
    assert_eq!(next.restart.expect("restart").query.offset, Some(0));
    let wire = serde_json::to_value(&result).expect("wire");
    assert_eq!(wire["pagination"]["outOfRange"], true, "{wire}");
    assert!(wire.get("sourceLineRanges").is_none(), "{wire}");

    let bytes = LocalFetchQuery {
        unit: Some(WindowUnit::Bytes),
        ..lines_query(source.len(), 100)
    };
    let result = fetch(&source, &bytes);
    assert!(result.out_of_range && result.source_line_ranges.is_empty());
    // The last real page is not out of range.
    assert!(!fetch(&source, &lines_query(400, 100)).out_of_range);
}
