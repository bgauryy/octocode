mod executor;
mod extraction;
mod pagination;
mod types;
mod validation;
pub use executor::{execute_local_fetch, execute_local_fetch_with_regex, process_fetched_content};
pub use types::*;
pub use validation::validate_request;

#[cfg(test)]
mod tests {
    use super::*;
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
    fn q(path: &Path) -> LocalFetchRequest {
        LocalFetchRequest {
            path: path.to_string_lossy().into(),
            ..Default::default()
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
        req.limit = Some(1);
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
    fn byte_pages_preserve_utf8_and_use_byte_offsets() {
        let t = Temp::new();
        let p = t.0.join("a.txt");
        fs::write(&p, "a😀b").expect("test fixture operation should succeed");
        let paths = Paths(t.0.clone());
        let mut req = q(&p);
        req.chunk_type = Some(ChunkType::Bytes);
        req.limit = Some(2);
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
        req.match_string = Some("needle".into());
        req.context_lines = Some(0);
        let r = execute_local_fetch(&req, &paths, &Safe, &NeverCancel);
        let expected_hash = format!("{:x}", sha2::Sha256::digest(b"zero\nneedle SECRET\nlast\n"));
        assert_eq!(r.source_sha256.as_deref(), Some(expected_hash.as_str()));
        assert_eq!(r.content.as_deref(), Some("needle [REDACTED]\n"));
        assert_eq!(r.match_ranges, vec![LineRange { start: 2, end: 2 }]);
        assert!(
            r.source_line_ranges.is_empty(),
            "redaction invalidates source mapping"
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
            serde_json::from_value::<LocalFetchRequest>(
                serde_json::json!({"path":"x","madeUp":true})
            )
            .is_err()
        );
        let mut req = q(Path::new("x"));
        req.full_content = Some(true);
        req.match_string = Some("x".into());
        assert!(validate_request(&req).is_err())
    }

    #[test]
    fn malformed_utf8_is_binary_and_long_lines_fall_back_to_bytes() {
        let t = Temp::new();
        let malformed = t.0.join("malformed.txt");
        fs::write(&malformed, [b'a', 0xff, b'b']).expect("malformed fixture should be written");
        let paths = Paths(t.0.clone());
        let malformed_result = execute_local_fetch(&q(&malformed), &paths, &Safe, &NeverCancel);
        assert_eq!(
            malformed_result.error_code.as_deref(),
            Some("binaryFileUnsupported")
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
    fn full_content_limits_return_executable_continuations() {
        let t = Temp::new();
        let paths = Paths(t.0.clone());
        let view_limited = t.0.join("view.txt");
        fs::write(&view_limited, "x".repeat(60_000)).expect("view fixture should be written");
        let mut compact = q(&view_limited);
        compact.full_content = Some(true);
        compact.minify = Some(MinifyMode::Standard);
        let result = execute_local_fetch(&compact, &paths, &Safe, &NeverCancel);
        assert_eq!(result.error_code.as_deref(), Some("fullContentLimit"));
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
        assert_eq!(result.error_code.as_deref(), Some("fileTooLarge"));
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::FullContentSourceSizeLimit]
        );
        assert_eq!(result.metadata_unavailable, vec!["totalLines"]);
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

    #[test]
    fn ecma_lookbehind_uses_the_isolated_worker_when_available() {
        let worker =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/octocode-regex-worker");
        if !worker.exists() {
            return;
        }
        let t = Temp::new();
        let p = t.0.join("regex.txt");
        fs::write(&p, "needle one\nneedle two\n").expect("regex fixture should be written");
        let mut request = q(&p);
        request.match_string = Some("(?<=needle )one".into());
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
