//! Unit tests of the read pipeline's private stages.

#[cfg(test)]
mod timestamp_tests {
    use super::super::system_time_iso;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn preserves_millisecond_precision() {
        assert_eq!(
            system_time_iso(UNIX_EPOCH + Duration::from_millis(1_600_000_000_443)),
            Some("2020-09-13T12:26:40.443Z".into())
        );
    }
}

#[cfg(test)]
mod source_size_tests {
    use super::super::*;
    use crate::execution::cancel::NeverCancel;
    use octocode_contract_types::ErrorCode;
    use std::fs;

    use super::super::super::tests::{Paths, fetch_with, q};
    use crate::security::scan::Passthrough;
    use crate::tools::test_support::Fixture;

    /// Read `req` under `dir` with a passthrough scan.
    fn fetch(req: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
        fetch_with(req, paths, &Passthrough)
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
        let len = super::super::super::large_source::MAX_STREAM_SOURCE_BYTES + 1;
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

    /// A bare read of a source past the whole-file ceiling states the span it
    /// returned (not the window it held) and a line pagination in file lines.
    #[test]
    fn a_streamed_bare_read_states_the_span_it_returned() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("line {lines} payload payload payload payload\n"));
        }
        fs::write(&path, &text).expect("write");
        let first = fetch(&q(&path), &Paths(dir.clone()));
        assert_eq!(first.status, ReadStatus::Success, "{first:?}");
        let returned = first.returned_lines.expect("returned lines");
        assert_eq!(first.end_line, Some(returned), "{first:?}");
        assert!(
            first
                .warnings
                .iter()
                .any(|warning| warning.starts_with("Large source (")),
            "{:?}",
            first.warnings
        );
        let page = first.pagination.as_ref().expect("pagination");
        assert_eq!(page.unit, WindowUnit::Lines, "{page:?}");
        assert_eq!(page.offset, 0, "{page:?}");
        assert_eq!(page.length, returned, "{page:?}");
        assert_eq!(page.total_lines, lines, "{page:?}");
        assert!(page.has_more, "{page:?}");
        assert_eq!(page.next_offset, Some(returned), "{page:?}");
    }

    // Past the whole-file ceiling a plain read streams a bounded line window:
    // exact whole-file totals, a digest-bound continuation that advances,
    // and matchString redirected to localSearch.
    #[test]
    fn oversized_text_source_is_served_as_streamed_line_windows() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("line {lines} payload payload payload payload\n"));
        }
        fs::write(&path, &text).expect("write");
        let query = |q: LocalFetchQuery| fetch(&q, &Paths(dir.clone()));
        let first = query(q(&path));
        assert_eq!(first.status, ReadStatus::Success, "{first:?}");
        assert_eq!(first.total_lines, Some(lines));
        assert_eq!(first.start_line, Some(1));
        assert!(
            first
                .content
                .as_deref()
                .is_some_and(|c| c.starts_with("line 1 "))
        );
        let next = first
            .next
            .as_ref()
            .and_then(|n| n.continue_window.clone())
            .expect("continueWindow");
        assert!(next.query.snapshot.is_some());
        let second = query(next.query);
        assert_eq!(second.status, ReadStatus::Success, "{second:?}");
        assert_eq!(second.start_line, first.end_line.map(|line| line + 1));

        // A wide explicit window pages within itself and advances.
        let wide = query(LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{}-{}", 1, 3_000).parse().expect("range")],
            ..test_local_fetch_query()
        });
        let inner = wide
            .next
            .as_ref()
            .and_then(|n| n.continue_window.clone())
            .expect("inner continue");
        let wide_two = query(inner.query);
        let page_end = |r: &LocalFetchResult| r.source_line_ranges.last().map(|range| range.end);
        let page_start =
            |r: &LocalFetchResult| r.source_line_ranges.first().map(|range| range.start);
        assert_eq!(
            page_start(&wide_two),
            page_end(&wide).map(|line| line + 1),
            "pages advance"
        );

        let tail = query(LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{}-{}", lines, lines).parse().expect("range")],
            ..test_local_fetch_query()
        });
        assert_eq!(tail.start_line, Some(lines), "{tail:?}");
        assert!(
            tail.content
                .as_deref()
                .is_some_and(|c| c.starts_with(&format!("line {lines} ")))
        );
        assert!(tail.next.is_none());

        let matched = query(LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            match_string: Some(MatchString::String("line 7 ".parse().expect("match"))),
            ..test_local_fetch_query()
        });
        assert_eq!(matched.error_code, Some(ErrorCode::LargeSourceWindowOnly));
        // The localSearch is an executable lead, and the tip stays short.
        let search = matched
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("findText lead");
        assert_eq!(search["tool"], "localSearch", "{search}");
        assert_eq!(search["query"]["queries"][0]["matchString"], "line 7 ");
        assert!(
            matched.hints.iter().all(|hint| hint.len() <= 120),
            "{matched:?}"
        );
    }

    // Several `ranges` on an oversized source are each streamed window reads:
    // one response holds every requested line once, with an omission marker
    // between non-adjacent ranges, instead of a rejection whose repair
    // restarts at line 1.
    #[test]
    fn oversized_source_serves_several_ranges_in_one_read() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("line {lines} payload payload payload payload\n"));
        }
        fs::write(&path, &text).expect("write");
        let far = lines - 2;
        let result = fetch(
            &LocalFetchQuery {
                path: path.to_string_lossy().parse().expect("path"),
                ranges: vec![
                    format!("{far}-{}", far + 1).parse().expect("range"),
                    "1-2".parse().expect("range"),
                    "2-3".parse().expect("range"),
                ],
                ..test_local_fetch_query()
            },
            &Paths(dir.clone()),
        );
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        assert_eq!(result.total_lines, Some(lines));
        assert_eq!(
            result.content.as_deref(),
            Some(
                format!(
                    "line 1 payload payload payload payload\nline 2 payload payload payload payload\nline 3 payload payload payload payload\n... [lines 4-{} not requested] ...\nline {far} payload payload payload payload\nline {} payload payload payload payload\n",
                    far - 1,
                    far + 1
                )
                .as_str()
            )
        );
        let spans: Vec<(usize, usize)> = result
            .source_line_ranges
            .iter()
            .map(|range| (range.start, range.end))
            .collect();
        assert_eq!(spans, [(1, 3), (far, far + 1)]);
        assert!(result.next.is_none(), "{result:?}");
        assert_eq!(result.error_code, None);
    }

    /// A streamed range pages in its own lines: every page continues where
    /// the last one ended (no stub page at a window boundary), the page
    /// offset counts from the range's start without resetting, every line
    /// is reached once, and the streamed note is stated by the first page.
    #[test]
    fn a_streamed_range_pages_evenly_across_window_boundaries() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("{:08} payload payload payload payload\n", lines));
        }
        fs::write(&path, &text).expect("write");
        let (start, end) = (100usize, 6_000usize);
        let mut query = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{start}-{end}").parse().expect("range")],
            ..test_local_fetch_query()
        };
        let mut next_line = start;
        let mut sizes = Vec::new();
        for page_index in 0.. {
            let page = fetch(&query, &Paths(dir.clone()));
            assert_eq!(page.status, ReadStatus::Success, "{page:?}");
            let pagination = page.pagination.as_ref().expect("pagination");
            assert_eq!(pagination.offset, next_line - start, "page {page_index}");
            assert_eq!(pagination.total_lines, end + 1 - start, "page {page_index}");
            assert_eq!(
                page.warnings.iter().any(|w| w.starts_with("Large source")),
                page_index == 0,
                "page {page_index}: {:?}",
                page.warnings
            );
            let rows = page.content.as_deref().unwrap_or_default().lines().count();
            for row in page.content.as_deref().unwrap_or_default().lines() {
                assert_eq!(
                    row,
                    format!("{next_line:08} payload payload payload payload")
                );
                next_line += 1;
            }
            sizes.push(rows);
            match page.next.and_then(|next| next.continue_window) {
                Some(read) => query = read.query,
                None => break,
            }
        }
        assert_eq!(next_line, end + 1, "every line once");
        // Equal lines: every page but the last is a full page.
        let full = sizes[0];
        assert!(sizes.len() > 2, "{sizes:?}");
        assert!(
            sizes[..sizes.len() - 1].iter().all(|size| *size == full),
            "{sizes:?}"
        );
    }

    // A line chunk of an oversized source larger than one response page
    // pages inside the chunk and then goes on past it: every line from the
    // first is reached once, in order, across the chunk boundary.
    #[test]
    fn oversized_source_line_chunks_continue_past_a_paged_chunk() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("line {lines} payload payload payload payload\n"));
        }
        fs::write(&path, &text).expect("write");
        let mut query = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            unit: Some(WindowUnit::Lines),
            length: super::super::super::types::wire_positive(1_000),
            ..test_local_fetch_query()
        };
        let mut next_line = 1usize;
        for _ in 0..12 {
            let page = fetch(&query, &Paths(dir.clone()));
            assert_eq!(page.status, ReadStatus::Success, "{page:?}");
            for row in page.content.as_deref().unwrap_or_default().lines() {
                assert_eq!(
                    row,
                    format!("line {next_line} payload payload payload payload")
                );
                next_line += 1;
            }
            query = page
                .next
                .as_ref()
                .and_then(|next| next.continue_window.clone())
                .unwrap_or_else(|| panic!("no continuation after line {}", next_line - 1))
                .query;
        }
        assert!(next_line > 2_001, "walked past two chunks: {next_line}");
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

    // A bounded read of an interior body line of a private key must not
    // leak the key, even when the file is NOT key-named and the selected window
    // contains no BEGIN/END marker (so the anchored full-block patterns cannot
    // fire). The `Passthrough` scan, proving the full-file block guard
    // — not the window sanitizer — closes the leak.
    #[test]
    fn interior_private_key_window_does_not_leak_in_non_key_named_file() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("notes.txt"); // deliberately NOT a *.pem/id_rsa path
        let body = "MIIEpQIBAAKCAQEAsplitKeyBodyLineOneAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let body2 = "c3BsaXRLZXlCb2R5TGluZVR3b0JBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC";
        let file = format!(
            "fn main() {{}}\nlet config = load();\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body2}\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
        );
        fs::write(&path, &file).expect("write file");

        // Select only the interior body lines (4..=5) — no BEGIN/END in view.
        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{}-{}", 4, 5).parse().expect("range")],
            ..test_local_fetch_query()
        };
        let result = fetch(&req, &Paths(dir.clone()));

        let content = result.content.clone().unwrap_or_default();
        assert!(
            !content.contains(body) && !content.contains(body2),
            "private key body leaked from an interior window: {content}"
        );
    }

    // matchString must run on redacted text: a line-level secret that the
    // window sanitizer redacts must not be confirmable by probing matchString
    // (an exact-prefix oracle). Matching the true secret and a wrong guess must
    // be indistinguishable, and line numbers must stay source-accurate.
    #[test]
    fn match_string_cannot_probe_line_level_secrets() {
        use crate::security::ContentSecurity;
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("config.txt");
        let secret = "AKIAIOSFODNN7EXAMPLE";
        fs::write(
            &path,
            format!("header\naws_access_key_id = {secret}\nneedle after\n"),
        )
        .expect("write file");
        let security = ContentSecurity::new();
        let probe = |needle: &str| {
            let req = LocalFetchQuery {
                path: path.to_string_lossy().parse().expect("path"),
                match_string: Some(MatchString::String(needle.parse().expect("match string"))),
                context_lines: Some(0),
                ..test_local_fetch_query()
            };
            execute_local_fetch(
                &req,
                &Paths(dir.clone()),
                &security,
                &NeverCancel,
                None,
                None,
            )
        };
        let right = probe("AKIAIOSFODNN7EXAMPL");
        let wrong = probe("AKIAIOSFODNN7EXAMPQ");
        assert_eq!(right.selected_match_count, wrong.selected_match_count);
        assert_eq!(right.error_code, wrong.error_code);
        assert_eq!(right.selected_match_count, Some(0), "{right:?}");
        // Same hints, and the same leads but for the guess they repeat,
        // either way (no oracle); the hints explain the miss.
        assert_eq!(right.hints, wrong.hints);
        assert_eq!(
            serde_json::to_string(&right.next)
                .expect("json")
                .replace("AKIAIOSFODNN7EXAMPL", "AKIAIOSFODNN7EXAMPQ"),
            serde_json::to_string(&wrong.next).expect("json")
        );
        assert!(
            right.hints.iter().any(|hint| hint == REDACTED_MATCH_HINT),
            "{right:?}"
        );
        let clean = dir.join("clean.txt");
        fs::write(&clean, "nothing secret\n").expect("write clean");
        let miss = execute_local_fetch(
            &LocalFetchQuery {
                path: clean.to_string_lossy().parse().expect("path"),
                match_string: Some(MatchString::String("absent".parse().expect("match string"))),
                ..test_local_fetch_query()
            },
            &Paths(dir.clone()),
            &security,
            &NeverCancel,
            None,
            None,
        );
        assert!(
            !miss.hints.iter().any(|hint| hint == REDACTED_MATCH_HINT),
            "{miss:?}"
        );
        let after = probe("needle");
        assert_eq!(after.match_ranges, vec![LineRange { start: 3, end: 3 }]);
        assert_eq!(after.total_lines, Some(3));
    }

    // Over-redaction guard: an ordinary long base64 line (config blob,
    // hash, minified asset) with NO private-key markers anywhere in the file
    // must pass through byte-identical — the guard keys off BEGIN/END markers,
    // never bare base64.
    #[test]
    fn innocent_base64_window_is_returned_byte_identical() {
        let temp = Fixture::new();
        let dir = temp.0.clone();
        let path = dir.join("data.txt");
        let blob = "aGVsbG8gd29ybGQgdGhpcyBpcyBqdXN0IGEgbG9uZyBiYXNlNjQgYmxvYg==";
        let file = format!("header\n{blob}\nfooter\n");
        fs::write(&path, &file).expect("write file");

        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{}-{}", 2, 2).parse().expect("range")],
            ..test_local_fetch_query()
        };
        let result = fetch(&req, &Paths(dir.clone()));

        let content = result.content.clone().unwrap_or_default();
        assert!(
            content.contains(blob),
            "innocent base64 was wrongly redacted: {content}"
        );
    }
}

