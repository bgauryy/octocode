use super::extraction::{extract, line_count};
use super::pagination::{continuation, page, result_counts};
use super::types::*;
use super::validation::{is_binary, validate_request};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;

/// Hard ceiling on source bytes read into memory for ANY localFetch path.
/// Plain, matchString, and line-range reads all slurp the whole source file
/// before extraction, so without this cap a single pathologically large file
/// would be read (and secret-scanned) entirely into memory. This is a
/// memory-safety bound distinct from — and larger than — the 100KB full-content
/// *return* cap below: files under this ceiling still page normally via
/// next.continue; files over it are refused outright with `fileTooLarge`.
const MAX_SOURCE_BYTES: u64 = 10 * 1024 * 1024;

/// Build the shared `fileTooLarge` result for a source that exceeds
/// [`MAX_SOURCE_BYTES`]. Terminal: there is no bounded continuation past the
/// hard ceiling.
fn source_too_large(path: &str, len: u64) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        path.to_owned(),
        "fileTooLarge",
        format!(
            "File too large: {}KB (hard limit: {}KB). This source exceeds the maximum size localFetch will read into memory. Use astSearch or localSearch to locate the relevant symbol, then read a bounded startLine/endLine range of a smaller source.",
            len / 1024,
            MAX_SOURCE_BYTES / 1024
        ),
    );
    result.resolved_path = Some(path.to_owned());
    result.source_bytes = Some(len as usize);
    result.is_partial = Some(true);
    result.terminal_limit = Some(true);
    result.partial_reasons = vec![PartialReason::FullContentSourceSizeLimit];
    result
}
pub fn execute_local_fetch(
    q: &LocalFetchRequest,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    execute_local_fetch_with_regex(q, paths, security, cancel, &LocalFetchRegex::default())
}
pub fn execute_local_fetch_with_regex(
    q: &LocalFetchRequest,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if let Err(e) = validate_request(q) {
        return LocalFetchResult::error(q.path.clone(), "invalidQuery", e);
    }
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.clone(), "cancelled", e);
    }
    let validated = match paths.validate_read(q.path.as_ref()) {
        Ok(path) => path,
        Err(failure) => {
            let display = failure.safe_path.as_deref().unwrap_or(&q.path);
            let message = if failure.resource_missing {
                format!(
                    "File not found: {display}. Verify the path with astSearch operation:\"files\"."
                )
            } else {
                failure.message
            };
            let mut result = LocalFetchResult::error(q.path.clone(), "fileAccessFailed", message);
            result.resource_missing = failure.resource_missing;
            result.resolved_path = Some(q.path.clone());
            return result;
        }
    };
    let path = validated.canonical;
    let display_path = validated.display;
    let meta = match fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => {
            return LocalFetchResult::error(
                q.path.clone(),
                "fileAccessFailed",
                "Path is not a regular file".into(),
            );
        }
        Err(e) => {
            return LocalFetchResult::error(q.path.clone(), "fileAccessFailed", e.to_string());
        }
    };
    // Enforce the hard source-size ceiling on every read path (default,
    // matchString, and line-range) before opening the file, so an oversized
    // source is never read into memory.
    if meta.len() > MAX_SOURCE_BYTES {
        return source_too_large(&q.path, meta.len());
    }
    let mut sample = [0_u8; 8192];
    let sample_len = match fs::File::open(&path).and_then(|mut file| file.read(&mut sample)) {
        Ok(length) => length,
        Err(error) => {
            return LocalFetchResult::error(q.path.clone(), "fileReadFailed", error.to_string());
        }
    };
    if is_binary(&sample[..sample_len]) {
        let mut result = LocalFetchResult::error(
            q.path.clone(),
            "binaryFileUnsupported",
            format!(
                "Binary file unsupported: {display_path}. Read a text source file, or use astSearch operation:\"files\" for file metadata."
            ),
        );
        result.resolved_path = Some(q.path.clone());
        return result;
    }
    if q.full_content == Some(true)
        && q.minify.unwrap_or_default() == MinifyMode::None
        && q.match_string.is_none()
        && q.start_line.is_none()
        && meta.len() > 100 * 1024
    {
        let mut result = LocalFetchResult::error(
            q.path.clone(),
            "fileTooLarge",
            format!(
                "File too large: {}KB (limit: 100KB). Follow next.continue to retrieve the complete file in bounded chunks, or select startLine/endLine or matchString.",
                meta.len() / 1024
            ),
        );
        result.resolved_path = Some(q.path.clone());
        result.source_bytes = Some(meta.len() as usize);
        result.is_partial = Some(true);
        result.partial_reasons = vec![PartialReason::FullContentSourceSizeLimit];
        result.metadata_unavailable = vec!["totalLines".into()];
        result.next = Some(bounded_continuation(q));
        return result;
    }
    // Read at most the cap (+1 sentinel byte). Re-check the length in case the
    // file grew past the ceiling between the stat above and this read (TOCTOU).
    let bytes = match fs::File::open(&path).and_then(|file| {
        let mut buf = Vec::new();
        file.take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut buf)
            .map(|_| buf)
    }) {
        Ok(b) if b.len() as u64 > MAX_SOURCE_BYTES => {
            return source_too_large(&q.path, b.len() as u64);
        }
        Ok(b) => b,
        Err(e) => return LocalFetchResult::error(q.path.clone(), "fileReadFailed", e.to_string()),
    };
    process_fetched_content(
        q,
        &bytes,
        &path,
        meta.modified().ok().and_then(system_time_iso),
        security,
        cancel,
        regex,
    )
}

