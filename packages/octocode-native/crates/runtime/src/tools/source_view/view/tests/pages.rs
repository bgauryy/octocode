use super::*;

fn byte_query(offset: usize) -> LocalFetchQuery {
    query(json!({"readProgress":{"unit":"bytes","offset":offset}}))
}

#[test]
fn line_and_byte_pages_redact_cross_boundary_secrets_and_reconstruct_exactly() {
    let body = "MIIEpAIBAAKCAQEAsplitKeyBodyLineABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let source = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\n{}token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n{}jwt_secret =\n\"s3cr3tValueThatIsLongEnough42\"\n{}",
        filler(10),
        filler(40),
        filler(37),
        filler(160)
    );
    for window in [97, 500, 1200] {
        for q in [query(json!({})), byte_query(0)] {
            let (actual, pages) = walk(&source, q, window);
            assert!(pages > 1);
            let original = source.lines().collect::<Vec<_>>();
            let shown = actual.lines().collect::<Vec<_>>();
            assert_eq!(
                shown.len(),
                original.len(),
                "source coordinates stay intact"
            );
            for (before, after) in original.iter().zip(shown) {
                if before.starts_with("const filler_") || *before == "jwt_secret =" {
                    assert_eq!(*before, after, "non-secret source is preserved");
                }
            }
            for secret in ["splitKeyBody", "ghp_aaaa", "s3cr3t"] {
                assert!(!actual.contains(secret));
            }
        }
    }
}

#[test]
fn ranged_secret_value_is_redacted_and_retains_original_line_coordinates() {
    let source = format!(
        "{}jwt_secret =\n\"s3cr3tValueThatIsLongEnough42\"\n{}",
        filler(9),
        filler(5)
    );
    let out = fetch_window(
        &source,
        &query(json!({"ranges":[{"startLine":11,"endLine":15}]})),
        1000,
    );
    assert!(!out.content.as_deref().expect("content").contains("s3cr3t"));
    assert_eq!(
        out.source_line_ranges,
        vec![LineRange { start: 11, end: 15 }]
    );
}

#[test]
fn byte_progress_inside_long_unicode_line_has_no_gap_or_duplicate() {
    let source = format!("{}needle{}\n", "é🦀".repeat(400), "世界".repeat(400));
    for window in [64, 127, 511] {
        let (actual, pages) = walk(&source, query(json!({})), window);
        assert_eq!(actual, source);
        assert!(pages > 1);
    }
}

#[test]
fn byte_progress_redacts_a_secret_even_when_it_starts_inside_the_block() {
    let body = "MIIEpAIBAAKCAQEAsplitKeyBodyLineABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let source = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\n{}",
        filler(98),
        filler(20)
    );
    let offset = source.find(body).expect("body") + 20;
    let out = fetch_window(&source, &byte_query(offset), 64);
    assert!(
        !out.content
            .as_deref()
            .expect("content")
            .contains("splitKeyBody")
    );
    assert_eq!(out.pagination.as_ref().expect("pagination").offset, offset);
}

#[test]
fn page_scanner_reads_a_bounded_context_window() {
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
    let scanner = Counting(ContentSecurity::new(), std::cell::Cell::new(0));
    let out = process_fetched_content(
        &byte_query(source.len() / 2),
        source.as_bytes(),
        Path::new("/fixture/app.ts"),
        &SourceFacts {
            window: Some(8_000),
            ..SourceFacts::default()
        },
        &scanner,
        &NeverCancel,
    );
    assert!(out.content.is_some());
    assert!(
        scanner.1.get() < 8_000 + 4 * crate::tools::source_view::pagination::PAGE_SCAN_MARGIN_BYTES
    );
}

#[test]
fn private_progress_past_end_is_empty_and_offers_a_fresh_restart() {
    let source = filler(487);
    for q in [
        query(json!({"readProgress":{"unit":"lines","offset":487}})),
        byte_query(source.len()),
    ] {
        let out = fetch_window(&source, &q, 1200);
        assert!(out.out_of_range);
        assert!(out.source_line_ranges.is_empty());
        assert_eq!(out.content.as_deref(), Some(""));
        let next = out.next.expect("recovery");
        assert!(next.continue_window.is_none());
        assert_eq!(
            local_fetch_query::offset(&next.restart.expect("restart").query).unwrap_or(0),
            0
        );
    }
}
