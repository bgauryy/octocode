//! Reading the source itself: decoding, BOM, binary bytes, cancellation,
//! sparse checkouts, and one preparation per version.

use super::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

#[test]
fn missing_files_in_a_sparse_checkout_say_so() {
    let t = Fixture::new();
    let git = t.0.join("clone/.git");
    fs::create_dir_all(git.join("info")).expect("fixture");
    fs::write(git.join("info/sparse-checkout"), "/*\n!/*/\n/src/\n").expect("fixture");
    fs::write(git.join("config"), "[core]\n\tsparseCheckout = true\n").expect("fixture");
    let policy = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(t.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let mut req = test_local_fetch_query();
    local_fetch_query::set_path(&mut req, "clone/docs/guide.md").expect("path");
    let r = fetch(&req, &policy);
    assert!(r.resource_missing, "{r:?}");
    let error = r.error.as_deref().unwrap_or_default();
    assert!(error.contains("clone/docs/guide.md"), "{error}");
    assert!(error.contains("sparse checkout"), "{error}");

    assert_eq!(r.error_code, Some(ErrorCode::PathNotFound), "{r:?}");

    // An ordinary repository keeps the plain not-found message, under
    // the not-found code the other local tools use; the recovery advice
    // is the row's hint, not a second copy in the message.
    fs::write(git.join("config"), "[core]\n\tbare = false\n").expect("fixture");
    let r = fetch(&req, &policy);
    assert_eq!(
        r.error.as_deref(),
        Some("Path does not exist: clone/docs/guide.md"),
        "{r:?}"
    );
    assert_eq!(r.error_code, Some(ErrorCode::PathNotFound), "{r:?}");
    assert!(r.resource_missing, "{r:?}");
}

#[test]
fn cancellation_is_checked_around_io() {
    let t = Fixture::new();
    let p = t.0.join("a");
    fs::write(&p, "text").expect("test fixture operation should succeed");
    let r = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Safe,
        &Cancel(AtomicUsize::new(0)),
        None,
        None,
        &Default::default(),
    );
    assert_eq!(r.error_code, Some(ErrorCode::Cancelled));
}

#[test]
fn latin1_text_decodes_with_a_flag_and_long_lines_fall_back_to_bytes() {
    let t = Fixture::new();
    // "café\nnaïve" in ISO-8859-1: text, but not valid UTF-8.
    let latin1 = t.0.join("latin1.txt");
    fs::write(&latin1, b"caf\xe9\nna\xefve\n").expect("latin-1 fixture should be written");
    let paths = Paths(t.0.clone());
    let decoded = fetch(&q(&latin1), &paths);
    assert_eq!(decoded.error_code, None, "{decoded:?}");
    assert_eq!(decoded.content.as_deref(), Some("café\nnaïve\n"));
    assert!(
        decoded.warnings.iter().any(|w| w.contains("Latin-1")),
        "{:?}",
        decoded.warnings
    );
    // Mostly UTF-8 with one stray byte stays UTF-8 (lossy), also flagged.
    let stray = t.0.join("stray.txt");
    fs::write(
        &stray,
        "é \u{2014} ok\n"
            .bytes()
            .chain([0xff, b'\n'])
            .collect::<Vec<_>>(),
    )
    .expect("stray fixture should be written");
    let lossy = fetch(&q(&stray), &paths);
    assert_eq!(lossy.content.as_deref(), Some("é \u{2014} ok\n\u{fffd}\n"));
    assert!(
        lossy.warnings.iter().any(|w| w.contains("UTF-8")),
        "{:?}",
        lossy.warnings
    );

    let long = t.0.join("long.txt");
    fs::write(&long, "x".repeat(18_000)).expect("long fixture should be written");
    let long_result = fetch(&q(&long), &paths);
    let pagination = long_result.pagination.expect("long result should paginate");
    assert_eq!(pagination.unit, ReadProgressUnit::Bytes);
    assert_eq!(pagination.length, 16_384);
    assert!(pagination.has_more);
}