/// Shared post-acquisition content processing. Performs no filesystem access;
/// callers own source authorization, binary/transport limits and provenance.
pub fn process_fetched_content(
    q: &LocalFetchRequest,
    bytes: &[u8],
    source_path: &std::path::Path,
    modified: Option<String>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if let Err(error) = validate_request(q) {
        return LocalFetchResult::error(q.path.clone(), "invalidQuery", error);
    }
    let source_sha256 = hex::encode(Sha256::digest(bytes));
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.clone(), "cancelled", e);
    }
    // Node's UTF-8 decoder replaces malformed sequences; binary detection is a
    // separate heuristic and must not turn an otherwise textual file into an error.
    let raw = String::from_utf8_lossy(bytes).into_owned();
    let source_chars = raw.encode_utf16().count();
    let source_bytes = raw.len();
    let total_lines = line_count(&raw);
    let mut warnings = vec![];
    // SEC-1: redact whole private-key blocks across the full file BEFORE any
    // window/extraction, so a bounded read of an interior body line cannot leak a
    // key whose BEGIN/END markers fall outside the selected window (the anchored
    // built-in patterns only match a complete block in a single view).
    let (raw, key_blocks_redacted) = security.redact_key_blocks(&raw);
    if key_blocks_redacted {
        warnings.push(
            "Redacted private-key block(s) found in the source before selecting the window.".into(),
        );
    }
    // matchString runs on redacted text: sanitize the whole source (line-count
    // preserving) before matching so a probe cannot confirm a secret that the
    // returned window would redact (exact-prefix oracle).
    let (raw, match_redacted) = if q.match_string.is_some() {
        match redact_source_lines(&raw, source_path, security) {
            Ok(value) => value,
            Err((code, message)) => return LocalFetchResult::error(q.path.clone(), &code, message),
        }
    } else {
        (raw, false)
    };
    let mode = q.minify.unwrap_or_default();
    let match_blocks = q.match_string.is_some() && mode != MinifyMode::None;
    let applied = if match_blocks { MinifyMode::None } else { mode };
    let ext = match extract(q, &raw, regex) {
        Ok(x) => x,
        Err(e) => {
            return LocalFetchResult::error(
                q.path.clone(),
                if e.starts_with("Invalid regex") || e.starts_with("Regex execution unavailable") {
                    "toolExecutionFailed"
                } else {
                    "noMatches"
                },
                e,
            );
        }
    };
    if ext.count == Some(0) {
        return LocalFetchResult {
            path: q.path.clone(),
            status: "empty".into(),
            resource_missing: false,
            source_sha256: Some(source_sha256.clone()),
            content: Some(String::new()),
            content_view: Some(MinifyMode::None),
            minify_fallback: None,
            error_code: Some("noMatches".into()),
            error: None,
            resolved_path: None,
            warnings: vec![],
            hints: vec!["Verify path/range, or remove matchString.".into()],
            total_lines: Some(total_lines),
            start_line: None,
            end_line: None,
            source_line_ranges: vec![],
            match_ranges: vec![],
            matched_lines: vec![],
            selected_match_count: Some(0),
            modified: None,
            source_chars: Some(source_chars),
            source_bytes: Some(source_bytes),
            returned_chars: Some(0),
            returned_bytes: Some(0),
            returned_lines: Some(0),
            pagination: None,
            is_partial: None,
            partial_reasons: vec![],
            terminal_limit: None,
            metadata_unavailable: vec![],
            next: None,
        };
    }
    let mut selected = ext.text;
    let mut content_view = applied;
    let mut minify_fallback = match_blocks.then(|| MinifyFallback {
        requested: mode,
        applied: MinifyMode::None,
        reason: "match-evidence".into(),
    });
    if applied == MinifyMode::Standard {
        selected = octocode_engine::portable::apply_content_view_minification(&selected, &q.path)
    } else if applied == MinifyMode::Symbols {
        if let Some(s) = octocode_engine::portable::extract_signatures(&selected, &q.path) {
            selected = octocode_engine::portable::apply_content_view_minification(&s, &q.path)
        } else if let Some(outline) = crate::content::markdown_heading_outline(&selected, &q.path) {
            selected = outline
        } else {
            warnings.push(format!("No smaller outline is available for {}; using the standard content view. The outline may be unsupported, oversized, or the source may be minified/bundled (single giant lines) — read specific line ranges instead.",q.path));
            selected =
                octocode_engine::portable::apply_content_view_minification(&selected, &q.path);
            content_view = MinifyMode::Standard;
            minify_fallback = Some(MinifyFallback {
                requested: MinifyMode::Symbols,
                applied: MinifyMode::Standard,
                reason: "outline-unavailable".into(),
            })
        }
    }
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.clone(), "cancelled", e);
    }
    let (safe, security_warnings) = match security.sanitize(&selected, source_path) {
        Ok(x) => x,
        Err((c, _)) if c == "contentSecurityLimit" => {
            let mut result = LocalFetchResult::error(
                q.path.clone(),
                &c,
                "The selected content view exceeds the secret scanner size limit. Byte windows cannot safely split unscanned content. Select a smaller source-line range.".into(),
            );
            result.path = q.path.clone();
            result.total_lines = Some(total_lines);
            result.source_chars = Some(source_chars);
            result.source_bytes = Some(source_bytes);
            result.is_partial = Some(true);
            result.terminal_limit = Some(true);
            result.partial_reasons = vec![PartialReason::SecuritySelectedViewSizeLimit];
            if total_lines > 1 && (q.start_line.is_none() || q.start_line != q.end_line) {
                let line = ext.start.or(q.start_line).unwrap_or(1);
                let query = LocalFetchRequest {
                    path: q.path.clone(),
                    start_line: Some(line),
                    end_line: Some(line),
                    minify: Some(MinifyMode::None),
                    ..Default::default()
                };
                result.next = Some(NextCalls {
                    r#continue: None,
                    read_bounded_lines: Some(Continuation {
                        tool: "localFetch".into(),
                        query,
                        confidence: "exact".into(),
                        reason: Some("The selected view is too large to scan safely. Read one source line; this starts a different source-line view, not a…".into()),
                    }),
                });
            }
            return result;
        }
        Err((c, m)) => return LocalFetchResult::error(q.path.clone(), &c, m),
    };
    warnings.extend(ext.warnings);
    warnings.extend(security_warnings);
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.clone(), "cancelled", e);
    }
    if q.full_content == Some(true) && safe.len() > 50000 {
        let mut result = LocalFetchResult::error(
            q.path.clone(),
            "fullContentLimit",
            "The complete view exceeds 50000 bytes. Follow next.continue to read the same view in bounded chunks.".into(),
        );
        result.path = q.path.clone();
        result.total_lines = Some(total_lines);
        result.source_chars = Some(source_chars);
        result.source_bytes = Some(source_bytes);
        result.is_partial = Some(true);
        result.partial_reasons = vec![PartialReason::FullContentLimit];
        result.next = Some(bounded_continuation(q));
        return result;
    }
    let pg = match page(&safe, q) {
        Ok(p) => p,
        Err(e) => return LocalFetchResult::error(q.path.clone(), "invalidPagination", e),
    };
    let (chars, ret_bytes, ret_lines) = result_counts(&pg.text);
    let next = continuation(q, &pg.pagination);
    let source_ranges = if !safe.is_empty()
        && !match_redacted
        && content_view == MinifyMode::None
        && safe == selected
    {
        if let Some(lines) = ext.source_lines.as_ref() {
            let page_lines =
                &lines[pg.view_lines.0.saturating_sub(1)..pg.view_lines.1.min(lines.len())];
            compress_ranges(page_lines)
        } else {
            vec![LineRange {
                start: pg.view_lines.0,
                end: pg.view_lines.1,
            }]
        }
    } else {
        vec![]
    };
    let matched_lines = ext
        .matched_lines
        .into_iter()
        .filter(|n| source_ranges.iter().any(|r| *n >= r.start && *n <= r.end))
        .collect();
    LocalFetchResult {
        path: q.path.clone(),
        status: "success".into(),
        resource_missing: false,
        source_sha256: Some(source_sha256),
        content: Some(pg.text),
        content_view: Some(content_view),
        minify_fallback,
        error_code: None,
        error: None,
        resolved_path: None,
        warnings,
        hints: vec![],
        total_lines: Some(total_lines),
        start_line: ext.start,
        end_line: ext.end,
        source_line_ranges: source_ranges,
        match_ranges: ext.match_ranges,
        matched_lines,
        selected_match_count: ext.count,
        modified: (content_view != MinifyMode::Symbols)
            .then_some(modified)
            .flatten(),
        source_chars: Some(source_chars),
        source_bytes: Some(source_bytes),
        returned_chars: Some(chars),
        returned_bytes: Some(ret_bytes),
        returned_lines: Some(ret_lines),
        pagination: Some(pg.pagination.clone()),
        is_partial: (next.is_some()).then_some(true),
        partial_reasons: vec![],
        terminal_limit: None,
        metadata_unavailable: vec![],
        next,
    }
}
/// Sanitize the full source while preserving its line structure so match line
/// numbers stay source-accurate. Falls back to per-line sanitization when a
/// whole-source redaction would change the line count (multi-line pattern) or
/// the whole source exceeds the scanner size limit.
fn redact_source_lines(
    raw: &str,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> Result<(String, bool), (String, String)> {
    if let Ok((whole, _)) = security.sanitize(raw, source_path) {
        if whole == raw {
            return Ok((whole, false));
        }
        if line_count(&whole) == line_count(raw) {
            return Ok((whole, true));
        }
    }
    let mut out = String::with_capacity(raw.len());
    for segment in raw.split_inclusive('\n') {
        let (body, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |body| (body, "\n"));
        let (clean, _) = security.sanitize(body, source_path)?;
        out.push_str(&clean.replace('\n', " "));
        out.push_str(newline);
    }
    let changed = out != raw;
    Ok((out, changed))
}
fn bounded_continuation(q: &LocalFetchRequest) -> NextCalls {
    let mut query = q.clone();
    query.full_content = None;
    query.chunk_type = Some(ChunkType::Lines);
    query.offset = Some(0);
    query.chunk_size = Some(100);
    NextCalls {
        r#continue: Some(Continuation {
            tool: "localFetch".into(),
            query,
            confidence: "exact".into(),
            reason: Some("Continue to the next page of results.".into()),
        }),
        read_bounded_lines: None,
    }
}
pub(crate) fn compress_ranges(lines: &[usize]) -> Vec<LineRange> {
    let mut out: Vec<LineRange> = vec![];
    for &n in lines {
        if let Some(x) = out.last_mut().filter(|x| n == x.end + 1) {
            x.end = n
        } else {
            out.push(LineRange { start: n, end: n })
        }
    }
    out
}
fn system_time_iso(t: std::time::SystemTime) -> Option<String> {
    let elapsed = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    let secs = elapsed.as_secs() as i64;
    let millis = elapsed.subsec_millis();
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    ))
}