#[cfg(test)]
mod line_page_scan_tests {
    use super::super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::security::ContentSecurity;
    use octocode_contract_types::ErrorCode;
    use std::path::Path;

    fn security() -> ContentSecurity {
        ContentSecurity::new()
    }
    fn fetch(source: &str, q: &LocalFetchQuery) -> LocalFetchResult {
        process_fetched_content(
            q,
            source.as_bytes(),
            Path::new("/fixture/app.ts"),
            &SourceFacts::default(),
            &security(),
            &NeverCancel,
        )
    }
    fn lines_query(offset: usize, chunk: usize) -> LocalFetchQuery {
        LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            unit: Some(WindowUnit::Lines),
            offset: Some(wire_count(offset)),
            length: wire_positive(chunk),
            ..test_local_fetch_query()
        }
    }
    fn filler(n: usize) -> String {
        (1..=n)
            .map(|i| format!("const filler_{i} = {i};\n"))
            .collect()
    }

    #[test]
    fn fetch_partition_binding_rejects_same_count_legacy_replay() {
        use crate::execution::SelectionLimits;
        use crate::tools::selection::{Candidate, Decision, JudgeCall, Runtime, SelectionJudge};
        use octocode_contract_types::SelectionUnit;
        use serde_json::json;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::time::{Duration, Instant};

        struct FirstCandidate(Arc<AtomicUsize>);
        impl SelectionJudge for FirstCandidate {
            fn identity(&self) -> serde_json::Value {
                json!({"test":"fetch-partition"})
            }
            fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(call
                    .admitted
                    .iter()
                    .map(|&ordinal| {
                        (
                            ordinal,
                            if ordinal == 0 {
                                Decision::Selected
                            } else {
                                Decision::Excluded
                            },
                        )
                    })
                    .collect())
            }
        }

        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = Runtime {
            limits: SelectionLimits {
                max_candidates: 64,
                max_chars: 20_000,
                timeout: Duration::from_secs(5),
            },
            available: true,
            judge: Arc::new(FirstCandidate(calls.clone())),
            handle: executor.handle().clone(),
            context: crate::execution::ExecutionContext {
                cancellation: tokio_util::sync::CancellationToken::new(),
                deadline: Instant::now() + Duration::from_secs(5),
                walk_threads: None,
                response_window: None,
                github_credential: None,
            },
        };
        let first = format!("{}\n", "a".repeat(999));
        let second = format!("{}\n", "b".repeat(999));
        let third = format!("{}\n", "c".repeat(2_047));
        let source = (1..=20)
            .map(|line| match line {
                1 => first.as_str(),
                10 => second.as_str(),
                20 => third.as_str(),
                _ => "gap\n",
            })
            .collect::<String>();
        let query: LocalFetchQuery = serde_json::from_value(json!({
            "path":"/fixture/source.txt", "ranges":["1-1","10-10","20-20"], "clasifyContext":{"classifiers":["Find the first evidence"]}
        }))
        .expect("query");
        let path = Path::new("/fixture/source.txt");
        let sha256 = octocode_engine::digest::sha256(source.as_bytes());
        let input = SelectionInput {
            q: &query,
            source_path: path,
            provider_path: "/fixture/source.txt",
            sha256: &sha256,
            runtime: Some(&runtime),
            query_json: std::cell::OnceCell::new(),
        };
        let extracted = extract(&query, &source).unwrap_or_else(|_| panic!("source extraction"));
        let (current, _) = super::super::super::selection::chunks(
            &extracted.text,
            extracted.source_lines.as_deref(),
            Some(&source),
            path,
            input.provider_path,
            &security(),
            Some(&runtime),
            None,
            None,
        );
        let legacy = [
            Candidate::new(Some(
                json!({"content":format!("{first}{}{second}", super::super::super::extraction::omission_marker(2, 9))}),
            )),
            Candidate::new(Some(
                json!({"content":super::super::super::extraction::omission_marker(11, 19)}),
            )),
            Candidate::new(Some(json!({"content":third}))),
        ];
        assert_eq!(current.len(), legacy.len());
        assert_eq!(current[0].state.as_ref().unwrap()["content"], first);
        assert!(
            legacy[0].state.as_ref().unwrap()["content"]
                .as_str()
                .unwrap()
                .contains(&second)
        );
        let query_json = serde_json::to_value(&query).expect("query JSON");
        let select = |source: &str, prior: Option<&str>, candidates: &[Candidate]| {
            crate::tools::selection::select(
                Some(&runtime),
                &query_json,
                source,
                prior,
                candidates,
                None,
                SelectionUnit::Chunks,
            )
        };
        let old = select(&format!("{}:{sha256}", path.display()), None, &legacy)
            .unwrap_or_else(|_| panic!("legacy plan"));
        let replay = (0..current.len())
            .map(|_| Candidate::new(None))
            .collect::<Vec<_>>();
        assert!(matches!(
            select(&input.source_key(), Some(&old.token), &replay),
            Err(crate::tools::selection::Failure::Stale)
        ));
        let fresh =
            select(&input.source_key(), None, &current).unwrap_or_else(|_| panic!("fresh plan"));
        let resumed = select(&input.source_key(), Some(&fresh.token), &replay)
            .unwrap_or_else(|_| panic!("fresh replay"));
        assert_eq!(resumed.selected, [0]);
        assert_eq!(resumed.token, fresh.token);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn selected_reads_fill_the_configured_window_and_keep_explicit_lengths() {
        let source = filler(1_200);
        let mut query = LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            ranges: vec!["1-1200".parse().expect("range")],
            ..test_local_fetch_query()
        };
        let read = |query: &LocalFetchQuery| {
            process_fetched_content(
                query,
                source.as_bytes(),
                Path::new("/fixture/app.ts"),
                &SourceFacts {
                    window: Some(50_000),
                    ..SourceFacts::default()
                },
                &security(),
                &NeverCancel,
            )
        };
        let complete = read(&query);
        assert_eq!(complete.status, ReadStatus::Success);
        assert!(
            complete
                .content
                .as_deref()
                .expect("content")
                .contains("filler_1200")
        );
        assert!(!complete.pagination.as_ref().expect("pagination").has_more);

        query.unit = Some(WindowUnit::Lines);
        query.length = wire_positive(17);
        let explicit = read(&query);
        assert_eq!(explicit.pagination.as_ref().expect("pagination").length, 17);
        assert!(explicit.pagination.as_ref().expect("pagination").has_more);
    }

    /// A read with no anchor whose view would not fit one page returns a
    /// short head instead of the page: the read is not targeted yet. Paging
    /// on, whole-file reads and anchored reads are unchanged; a file that
    /// fits one page is returned whole, with no note.
    #[test]
    fn an_unanchored_read_over_one_page_returns_a_head() {
        // 1,000 short lines: far below any line-count ceiling, over 16 KiB.
        let source = filler(1_000);
        assert!(source.len() > 16 * 1024);
        let plain = LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            ..test_local_fetch_query()
        };
        let head = fetch(&source, &plain);
        assert_eq!(head.returned_lines, Some(HEAD_LINES), "{head:?}");
        // The head's facts are data (totalLines, continueWindow), not a note.
        assert!(head.warnings.is_empty(), "{:?}", head.warnings);
        assert_eq!(head.total_lines, Some(1_000), "{head:?}");
        let next = head
            .next
            .and_then(|next| next.continue_window)
            .expect("continueWindow");
        assert_eq!(local_fetch_query::offset(&next.query), Some(HEAD_LINES));
        assert_eq!(local_fetch_query::window_length(&next.query), None);
        let page = fetch(&source, &next.query);
        assert!(page.returned_lines.expect("lines") > HEAD_LINES, "{page:?}");
        // Whole-file, anchored and symbol reads are unchanged.
        let whole = fetch(
            &source,
            &LocalFetchQuery {
                full_content: Some(true),
                ..plain.clone()
            },
        );
        assert!(whole.returned_lines.expect("lines") > HEAD_LINES);
        let ranged = fetch(
            &source,
            &LocalFetchQuery {
                ranges: vec![format!("{}-{}", 10, 200).parse().expect("range")],
                ..plain.clone()
            },
        );
        assert_eq!(ranged.returned_lines, Some(191));
        // A standard view is guarded the same way (ghGetFileContent's default).
        let standard = fetch(
            &source,
            &LocalFetchQuery {
                minify: Some(MinifyMode::Standard),
                ..plain.clone()
            },
        );
        assert_eq!(standard.returned_lines, Some(HEAD_LINES), "{standard:?}");
        // A file that fits one page is read whole, with no note.
        let small = fetch(&filler(500), &plain);
        assert_eq!(small.returned_lines, Some(500), "{small:?}");
        assert!(small.warnings.is_empty(), "{:?}", small.warnings);
        assert!(small.next.is_none(), "{small:?}");
    }

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
            scanned < 8_000 + 4 * super::super::super::pagination::PAGE_SCAN_MARGIN_BYTES,
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
}