/// A leading UTF-8 BOM is not text of line 1: a line-anchored pattern
/// matches it there and line 1 shows without it, as localSearch does, in
/// every dialect and in a plain read; the source size stays the file's.
#[test]
fn a_leading_bom_is_not_text_of_line_one() {
    let t = Fixture::new();
    let p = t.0.join("bom.txt");
    fs::write(&p, "\u{feff}alpha\nbeta alpha\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for regex in ["rust", "pcre2"] {
        let r = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["^alpha"], "regexEngine": regex, "contextLines":{"before":0,"after":0}}),
            ),
            &paths,
        );
        assert_eq!(r.matched_lines, vec![1], "{regex}: {r:?}");
        assert_eq!(r.content.as_deref(), Some("alpha\n"), "{regex}");
    }
    let plain = fetch(&qj(&p, serde_json::json!({})), &paths);
    assert_eq!(
        plain.content.as_deref(),
        Some("alpha\nbeta alpha\n"),
        "{plain:?}"
    );
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["alpha"], "wholeWord": true, "contextLines":{"before":0,"after":0}}),
        ),
        &paths,
    );
    assert_eq!(r.matched_lines, vec![1, 2], "{r:?}");
}

/// A NUL byte past the binary sniff's head sample makes the file binary.
#[test]
fn a_late_nul_byte_makes_an_in_memory_source_binary() {
    let t = Fixture::new();
    let p = t.0.join("late.txt");
    let mut bytes = "text line\n".repeat(1_000).into_bytes();
    bytes.extend_from_slice(b"\0\x01\x02tail\n");
    fs::write(&p, bytes).expect("fixture");
    let r = fetch(&q(&p), &Paths(t.0.clone()));
    assert_eq!(
        r.error_code,
        Some(ErrorCode::BinaryFileUnsupported),
        "{r:?}"
    );
}

/// The pages of one large file version decode and key-redact it once; an
/// edited version is prepared again.
#[test]
fn a_large_source_is_prepared_once_per_version() {
    struct CountingKeys(AtomicUsize);
    impl ContentScan for CountingKeys {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::Relaxed);
            crate::security::redact_private_key_blocks(content)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("large.txt");
    let line = format!("{}\n", "x".repeat(79));
    fs::write(&p, line.repeat(4_000)).expect("fixture");
    let scan = CountingKeys(AtomicUsize::new(0));
    let read = |req: &LocalFetchQuery| {
        execute_local_fetch(
            req,
            &Paths(t.0.clone()),
            &scan,
            &NeverCancel,
            None,
            None,
            &Default::default(),
        )
    };
    let first = read(&q(&p));
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("a second page")
        .query;
    let second = read(&next);
    assert_eq!(second.status, ReadStatus::Success, "{second:?}");
    assert_eq!(
        scan.0.load(Ordering::Relaxed),
        1,
        "one preparation for both pages"
    );
    fs::write(&p, line.repeat(4_001)).expect("edit");
    assert_eq!(read(&q(&p)).total_lines, Some(4_001));
    assert_eq!(
        scan.0.load(Ordering::Relaxed),
        2,
        "a new version is prepared again"
    );
}

/// Rows of one batch reading the same large version at once prepare it
/// once: the rest wait for the first preparation and share it, whether they
/// read the same window or different ones.
#[test]
fn concurrent_rows_share_one_preparation() {
    struct SlowKeys(AtomicUsize);
    impl ContentScan for SlowKeys {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::Relaxed);
            // Long enough that every row arrives while the first prepares.
            std::thread::sleep(std::time::Duration::from_millis(200));
            crate::security::redact_private_key_blocks(content)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("shared.txt");
    let line = format!("{}\n", "x".repeat(79));
    fs::write(&p, line.repeat(4_000)).expect("fixture");
    let scan = SlowKeys(AtomicUsize::new(0));
    let rows = [
        q(&p),
        q(&p),
        qj(
            &p,
            serde_json::json!({"ranges":[{"startLine":10,"endLine":20}]}),
        ),
        qj(
            &p,
            serde_json::json!({"matchString": ["xx"], "contextLines":{"before":0,"after":0}}),
        ),
    ];
    let start = std::sync::Barrier::new(rows.len());
    let results = std::thread::scope(|scope| {
        let workers = rows
            .iter()
            .map(|row| {
                let (scan, start, paths) = (&scan, &start, Paths(t.0.clone()));
                scope.spawn(move || {
                    start.wait();
                    execute_local_fetch(
                        row,
                        &paths,
                        scan,
                        &NeverCancel,
                        None,
                        None,
                        &Default::default(),
                    )
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("row"))
            .collect::<Vec<_>>()
    });
    assert!(
        results.iter().all(|result| result.error.is_none()),
        "{results:?}"
    );
    assert_eq!(scan.0.load(Ordering::Relaxed), 1, "one shared preparation");
}