#[cfg(test)]
mod timestamp_tests {
    use super::system_time_iso;
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
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Paths(PathBuf);
    impl PathAccess for Paths {
        fn validate_read(&self, p: &Path) -> Result<ValidatedRead, PathFailure> {
            let joined = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.0.join(p)
            };
            joined
                .canonicalize()
                .map(|canonical| ValidatedRead {
                    canonical,
                    display: joined.to_string_lossy().into_owned(),
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
            Ok((text.to_owned(), vec![]))
        }
    }

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);
    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "local-fetch-size-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn oversized_plain_read_returns_file_too_large() {
        let dir = temp_dir();
        let path = dir.join("huge.txt");
        // Sparse file just past the hard ceiling — the size guard fires before
        // any bytes are read, so this stays cheap.
        let file = fs::File::create(&path).expect("create file");
        file.set_len(MAX_SOURCE_BYTES + 1).expect("grow file");
        drop(file);

        let req = LocalFetchRequest {
            path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        assert_eq!(result.status, "error");
        assert_eq!(result.error_code.as_deref(), Some("fileTooLarge"));
        assert_eq!(result.source_bytes, Some((MAX_SOURCE_BYTES + 1) as usize));
        assert_eq!(result.terminal_limit, Some(true));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn at_limit_plain_read_is_allowed() {
        let dir = temp_dir();
        let path = dir.join("ok.txt");
        fs::write(&path, "hello\nworld\n").expect("write file");

        let req = LocalFetchRequest {
            path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        assert_eq!(result.status, "success");
        assert_eq!(result.error_code, None);

        let _ = fs::remove_dir_all(&dir);
    }

    // SEC-1: a bounded read of an interior body line of a private key must not
    // leak the key, even when the file is NOT key-named and the selected window
    // contains no BEGIN/END marker (so the anchored full-block patterns cannot
    // fire). The `Safe` scan is a passthrough, proving the full-file block guard
    // — not the window sanitizer — closes the leak.
    #[test]
    fn interior_private_key_window_does_not_leak_in_non_key_named_file() {
        let dir = temp_dir();
        let path = dir.join("notes.txt"); // deliberately NOT a *.pem/id_rsa path
        let body = "MIIEpQIBAAKCAQEAsplitKeyBodyLineOneAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let body2 = "c3BsaXRLZXlCb2R5TGluZVR3b0JBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC";
        let file = format!(
            "fn main() {{}}\nlet config = load();\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body2}\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
        );
        fs::write(&path, &file).expect("write file");

        // Select only the interior body lines (4..=5) — no BEGIN/END in view.
        let req = LocalFetchRequest {
            path: path.to_string_lossy().into_owned(),
            start_line: Some(4),
            end_line: Some(5),
            ..Default::default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        let content = result.content.clone().unwrap_or_default();
        assert!(
            !content.contains(body) && !content.contains(body2),
            "private key body leaked from an interior window: {content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // matchString must run on redacted text: a line-level secret that the
    // window sanitizer redacts must not be confirmable by probing matchString
    // (an exact-prefix oracle). Matching the true secret and a wrong guess must
    // be indistinguishable, and line numbers must stay source-accurate.
    #[test]
    fn match_string_cannot_probe_line_level_secrets() {
        use crate::security::{ContentSecurity, SecurityRegistry};
        use std::sync::Arc;
        let dir = temp_dir();
        let path = dir.join("config.txt");
        let secret = "AKIAIOSFODNN7EXAMPLE";
        fs::write(
            &path,
            format!("header\naws_access_key_id = {secret}\nneedle after\n"),
        )
        .expect("write file");
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let probe = |needle: &str| {
            let req = LocalFetchRequest {
                path: path.to_string_lossy().into_owned(),
                match_string: Some(needle.into()),
                context_lines: Some(0),
                ..Default::default()
            };
            execute_local_fetch(&req, &Paths(dir.clone()), &security, &NeverCancel)
        };
        let right = probe("AKIAIOSFODNN7EXAMPL");
        let wrong = probe("AKIAIOSFODNN7EXAMPQ");
        assert_eq!(right.selected_match_count, wrong.selected_match_count);
        assert_eq!(right.error_code, wrong.error_code);
        assert_eq!(right.selected_match_count, Some(0), "{right:?}");
        let after = probe("needle");
        assert_eq!(after.match_ranges, vec![LineRange { start: 3, end: 3 }]);
        assert_eq!(after.total_lines, Some(3));
        let _ = fs::remove_dir_all(&dir);
    }

    // SEC-1 over-redaction guard: an ordinary long base64 line (config blob,
    // hash, minified asset) with NO private-key markers anywhere in the file
    // must pass through byte-identical — the guard keys off BEGIN/END markers,
    // never bare base64.
    #[test]
    fn innocent_base64_window_is_returned_byte_identical() {
        let dir = temp_dir();
        let path = dir.join("data.txt");
        let blob = "aGVsbG8gd29ybGQgdGhpcyBpcyBqdXN0IGEgbG9uZyBiYXNlNjQgYmxvYg==";
        let file = format!("header\n{blob}\nfooter\n");
        fs::write(&path, &file).expect("write file");

        let req = LocalFetchRequest {
            path: path.to_string_lossy().into_owned(),
            start_line: Some(2),
            end_line: Some(2),
            ..Default::default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        let content = result.content.clone().unwrap_or_default();
        assert!(
            content.contains(blob),
            "innocent base64 was wrongly redacted: {content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