#[cfg(all(test, unix))]
mod open_tests {
    use super::super::*;
    use octocode_contract_types::ErrorCode;

    /// A FIFO is opened without blocking and refused by the handle's own
    /// stat, so a path swapped for one after the policy check cannot hang
    /// the read; a regular file resolves with the opened handle.
    #[test]
    fn a_fifo_is_refused_without_blocking_and_a_file_opens() {
        let root = tempfile::tempdir().expect("root");
        let fifo = root.path().join("pipe.txt");
        crate::tools::test_support::make_fifo(&fifo);
        std::fs::write(root.path().join("a.txt"), "x\n").expect("file");
        let (paths, _) = crate::tools::test_support::simple_policy(root.path());
        let query = |path: &std::path::Path| LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ..test_local_fetch_query()
        };
        let refused = resolve_file(&query(&fifo), &paths).expect_err("a FIFO is no file");
        assert_eq!(refused.error_code, Some(ErrorCode::NotAFile));
        let (_, _, meta) =
            resolve_file(&query(&root.path().join("a.txt")), &paths).expect("regular file");
        assert_eq!(meta.len(), 2);
    }

    /// A FIFO is `notAFile`, named by the requested path, with nothing to
    /// retry: no lead and no "retry the exact path" tip.
    #[test]
    fn a_fifo_is_not_a_file_named_by_its_path_with_nothing_to_retry() {
        let root = tempfile::tempdir().expect("root");
        let fifo = root.path().join("pipe1");
        crate::tools::test_support::make_fifo(&fifo);
        let paths = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let requested = fifo.to_string_lossy().into_owned();
        let result = execute_local_fetch(
            &LocalFetchQuery {
                path: requested.parse().expect("path"),
                ..test_local_fetch_query()
            },
            &paths,
            &crate::security::ContentSecurity::new(),
            &crate::execution::cancel::NeverCancel,
            None,
            None,
        );
        assert_eq!(result.error_code, Some(ErrorCode::NotAFile), "{result:?}");
        let message = result.error.as_deref().unwrap_or_default();
        assert!(message.starts_with(&requested), "{message}");
        assert!(
            result.next.is_none() && result.hints.is_empty(),
            "{result:?}"
        );
        use crate::response::output::ToolOutput;
        assert_eq!(
            super::super::super::output::Output.error_hint(ErrorCode::NotAFile),
            Some(crate::response::output::NO_HINT)
        );
    }
}
