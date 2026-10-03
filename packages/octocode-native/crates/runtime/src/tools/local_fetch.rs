mod block;
pub(crate) use block::declaration_spans;
mod executor;
mod extraction;
mod large_source;
mod pagination;
mod types;
mod validation;
pub(crate) use executor::no_match_hint;
pub use executor::{execute_local_fetch, execute_local_fetch_with_regex, process_fetched_content};
pub use types::*;
pub use validation::validate_request;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::scan::ContentScan;
    use crate::tools::cancel::{CancellationCheck, NeverCancel};
    use sha2::Digest;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "local-fetch-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("test fixture operation should succeed")
                    .as_nanos(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).expect("test fixture operation should succeed");
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Paths(PathBuf);
    impl PathAccess for Paths {
        fn validate_read(&self, p: &Path) -> Result<ValidatedRead, PathFailure> {
            let p = if p.is_absolute() {
                p.into()
            } else {
                self.0.join(p)
            };
            p.canonicalize()
                .map(|canonical| ValidatedRead {
                    canonical,
                    display: p.to_string_lossy().into_owned(),
                })
                .map_err(|error| PathFailure {
                    code: "fileAccessFailed".into(),
                    message: error.to_string(),
                    safe_path: None,
                    resource_missing: false,
                    sparse_checkout: false,
                })
        }
    }
    struct Safe;
    impl ContentScan for Safe {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((
                text.replace("SECRET", "[REDACTED]"),
                if text.contains("SECRET") {
                    vec!["Secrets detected and redacted: synthetic".into()]
                } else {
                    vec![]
                },
            ))
        }
    }
    fn q(path: &Path) -> LocalFetchQuery {
        LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ..LocalFetchQuery::test_default()
        }
    }
    #[test]
    fn exact_line_and_continuation_union() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "one\ntwo 😀\nthree\n").expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.chunk_type = Some(ChunkType::Lines);
        req.chunk_size = wire_positive(1);
        let mut joined = String::new();
        loop {
            let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
            assert_eq!(r.status, "success");
            joined.push_str(
                r.content
                    .as_deref()
                    .expect("test fixture operation should succeed"),
            );
            let Some(next) = r.next.and_then(|n| n.r#continue) else {
                break;
            };
            req = next.query
        }
        assert_eq!(joined, "one\ntwo 😀\nthree\n")
    }
    #[test]
    fn regex_line_anchors_match_every_line() {
        let t = Temp::new();
        let p = t.0.join("Cargo.toml");
        fs::write(&p, "[package]\nversion = \"1\"\n[dep]\nversion = \"2\"\n")
            .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("^version.*\"$".parse().expect("match string"));
        req.match_string_is_regex = Some(true);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(execute_local_fetch(&req, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(wire["matchedLines"], serde_json::json!([2, 4]));
    }
    #[test]
    fn wire_form_omits_metadata_derivable_from_emitted_fields() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "zero\none\nneedle\nthree\nfour\n")
            .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("needle".parse().expect("match string"));
        req.context_lines = Some(1);
        let wire = serde_json::to_value(execute_local_fetch(&req, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(wire["content"], "one\nneedle\nthree\n");
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"start":2,"end":4}])
        );
        assert_eq!(wire["matchedLines"], serde_json::json!([3]));
        assert_eq!(wire["totalLines"], 5);
        assert_eq!(wire["sourceBytes"], 27);
        assert_eq!(wire["returnedBytes"], 17);
        for redundant in [
            "contentView",
            "startLine",
            "endLine",
            "matchRanges",
            "selectedMatchCount",
            "sourceChars",
            "returnedChars",
            "returnedLines",
            "pagination",
        ] {
            assert!(wire.get(redundant).is_none(), "{redundant}: {wire}");
        }

        let mut paged = q(&p);
        paged.chunk_size = wire_positive(2);
        let wire = serde_json::to_value(execute_local_fetch(&paged, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(
            wire["pagination"],
            serde_json::json!({"chunkType":"lines","offset":0,"chunkSize":2,"hasMore":true,"nextOffset":2})
        );
        assert!(wire["next"]["continue"].is_object());
        paged.offset = Some(4);
        let last = serde_json::to_value(execute_local_fetch(&paged, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(
            last["pagination"],
            serde_json::json!({"chunkType":"lines","offset":4,"length":1,"chunkSize":2,"hasMore":false})
        );

        let wide = t.0.join("wide.txt");
        fs::write(&wide, "a😀\n").expect("test fixture operation should succeed");
        let wire =
            serde_json::to_value(execute_local_fetch(&q(&wide), &paths, &Safe, &NeverCancel))
                .expect("serializable");
        assert_eq!(wire["sourceChars"], 4);
        assert_eq!(wire["sourceBytes"], 6);
        assert_eq!(wire["returnedChars"], 4);
    }
    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("l{i}\n")).collect()
    }
    #[test]
    fn match_windows_are_separated_by_omission_marker() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(
            &p,
            numbered(30)
                .replace("l3\n", "hit3\n")
                .replace("l25\n", "hit25\n"),
        )
        .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("hit".parse().expect("match string"));
        req.context_lines = Some(1);
        let wire = serde_json::to_value(execute_local_fetch(&req, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(
            wire["content"],
            "l2\nhit3\nl4\n... [lines 5-23 omitted] ...\nl24\nhit25\nl26\n"
        );
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"start":2,"end":4},{"start":24,"end":26}])
        );
        assert_eq!(wire["matchedLines"], serde_json::json!([3, 25]));
    }
    #[test]
    fn adjacent_match_windows_have_no_marker() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(
            &p,
            numbered(10)
                .replace("l3\n", "hit3\n")
                .replace("l5\n", "hit5\n"),
        )
        .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("hit".parse().expect("match string"));
        req.context_lines = Some(1);
        let wire = serde_json::to_value(execute_local_fetch(&req, &paths, &Safe, &NeverCancel))
            .expect("serializable");
        assert_eq!(wire["content"], "l2\nhit3\nl4\nhit5\nl6\n");
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"start":2,"end":6}])
        );
    }
    #[test]
    fn omission_marker_survives_redaction() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(
            &p,
            numbered(10)
                .replace("l2\n", "hit\n")
                .replace("l9\n", "hit SECRET\n"),
        )
        .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("hit".parse().expect("match string"));
        req.context_lines = Some(0);
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.content.as_deref(),
            Some("hit\n... [lines 3-8 omitted] ...\nhit [REDACTED]\n")
        );
        // Redaction keeps line counts, so the anchors stay; the warning says
        // the text is not verbatim.
        assert_eq!(
            r.source_line_ranges,
            vec![
                LineRange { start: 2, end: 2 },
                LineRange { start: 9, end: 9 }
            ]
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("not verbatim")),
            "{:?}",
            r.warnings
        );
    }
    #[test]
    fn paged_match_windows_map_source_ranges_around_marker() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(
            &p,
            numbered(30)
                .replace("l3\n", "hit3\n")
                .replace("l25\n", "hit25\n"),
        )
        .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("hit".parse().expect("match string"));
        req.context_lines = Some(1);
        req.chunk_type = Some(ChunkType::Lines);
        req.chunk_size = wire_positive(4);
        let first = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(
            first.content.as_deref(),
            Some("l2\nhit3\nl4\n... [lines 5-23 omitted] ...\n")
        );
        assert_eq!(
            first.source_line_ranges,
            vec![LineRange { start: 2, end: 4 }]
        );
        let next = first
            .next
            .and_then(|n| n.r#continue)
            .expect("second page")
            .query;
        let second = execute_local_fetch(&next, &paths, &Safe, &NeverCancel);
        assert_eq!(second.content.as_deref(), Some("l24\nhit25\nl26\n"));
        assert_eq!(
            second.source_line_ranges,
            vec![LineRange { start: 24, end: 26 }]
        );
    }
    #[test]
    fn matches_on_minified_lines_default_to_byte_windows() {
        let t = Temp::new();
        let p = t.0.join("bundle.min.js");
        let filler = "x".repeat(3_000);
        let source = format!("{filler}needle{filler}needle{filler}\nshort needle\n");
        fs::write(&p, &source).expect("fixture");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("needle".parse().expect("match string"));
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        let content = r.content.expect("content");
        assert_eq!(content.matches("needle").count(), 3, "{content:?}");
        assert!(content.len() < 2_000, "{} bytes", content.len());
        for part in content.split('\n').filter(|part| !part.is_empty()) {
            assert!(
                part.starts_with("... [") || source.contains(part),
                "{part:?}"
            );
        }
        // The cut windows are not the whole lines: one read reaches them.
        let next = r.next.clone().expect("next");
        assert_eq!(
            next.whole_lines
                .as_ref()
                .map(|read| read.query.context_lines),
            Some(Some(0)),
            "{next:?}"
        );
        assert!(next.r#continue.is_none(), "{next:?}");
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange { start: 1, end: 2 }],
            "anchor kept"
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("contextLines")),
            "{:?}",
            r.warnings
        );
        // An explicit contextLines keeps whole lines.
        req.context_lines = Some(0);
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert!(
            r.warnings.iter().all(|w| !w.contains("contextLines")),
            "{:?}",
            r.warnings
        );
    }
    #[test]
    fn missing_files_in_a_sparse_checkout_say_so() {
        let t = Temp::new();
        let git = t.0.join("clone/.git");
        fs::create_dir_all(git.join("info")).expect("fixture");
        fs::write(git.join("info/sparse-checkout"), "/*\n!/*/\n/src/\n").expect("fixture");
        fs::write(git.join("config"), "[core]\n\tsparseCheckout = true\n").expect("fixture");
        let policy = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(t.0.clone()),
            ..Default::default()
        })
        .expect("policy");
        let mut req = LocalFetchQuery::test_default();
        req.path = "clone/docs/guide.md".parse().expect("path");
        let r = execute_local_fetch(&req, &policy, &Safe, &NeverCancel);
        assert!(r.resource_missing, "{r:?}");
        let error = r.error.as_deref().unwrap_or_default();
        assert!(error.contains("clone/docs/guide.md"), "{error}");
        assert!(error.contains("sparse checkout"), "{error}");

        // An ordinary repository keeps the plain not-found message.
        fs::write(git.join("config"), "[core]\n\tbare = false\n").expect("fixture");
        let r = execute_local_fetch(&req, &policy, &Safe, &NeverCancel);
        assert!(
            !r.error.as_deref().unwrap_or_default().contains("sparse"),
            "{r:?}"
        );
    }
    #[test]
    fn redacted_chunk_and_range_reads_keep_their_source_anchor() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, numbered(30).replace("l12\n", "l12 SECRET\n")).expect("fixture");
        let paths = Paths(t.0.clone());
        let mut chunk = q(&p);
        chunk.chunk_type = Some(ChunkType::Lines);
        chunk.offset = Some(wire_count(10));
        chunk.chunk_size = wire_positive(5);
        let mut range = q(&p);
        range.start_line = wire_positive(11);
        range.end_line = wire_positive(13);
        for (req, expected) in [(chunk, (11, 15)), (range, (11, 13))] {
            let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
            assert!(
                r.content
                    .as_deref()
                    .is_some_and(|c| c.contains("[REDACTED]")),
                "{r:?}"
            );
            assert_eq!(
                r.source_line_ranges,
                vec![LineRange {
                    start: expected.0,
                    end: expected.1
                }]
            );
            assert!(
                r.warnings.iter().any(|w| w.contains("not verbatim")),
                "{:?}",
                r.warnings
            );
        }
    }
    #[test]
    fn context_bytes_overlapping_windows_do_not_duplicate_source() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        let source = "aaa needle bbb\nline2\nline3\nline4\nccc needle ddd\nneedle x needle\n";
        fs::write(&p, source).expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("needle".parse().expect("match string"));
        req.context_bytes = Some(4);
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        let content = r.content.expect("content");
        for part in content.split("\n").filter(|part| !part.is_empty()) {
            assert!(
                part.starts_with("... [") || source.contains(part),
                "fabricated text {part:?} in {content:?}"
            );
        }
        assert!(content.contains("bytes omitted"), "{content:?}");
    }
    #[test]
    fn multiline_matches_preserve_every_matched_source_line() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "before\nfirst\nsecond\nafter\n").expect("fixture");
        let paths = Paths(t.0.clone());
        for (pattern, is_regex) in [("first\nsecond", false), (r"first\nsecond", true)] {
            let mut req = q(&p);
            req.match_string = Some(pattern.parse().expect("match string"));
            req.match_string_is_regex = Some(is_regex);
            req.context_lines = Some(0);
            let wire = serde_json::to_value(execute_local_fetch(&req, &paths, &Safe, &NeverCancel))
                .expect("serializable");
            assert_eq!(wire["content"], "first\nsecond\n", "{pattern:?}: {wire}");
            assert_eq!(wire["matchedLines"], serde_json::json!([2, 3]));
            assert_eq!(
                wire["sourceLineRanges"],
                serde_json::json!([{"start": 2, "end": 3}])
            );
        }
    }

    #[test]
    fn context_bytes_keep_original_offsets_when_unicode_lowercase_expands() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        let paths = Paths(t.0.clone());
        for (source, pattern, expected) in [
            ("İneedle tail\n", "needle", "needle"),
            ("İNEEDLE\n", "needle", "NEEDLE"),
            ("prefix İ tail\n", "İ", "İ"),
            ("İİneedle", "needle", "needle"),
        ] {
            fs::write(&p, source).expect("fixture");
            let mut req = q(&p);
            req.match_string = Some(pattern.parse().expect("match string"));
            req.context_bytes = Some(0);
            let result = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
            assert_eq!(result.content.as_deref(), Some(expected), "{source:?}");
        }
    }

    #[test]
    fn byte_pages_preserve_utf8_and_use_byte_offsets() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "a😀b").expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.chunk_type = Some(ChunkType::Bytes);
        req.chunk_size = wire_positive(2);
        let a = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(a.content.as_deref(), Some("a😀"));
        req = a
            .next
            .expect("test fixture operation should succeed")
            .r#continue
            .expect("test fixture operation should succeed")
            .query;
        let b = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(b.content.as_deref(), Some("b"));
        assert_eq!(b.returned_chars, Some(1))
    }
    #[test]
    fn ranges_matches_redaction_and_binary() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "zero\nneedle SECRET\nlast\n")
            .expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.match_string = Some("needle".parse().expect("match string"));
        req.context_lines = Some(0);
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        let expected_hash = hex::encode(sha2::Sha256::digest(b"zero\nneedle SECRET\nlast\n"));
        assert_eq!(r.source_sha256.as_deref(), Some(expected_hash.as_str()));
        assert_eq!(r.content.as_deref(), Some("needle [REDACTED]\n"));
        assert_eq!(r.match_ranges, vec![LineRange { start: 2, end: 2 }]);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange { start: 2, end: 2 }],
            "line-preserving redaction keeps the source anchor"
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("not verbatim")),
            "{:?}",
            r.warnings
        );
        let bin = t.0.join("b.bin");
        fs::write(&bin, [0, 1, 2]).expect("test fixture operation should succeed");
        assert_eq!(
            execute_local_fetch(&q(&bin), &paths, &Safe, &NeverCancel)
                .error_code
                .as_deref(),
            Some("binaryFileUnsupported")
        )
    }
    struct Cancel(AtomicUsize);
    impl CancellationCheck for Cancel {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn cancellation_is_checked_around_io() {
        let t = Temp::new();
        let p = t.0.join("a");
        fs::write(&p, "text").expect("test fixture operation should succeed");
        let r = execute_local_fetch(
            &q(&p),
            &Paths(t.0.clone()),
            &Safe,
            &Cancel(AtomicUsize::new(0)),
        );
        assert_eq!(r.error_code.as_deref(), Some("cancelled"))
    }
    #[test]
    fn schema_rejects_unknown_fields_and_invalid_combinations() {
        assert!(
            serde_json::from_value::<LocalFetchQuery>(
                serde_json::json!({"path":"x","madeUp":true})
            )
            .is_err()
        );
        let mut req = q(Path::new("x"));
        req.full_content = Some(true);
        req.match_string = Some("x".parse().expect("match string"));
        assert!(validate_request(&req).is_err())
    }

    #[test]
    fn latin1_text_decodes_with_a_flag_and_long_lines_fall_back_to_bytes() {
        let t = Temp::new();
        // "café\nnaïve" in ISO-8859-1: text, but not valid UTF-8.
        let latin1 = t.0.join("latin1.txt");
        fs::write(&latin1, b"caf\xe9\nna\xefve\n").expect("latin-1 fixture should be written");
        let paths = Paths(t.0.clone());
        let decoded = execute_local_fetch(&q(&latin1), &paths, &Safe, &NeverCancel);
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
        let lossy = execute_local_fetch(&q(&stray), &paths, &Safe, &NeverCancel);
        assert_eq!(lossy.content.as_deref(), Some("é \u{2014} ok\n\u{fffd}\n"));
        assert!(
            lossy.warnings.iter().any(|w| w.contains("UTF-8")),
            "{:?}",
            lossy.warnings
        );

        let long = t.0.join("long.txt");
        fs::write(&long, "x".repeat(18_000)).expect("long fixture should be written");
        let long_result = execute_local_fetch(&q(&long), &paths, &Safe, &NeverCancel);
        let pagination = long_result.pagination.expect("long result should paginate");
        assert_eq!(pagination.chunk_type, ChunkType::Bytes);
        assert_eq!(pagination.length, 16_384);
        assert!(pagination.has_more);
    }

    #[test]
    fn full_content_continuation_pages_by_the_byte_budget_not_100_lines() {
        let t = Temp::new();
        let paths = Paths(t.0.clone());
        let file = t.0.join("short-lines.rs");
        fs::write(&file, "let x = 1;\n".repeat(6_000)).expect("fixture should be written");
        let mut full = q(&file);
        full.full_content = Some(true);
        let limited = execute_local_fetch(&full, &paths, &Safe, &NeverCancel);
        assert_eq!(
            limited.partial_reasons,
            vec![PartialReason::FullContentLimit]
        );
        assert!(limited.pagination.as_ref().expect("first page").length > 1_000);
        let next = limited
            .next
            .and_then(|next| next.r#continue)
            .expect("continuation");
        let page = execute_local_fetch(&next.query, &paths, &Safe, &NeverCancel);
        let pagination = page.pagination.expect("paged");
        // 11-byte lines: a 16 KiB page holds ~1,489 lines, not 100.
        assert!(pagination.length > 1_000, "{pagination:?}");
        assert!(page.content.as_deref().unwrap_or_default().len() <= 16_384);
    }

    #[test]
    fn full_content_limits_return_executable_continuations() {
        let t = Temp::new();
        let paths = Paths(t.0.clone());
        let view_limited = t.0.join("view.txt");
        fs::write(&view_limited, "x".repeat(60_000)).expect("view fixture should be written");
        let mut compact = q(&view_limited);
        compact.full_content = Some(true);
        compact.minify = Some(MinifyMode::Standard);
        let result = execute_local_fetch(&compact, &paths, &Safe, &NeverCancel);
        // Page 1 of the view is returned inline with the continuation.
        assert_eq!(result.error_code, None);
        assert!(
            result
                .content
                .as_deref()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::FullContentLimit]
        );
        assert!(result.next.and_then(|next| next.r#continue).is_some());

        let source_temp = Temp::new();
        let source_limited = source_temp.0.join("source.txt");
        fs::write(&source_limited, "y".repeat(110 * 1024))
            .expect("source fixture should be written");
        let mut full = q(&source_limited);
        full.full_content = Some(true);
        let result = execute_local_fetch(&full, &Paths(source_temp.0.clone()), &Safe, &NeverCancel);
        assert_eq!(result.error_code, None);
        assert!(
            result
                .content
                .as_deref()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::FullContentLimit]
        );
        assert!(result.next.and_then(|next| next.r#continue).is_some());
    }

    #[test]
    fn security_limit_is_terminal_but_offers_a_bounded_line_read() {
        struct Limited;
        impl ContentScan for Limited {
            fn sanitize(
                &self,
                _: &str,
                _: &Path,
            ) -> Result<(String, Vec<String>), (String, String)> {
                Err(("contentSecurityLimit".into(), "limited".into()))
            }
        }
        let t = Temp::new();
        let p = t.0.join("large.txt");
        fs::write(&p, "one\ntwo\n").expect("security fixture should be written");
        let result = execute_local_fetch(&q(&p), &Paths(t.0.clone()), &Limited, &NeverCancel);
        assert_eq!(result.error_code.as_deref(), Some("contentSecurityLimit"));
        assert_eq!(result.terminal_limit, Some(true));
        assert!(
            result
                .next
                .and_then(|next| next.read_bounded_lines)
                .is_some()
        );
    }

    /// A query built from wire JSON (fields the test does not name default).
    fn qj(path: &Path, fields: serde_json::Value) -> LocalFetchQuery {
        let mut query = serde_json::json!({"path": path.to_string_lossy(), "goal": "test", "reasoning": "test"});
        for (key, value) in fields.as_object().expect("object") {
            query[key] = value.clone();
        }
        serde_json::from_value(query).expect("localFetch query")
    }

    #[test]
    fn ranges_read_several_windows_in_one_row() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, numbered(30)).expect("fixture");
        let paths = Paths(t.0.clone());
        // Unordered, overlapping, and past-the-end ranges merge and clamp.
        let req = qj(
            &p,
            serde_json::json!({"ranges": ["20-21", "2-3", "3-4", "29-40"]}),
        );
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(r.error, None, "{r:?}");
        assert_eq!(
            r.content.as_deref(),
            Some(
                "l2\nl3\nl4\n... [lines 5-19 omitted] ...\nl20\nl21\n... [lines 22-28 omitted] ...\nl29\nl30\n"
            )
        );
        assert_eq!(
            r.source_line_ranges,
            vec![
                LineRange { start: 2, end: 4 },
                LineRange { start: 20, end: 21 },
                LineRange { start: 29, end: 30 }
            ]
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("29-40")),
            "{:?}",
            r.warnings
        );
        // Old startLine/endLine reads are unchanged.
        let old = qj(&p, serde_json::json!({"startLine": 2, "endLine": 3}));
        assert_eq!(
            execute_local_fetch(&old, &paths, &Safe, &NeverCancel)
                .content
                .as_deref(),
            Some("l2\nl3\n")
        );
        // Ranges wholly past the end are an error, not an empty success.
        let past = qj(&p, serde_json::json!({"ranges": ["40-41"]}));
        assert!(
            execute_local_fetch(&past, &paths, &Safe, &NeverCancel)
                .error
                .is_some()
        );
    }

    #[test]
    fn block_reads_through_the_enclosing_declaration() {
        let t = Temp::new();
        let p = t.0.join("m.py");
        fs::write(
            &p,
            "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n",
        )
        .expect("fixture");
        let paths = Paths(t.0.clone());
        let ranged = qj(
            &p,
            serde_json::json!({"startLine": 8, "endLine": 9, "block": true}),
        );
        let r = execute_local_fetch(&ranged, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange { start: 8, end: 11 }],
            "{r:?}"
        );
        let matched = qj(
            &p,
            serde_json::json!({"matchString": ["return a", "return 1"], "contextLines": 0, "block": true}),
        );
        let r = execute_local_fetch(&matched, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![
                LineRange { start: 3, end: 5 },
                LineRange { start: 8, end: 11 }
            ],
            "{r:?}"
        );
        assert_eq!(r.matched_lines, vec![5, 10]);
    }

    #[test]
    fn a_match_list_is_a_grep_map_of_every_literal() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(
            &p,
            numbered(12)
                .replace("l4\n", "def a|b\n")
                .replace("l9\n", "x.y\n"),
        )
        .expect("fixture");
        let paths = Paths(t.0.clone());
        // Literal entries are literal: `|` and `.` are not regex syntax.
        let req = qj(
            &p,
            serde_json::json!({"matchString": ["a|b", "x.y", "absent"], "contextLines": 0}),
        );
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.content.as_deref(),
            Some("def a|b\n... [lines 5-8 omitted] ...\nx.y\n")
        );
        assert_eq!(r.matched_lines, vec![4, 9]);
        let one = qj(
            &p,
            serde_json::json!({"matchString": "x.y", "contextLines": 0}),
        );
        assert_eq!(
            execute_local_fetch(&one, &paths, &Safe, &NeverCancel).matched_lines,
            vec![9]
        );
    }

    #[test]
    fn context_lines_over_the_maximum_clamp_with_a_warning() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, numbered(300).replace("l150\n", "hit\n")).expect("fixture");
        let paths = Paths(t.0.clone());
        let req = qj(
            &p,
            serde_json::json!({"matchString": "hit", "contextLines": 120}),
        );
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: 50,
                end: 250
            }]
        );
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("contextLines 120 clamped to 100")),
            "{:?}",
            r.warnings
        );
        // The clamped-off context is one executable read away, each line once.
        let rest = r
            .next
            .and_then(|next| next.read_context)
            .expect("next.readContext");
        let ranges: Vec<&str> = rest
            .query
            .ranges
            .iter()
            .map(|range| range.as_str())
            .collect();
        assert_eq!(ranges, ["30-49", "251-270"], "{rest:?}");
        assert!(rest.query.match_string.is_none() && rest.query.context_lines.is_none());
        let r = execute_local_fetch(&rest.query, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![
                LineRange { start: 30, end: 49 },
                LineRange {
                    start: 251,
                    end: 270
                }
            ],
            "{r:?}"
        );
    }

    #[test]
    fn an_oversized_block_continues_through_the_declaration_end() {
        let t = Temp::new();
        let p = t.0.join("m.py");
        let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
        // def at line 1, 450 body lines (2-451), return at 452.
        fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
        let paths = Paths(t.0.clone());
        let ranged = qj(
            &p,
            serde_json::json!({"startLine": 10, "endLine": 12, "block": true}),
        );
        let r = execute_local_fetch(&ranged, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: 10,
                end: 409
            }]
        );
        let rest = r
            .next
            .and_then(|next| next.continue_block)
            .expect("next.continueBlock");
        assert_eq!(
            (rest.query.start_line(), rest.query.end_line()),
            (Some(410), Some(452)),
            "{rest:?}"
        );
        assert!(
            !rest.query.block(),
            "a block re-read would widen back: {rest:?}"
        );
        let r = execute_local_fetch(&rest.query, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: 410,
                end: 452
            }]
        );
        // A match inside an oversized declaration keeps its context window
        // and offers the whole declaration.
        let matched = qj(
            &p,
            serde_json::json!({"matchString": "x200 = 200", "contextLines": 1, "block": true}),
        );
        let r = execute_local_fetch(&matched, &paths, &Safe, &NeverCancel);
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: 201,
                end: 203
            }]
        );
        let whole = r
            .next
            .and_then(|next| next.read_block)
            .expect("next.readBlock");
        let ranges: Vec<&str> = whole
            .query
            .ranges
            .iter()
            .map(|range| range.as_str())
            .collect();
        assert_eq!(ranges, ["1-200", "204-452"], "{whole:?}");
        assert!(whole.query.match_string.is_none() && !whole.query.block());
    }

    #[test]
    fn a_long_matched_line_marks_both_cuts_and_offers_the_whole_line() {
        let t = Temp::new();
        let p = t.0.join("min.js");
        let line = format!("{}needleFn(){}", "a".repeat(3000), "b".repeat(3000));
        fs::write(&p, format!("head\n{line}\ntail\n")).expect("fixture");
        let paths = Paths(t.0.clone());
        let req = qj(&p, serde_json::json!({"matchString": "needleFn"}));
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        let content = r.content.as_deref().expect("content");
        assert!(
            content.starts_with("... [2800 bytes omitted] ...\n"),
            "{content:.80}"
        );
        assert!(
            content.ends_with("\n... [2802 bytes omitted] ...\n"),
            "{content:.80}"
        );
        let wire = serde_json::to_value(&r).expect("serializable");
        crate::contracts::validate_output(
            "localFetch",
            &serde_json::json!({"results":[{"index":0,"data":wire}]}),
        )
        .expect("a row with next.wholeLines satisfies the output contract");
        let whole = r
            .next
            .and_then(|next| next.whole_lines)
            .expect("next.wholeLines");
        assert_eq!(whole.query.context_lines, Some(0), "{whole:?}");
        let r = execute_local_fetch(&whole.query, &paths, &Safe, &NeverCancel);
        assert_eq!(r.content, Some(format!("{line}\n")), "{:?}", r.warnings);
    }

    #[test]
    fn ecma_lookbehind_uses_the_isolated_worker_when_available() {
        let worker =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/octocode-regex-worker");
        if !worker.exists() {
            return;
        }
        // On macOS a freshly compiled binary triggers a Gatekeeper security scan on first
        // launch that can easily exceed the 1-second deadline. Spawn the worker once with
        // no stdin so macOS completes the scan before the timed engine is created.
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new(&worker)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|mut c| c.wait());
        }
        let t = Temp::new();
        let p = t.0.join("regex.txt");
        fs::write(&p, "needle one\nneedle two\n").expect("regex fixture should be written");
        let mut request = q(&p);
        request.match_string = Some("(?<=needle )one".parse().expect("match string"));
        request.match_string_is_regex = Some(true);
        request.context_lines = Some(0);
        let engine = std::sync::Arc::new(crate::regex::IsolatedRegexEngine::new(
            worker,
            crate::regex::IsolatedRegexLimits::default(),
        ));
        let regex = LocalFetchRegex::new(Some(engine));
        let result = execute_local_fetch_with_regex(
            &request,
            &Paths(t.0.clone()),
            &Safe,
            &NeverCancel,
            &regex,
        );
        assert_eq!(
            result.content.as_deref(),
            Some("needle one\n"),
            "{:?}",
            result.error
        );
    }
}
