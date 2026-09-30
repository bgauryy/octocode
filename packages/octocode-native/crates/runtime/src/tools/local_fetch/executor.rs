use super::extraction::{extract, line_count};
use super::pagination::{
    continuation, page, result_counts, sanitize_byte_page, sanitize_line_page,
};
use super::types::*;
use super::validation::{decode_text, is_binary, validate_request};
use crate::security::scan::ContentScan;
use crate::tools::cancel::CancellationCheck;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

/// Hard ceiling on source bytes read into memory for ANY localFetch path.
/// Plain, matchString, and line-range reads all slurp the whole source file
/// before extraction, so without this cap a single pathologically large file
/// would be read (and secret-scanned) entirely into memory. This is a
/// memory-safety bound distinct from — and larger than — the 100KB full-content
/// *return* cap below: files under this ceiling still page normally via
/// next.continue; files over it are refused outright with `fileTooLarge`.
pub(super) const MAX_SOURCE_BYTES: u64 = 10 * 1024 * 1024;

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
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    execute_local_fetch_with_regex(q, paths, security, cancel, &LocalFetchRegex::default())
}
pub fn execute_local_fetch_with_regex(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if let Err(e) = validate_request(q) {
        return LocalFetchResult::error(q.path.to_string(), "invalidQuery", e);
    }
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    let validated = match paths.validate_read(Path::new(q.path.as_str())) {
        Ok(path) => path,
        Err(failure) => {
            let display = failure.safe_path.as_deref().unwrap_or(&q.path);
            let message = if failure.sparse_checkout {
                format!(
                    "File not found: {display}. It lies in a sparse checkout, so it may exist outside the checked-out paths: re-clone with a sparsePath that covers it, or read it with ghGetFileContent."
                )
            } else if failure.resource_missing {
                format!(
                    "File not found: {display}. Verify the path with structureSearch operation:\"files\"."
                )
            } else {
                failure.message
            };
            let code = if failure.code == crate::policy::PATH_OUTSIDE_ALLOWED_ROOTS {
                crate::policy::PATH_OUTSIDE_ALLOWED_ROOTS
            } else {
                "fileAccessFailed"
            };
            let mut result = LocalFetchResult::error(q.path.to_string(), code, message);
            result.resource_missing = failure.resource_missing;
            result.resolved_path = Some(q.path.to_string());
            return result;
        }
    };
    let path = validated.canonical;
    let display_path = validated.display;
    let meta = match fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => {
            return LocalFetchResult::error(
                q.path.to_string(),
                "fileAccessFailed",
                "Path is not a regular file".into(),
            );
        }
        Err(e) => {
            return LocalFetchResult::error(q.path.to_string(), "fileAccessFailed", e.to_string());
        }
    };
    // Past the whole-file ceiling, only a streamed line window is served
    // (see `large_source`); past the streaming ceiling nothing is read.
    if meta.len() > super::large_source::MAX_STREAM_SOURCE_BYTES {
        return source_too_large(&q.path, meta.len());
    }
    let mut sample = [0_u8; 8192];
    let sample_len = match fs::File::open(&path).and_then(|mut file| file.read(&mut sample)) {
        Ok(length) => length,
        Err(error) => {
            return LocalFetchResult::error(
                q.path.to_string(),
                "fileReadFailed",
                error.to_string(),
            );
        }
    };
    if is_binary(&sample[..sample_len]) {
        let mut result = LocalFetchResult::error(
            q.path.to_string(),
            "binaryFileUnsupported",
            format!(
                "Binary file unsupported: {display_path}. Read a text source file, or use structureSearch operation:\"files\" for file metadata."
            ),
        );
        result.resolved_path = Some(q.path.to_string());
        // No range or matchString can make binary bytes readable; say so
        // instead of the generic "remove matchString" recovery.
        result.hints = vec![
            "Do not retry this path with other ranges; choose a text source file instead.".into(),
        ];
        return result;
    }
    if meta.len() > MAX_SOURCE_BYTES {
        let mut result = super::large_source::fetch_window(
            q,
            &path,
            meta.len(),
            meta.modified().ok().and_then(system_time_iso),
            security,
            cancel,
            regex,
        );
        if result.status != "error" {
            result.path = q.path.to_string();
        }
        return result;
    }
    // An oversized whole-file read returns its first bounded page inline
    // (with next.continue) instead of an empty error that costs a call.
    let bounded;
    let full_content_limited = q.full_content == Some(true)
        && q.minify_mode() == MinifyMode::None
        && q.match_string.is_none()
        && q.start_line().is_none()
        && meta.len() > 100 * 1024;
    let q = if full_content_limited {
        bounded = bounded_query(q);
        &bounded
    } else {
        q
    };
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
        Err(e) => {
            return LocalFetchResult::error(q.path.to_string(), "fileReadFailed", e.to_string());
        }
    };
    let mut result = process_fetched_content(
        q,
        &bytes,
        &path,
        meta.modified().ok().and_then(system_time_iso),
        security,
        cancel,
        regex,
    );
    if full_content_limited {
        mark_full_content_limited(&mut result);
    }
    bind_continuation_to_source(&mut result);
    result
}

/// Bind the next page of a local view to the file version it was cut from, so
/// a replay against a changed file is rejected instead of mixing versions.
/// (Remote reads pin a commit SHA in their continuation instead.)
fn bind_continuation_to_source(result: &mut LocalFetchResult) {
    let snapshot = result
        .source_sha256
        .as_deref()
        .and_then(|digest| digest.parse().ok());
    if let Some(next) = result
        .next
        .as_mut()
        .and_then(|next| next.r#continue.as_mut())
    {
        next.query.snapshot = snapshot;
    }
}

/// Shared post-acquisition content processing. Performs no filesystem access;
/// callers own source authorization, binary/transport limits and provenance.
pub fn process_fetched_content(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    modified: Option<String>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if let Err(error) = validate_request(q) {
        return LocalFetchResult::error(q.path.to_string(), "invalidQuery", error);
    }
    let source_sha256 = hex::encode(Sha256::digest(bytes));
    if let Some(expected) = q.snapshot.as_deref()
        && **expected != source_sha256
    {
        return stale_snapshot(q);
    }
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    // Binary detection is a separate heuristic; text that is not UTF-8 is
    // decoded (Latin-1 or lossy UTF-8) with a warning, never refused.
    let (raw, decode_warning) = decode_text(bytes);
    let source_chars = raw.encode_utf16().count();
    let source_bytes = raw.len();
    let total_lines = line_count(&raw);
    let mut warnings: Vec<String> = decode_warning.map(str::to_owned).into_iter().collect();
    // Redact whole private-key blocks across the full file BEFORE any
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
            Err((code, message)) => {
                return LocalFetchResult::error(q.path.to_string(), &code, message);
            }
        }
    } else {
        (raw, false)
    };
    let mode = q.minify_mode();
    let match_blocks = q.match_string.is_some() && mode != MinifyMode::None;
    let applied = if match_blocks { MinifyMode::None } else { mode };
    let ext = match extract(q, &raw, regex) {
        Ok(x) => x,
        Err(e) => {
            return LocalFetchResult::error(
                q.path.to_string(),
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
            path: q.path.to_string(),
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
            // The response keeps one hint per row. In a file with redactions
            // a shorter token cannot reach text matched against a
            // placeholder, so that explanation replaces the generic advice.
            // It depends only on the file, never on the guess (no oracle),
            // and reveals no more than a plain fetch (which shows the
            // placeholders).
            hints: vec![if match_redacted || key_blocks_redacted {
                REDACTED_MATCH_HINT.into()
            } else {
                no_match_hint(
                    q.match_string_is_regex.unwrap_or(false),
                    q.match_string_case_sensitive.unwrap_or(false),
                    "localSearch",
                )
            }],
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
            out_of_range: false,
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
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    // A complete view over the limit returns its first bounded page inline.
    let bounded_view;
    let view_limited = q.full_content == Some(true) && selected.len() > FULL_CONTENT_LIMIT_BYTES;
    let q = if view_limited {
        bounded_view = bounded_query(q);
        &bounded_view
    } else {
        q
    };
    // A line or byte page is scanned on its own window (see
    // `sanitize_line_page` / `sanitize_byte_page`); complete views keep the
    // whole-view scan because they return the whole view.
    let line_page = if q.full_content != Some(true) {
        match page(&selected, q) {
            // A window scan error falls back to the whole-view scan, which
            // owns the typed security-limit recovery.
            Ok(raw) if raw.pagination.chunk_type == ChunkType::Lines => {
                sanitize_line_page(&selected, raw, source_path, security).unwrap_or(None)
            }
            Ok(raw) => sanitize_byte_page(&selected, raw, source_path, security).unwrap_or(None),
            Err(e) => return LocalFetchResult::error(q.path.to_string(), "invalidPagination", e),
        }
    } else {
        None
    };
    // Page sanitizers keep line counts, so a redacted page still maps onto
    // source lines; a whole-view scan does only when it kept the count.
    let (pg, view_redacted, view_empty, lines_kept) = if let Some((pg, redacted)) = line_page {
        warnings.extend(ext.warnings);
        (pg, redacted, selected.is_empty(), true)
    } else {
        let (safe, security_warnings) = match security.sanitize(&selected, source_path) {
            Ok(x) => x,
            Err((c, _)) if c == "contentSecurityLimit" => {
                let mut result = LocalFetchResult::error(
                q.path.to_string(),
                &c,
                "The selected content view exceeds the secret scanner size limit. Byte windows cannot safely split unscanned content. Select a smaller source-line range.".into(),
            );
                result.path = q.path.to_string();
                result.total_lines = Some(total_lines);
                result.source_chars = Some(source_chars);
                result.source_bytes = Some(source_bytes);
                result.is_partial = Some(true);
                result.terminal_limit = Some(true);
                result.partial_reasons = vec![PartialReason::SecuritySelectedViewSizeLimit];
                if total_lines > 1 && (q.start_line().is_none() || q.start_line() != q.end_line()) {
                    let line = ext.start.or(q.start_line()).unwrap_or(1);
                    let query = single_line_query(q, line);
                    result.next = Some(NextCalls {
                    r#continue: None,
                    restart: None,
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
            Err((c, m)) => return LocalFetchResult::error(q.path.to_string(), &c, m),
        };
        warnings.extend(ext.warnings);
        warnings.extend(security_warnings);
        if let Err(e) = cancel.check() {
            return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
        }
        let pg = match page(&safe, q) {
            Ok(p) => p,
            Err(e) => return LocalFetchResult::error(q.path.to_string(), "invalidPagination", e),
        };
        let lines_kept = safe == selected || line_count(&safe) == line_count(&selected);
        (pg, safe != selected, safe.is_empty(), lines_kept)
    };
    let (chars, ret_bytes, ret_lines) = result_counts(&pg.text);
    // Redacted text is not the source: say so, as localSearch does. Count only
    // when a redaction pass ran, so literal placeholder text in a clean file
    // is not misreported.
    let redactions = if view_redacted || match_redacted || key_blocks_redacted {
        pg.text.matches("[REDACTED").count()
    } else {
        0
    };
    if redactions > 0 {
        warnings.push(format!(
            "redactedContent: {redactions} secret-shaped value(s) in the returned text were replaced by [REDACTED…] placeholders; this content is not verbatim source."
        ));
    } else if view_redacted {
        warnings.push(
            "redactedContent: secret-shaped text in the requested view was redacted; this content is not verbatim source.".into(),
        );
    }
    let out_of_range = pg.out_of_range;
    let next = if out_of_range {
        warnings.push(format!(
            "offset {} is past the end of the selected view ({} {}); nothing was returned. Follow next.restart to read from the start.",
            q.offset().unwrap_or(0),
            match pg.pagination.chunk_type {
                ChunkType::Lines => pg.pagination.total_lines,
                ChunkType::Bytes => pg.pagination.total_bytes,
            },
            match pg.pagination.chunk_type {
                ChunkType::Lines => "lines",
                ChunkType::Bytes => "bytes",
            }
        ));
        let mut query = q.clone();
        query.offset = Some(0);
        Some(NextCalls {
            r#continue: None,
            read_bounded_lines: None,
            restart: Some(Continuation {
                tool: "localFetch".into(),
                query,
                confidence: "exact".into(),
                reason: Some(
                    "Offset is past the end of the selected view; restart from offset 0.".into(),
                ),
            }),
        })
    } else {
        continuation(q, &pg.pagination)
    };
    // Redaction (source-wide for matchString, or on the page) replaces text
    // within lines; the anchors stay and the warning above says the text is
    // not verbatim.
    let source_ranges =
        if !out_of_range && !view_empty && content_view == MinifyMode::None && lines_kept {
            if let Some(lines) = ext.source_lines.as_ref() {
                let page_lines: Vec<usize> = lines
                    [pg.view_lines.0.saturating_sub(1)..pg.view_lines.1.min(lines.len())]
                    .iter()
                    .copied()
                    .filter(|line| *line != super::extraction::OMISSION_LINE)
                    .collect();
                compress_ranges(&page_lines)
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
        path: q.path.to_string(),
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
        is_partial: (next.is_some() && !out_of_range).then_some(true),
        partial_reasons: if view_limited {
            vec![PartialReason::FullContentLimit]
        } else {
            vec![]
        },
        terminal_limit: None,
        metadata_unavailable: vec![],
        out_of_range,
        next,
    }
}

/// Complete views larger than this return page 1 plus next.continue.
const FULL_CONTENT_LIMIT_BYTES: usize = 50_000;

fn mark_full_content_limited(result: &mut LocalFetchResult) {
    if result.status == "error" {
        return;
    }
    if !result
        .partial_reasons
        .contains(&PartialReason::FullContentLimit)
    {
        result.partial_reasons.push(PartialReason::FullContentLimit);
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
/// Same file and caller metadata, every extraction selector reset to one raw line.
fn single_line_query(q: &LocalFetchQuery, line: usize) -> LocalFetchQuery {
    LocalFetchQuery {
        start_line: wire_positive(line),
        end_line: wire_positive(line),
        minify: Some(MinifyMode::None),
        full_content: None,
        match_string: None,
        match_string_is_regex: None,
        match_string_case_sensitive: None,
        context_lines: None,
        context_bytes: None,
        chunk_type: None,
        offset: None,
        chunk_size: None,
        ..q.clone()
    }
}

/// The continued source changed since its page was cut. Pages of two file
/// versions must not be combined; restart the same view from the start.
fn stale_snapshot(q: &LocalFetchQuery) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "staleSnapshot",
        "The file changed since the previous page was read; its pages cannot be combined. Follow next.restart to read the current version from the start.".into(),
    );
    result.resolved_path = Some(q.path.to_string());
    let mut query = q.clone();
    query.snapshot = None;
    query.offset = None;
    result.next = Some(NextCalls {
        r#continue: None,
        read_bounded_lines: None,
        restart: Some(Continuation {
            tool: "localFetch".into(),
            query,
            confidence: "exact".into(),
            reason: Some("The source changed; restart this view on the current version.".into()),
        }),
    });
    result
}

/// The first bounded line page of the same view as a complete read.
fn bounded_query(q: &LocalFetchQuery) -> LocalFetchQuery {
    let mut query = q.clone();
    query.full_content = None;
    query.chunk_type = Some(ChunkType::Lines);
    query.offset = Some(0);
    query.chunk_size = wire_positive(DEFAULT_LINE_CHUNK);
    query
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
    let (y, m, d) = crate::civil_date::civil_from_days(days);
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
    use crate::tools::cancel::NeverCancel;
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
    fn source_above_the_streaming_ceiling_returns_file_too_large() {
        let dir = temp_dir();
        let path = dir.join("huge.txt");
        // Sparse file just past the streaming ceiling — the size guard fires
        // before any bytes are read, so this stays cheap.
        let file = fs::File::create(&path).expect("create file");
        let len = super::super::large_source::MAX_STREAM_SOURCE_BYTES + 1;
        file.set_len(len).expect("grow file");
        drop(file);

        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ..LocalFetchQuery::test_default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        assert_eq!(result.status, "error");
        assert_eq!(result.error_code.as_deref(), Some("fileTooLarge"));
        assert_eq!(result.source_bytes, Some(len as usize));
        assert_eq!(result.terminal_limit, Some(true));

        let _ = fs::remove_dir_all(&dir);
    }

    // Past the whole-file ceiling a plain read streams a bounded line window:
    // exact whole-file totals, a digest-bound continuation that advances,
    // and matchString redirected to localSearch.
    #[test]
    fn oversized_text_source_is_served_as_streamed_line_windows() {
        let dir = temp_dir();
        let path = dir.join("big.log");
        let mut text = String::new();
        let mut lines = 0usize;
        while text.len() as u64 <= MAX_SOURCE_BYTES {
            lines += 1;
            text.push_str(&format!("line {lines} payload payload payload payload\n"));
        }
        fs::write(&path, &text).expect("write");
        let query =
            |q: LocalFetchQuery| execute_local_fetch(&q, &Paths(dir.clone()), &Safe, &NeverCancel);
        let first = query(LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ..LocalFetchQuery::test_default()
        });
        assert_eq!(first.status, "success", "{first:?}");
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
            .and_then(|n| n.r#continue.clone())
            .expect("continue");
        assert!(next.query.snapshot.is_some());
        let second = query(next.query);
        assert_eq!(second.status, "success", "{second:?}");
        assert_eq!(second.start_line, first.end_line.map(|line| line + 1));

        // A wide explicit window pages within itself and advances.
        let wide = query(LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            start_line: wire_positive(1),
            end_line: wire_positive(3_000),
            ..LocalFetchQuery::test_default()
        });
        let inner = wide
            .next
            .as_ref()
            .and_then(|n| n.r#continue.clone())
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
            start_line: wire_positive(lines),
            end_line: wire_positive(lines),
            ..LocalFetchQuery::test_default()
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
            match_string: Some("line 7 ".parse().expect("match")),
            ..LocalFetchQuery::test_default()
        });
        assert_eq!(matched.error_code.as_deref(), Some("largeSourceWindowOnly"));
        assert!(
            matched
                .hints
                .iter()
                .any(|hint| hint.contains("localSearch")),
            "{matched:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn at_limit_plain_read_is_allowed() {
        let dir = temp_dir();
        let path = dir.join("ok.txt");
        fs::write(&path, "hello\nworld\n").expect("write file");

        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ..LocalFetchQuery::test_default()
        };
        let result = execute_local_fetch(&req, &Paths(dir.clone()), &Safe, &NeverCancel);

        assert_eq!(result.status, "success");
        assert_eq!(result.error_code, None);

        let _ = fs::remove_dir_all(&dir);
    }

    // A bounded read of an interior body line of a private key must not
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
        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            start_line: wire_positive(4),
            end_line: wire_positive(5),
            ..LocalFetchQuery::test_default()
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
        use crate::security::ContentSecurity;
        let dir = temp_dir();
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
                match_string: Some(needle.parse().expect("match string")),
                context_lines: Some(0),
                ..LocalFetchQuery::test_default()
            };
            execute_local_fetch(&req, &Paths(dir.clone()), &security, &NeverCancel)
        };
        let right = probe("AKIAIOSFODNN7EXAMPL");
        let wrong = probe("AKIAIOSFODNN7EXAMPQ");
        assert_eq!(right.selected_match_count, wrong.selected_match_count);
        assert_eq!(right.error_code, wrong.error_code);
        assert_eq!(right.selected_match_count, Some(0), "{right:?}");
        // Same hints either way (no oracle), and they explain the miss.
        assert_eq!(right.hints, wrong.hints);
        assert!(
            right.hints.iter().any(|hint| hint == REDACTED_MATCH_HINT),
            "{right:?}"
        );
        let clean = dir.join("clean.txt");
        fs::write(&clean, "nothing secret\n").expect("write clean");
        let miss = execute_local_fetch(
            &LocalFetchQuery {
                path: clean.to_string_lossy().parse().expect("path"),
                match_string: Some("absent".parse().expect("match string")),
                ..LocalFetchQuery::test_default()
            },
            &Paths(dir.clone()),
            &security,
            &NeverCancel,
        );
        assert!(
            !miss.hints.iter().any(|hint| hint == REDACTED_MATCH_HINT),
            "{miss:?}"
        );
        let after = probe("needle");
        assert_eq!(after.match_ranges, vec![LineRange { start: 3, end: 3 }]);
        assert_eq!(after.total_lines, Some(3));
        let _ = fs::remove_dir_all(&dir);
    }

    // Over-redaction guard: an ordinary long base64 line (config blob,
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

        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            start_line: wire_positive(2),
            end_line: wire_positive(2),
            ..LocalFetchQuery::test_default()
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

/// Why a matchString can miss text that is visibly in the file.
const REDACTED_MATCH_HINT: &str = "No visible line matches; this file has secrets, and matchString runs on [REDACTED…] placeholders, never secret text.";

/// Next step for a matchString that selected no line, tuned to how it matched.
/// `finder` names the search tool that locates the file containing the text.
/// Kept under the 120-char guidance cap.
pub(crate) fn no_match_hint(regex: bool, case_sensitive: bool, finder: &str) -> String {
    match (regex, case_sensitive) {
        (false, false) => format!(
            "No line contains this text; try a shorter token, matchStringIsRegex:true, or {finder}."
        ),
        (false, true) => format!(
            "No line contains this case-sensitive text; drop matchStringCaseSensitive or use {finder}."
        ),
        (true, false) => {
            format!("No line matches this regex (^/$ per line); simplify it or use {finder}.")
        }
        (true, true) => format!(
            "No line matches this case-sensitive regex; drop matchStringCaseSensitive or use {finder}."
        ),
    }
}

#[cfg(test)]
mod line_page_scan_tests {
    use super::*;
    use crate::security::ContentSecurity;
    use crate::tools::cancel::NeverCancel;
    use std::path::Path;

    fn security() -> ContentSecurity {
        ContentSecurity::new()
    }
    fn fetch(source: &str, q: &LocalFetchQuery) -> LocalFetchResult {
        process_fetched_content(
            q,
            source.as_bytes(),
            Path::new("/fixture/app.ts"),
            None,
            &security(),
            &NeverCancel,
            &LocalFetchRegex::default(),
        )
    }
    fn lines_query(offset: usize, chunk: usize) -> LocalFetchQuery {
        LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            chunk_type: Some(ChunkType::Lines),
            offset: Some(wire_count(offset)),
            chunk_size: wire_positive(chunk),
            ..LocalFetchQuery::test_default()
        }
    }
    fn filler(n: usize) -> String {
        (1..=n)
            .map(|i| format!("const filler_{i} = {i};\n"))
            .collect()
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
            let Some(next) = result.next.and_then(|next| next.r#continue) else {
                break;
            };
            q = next.query;
        }
        assert_eq!(union, expected);
    }

    fn bytes_query(offset: usize, chunk: usize) -> LocalFetchQuery {
        LocalFetchQuery {
            chunk_type: Some(ChunkType::Bytes),
            ..lines_query(offset, chunk)
        }
    }

    /// Byte pages walked by `next.continue` reproduce the whole-view scan:
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
                let Some(next) = result.next.and_then(|next| next.r#continue) else {
                    break;
                };
                q = next.query;
            }
            assert!(pages > 1, "chunk {chunk}");
            assert_eq!(union, expected, "chunk {chunk}");
        }
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
            ) -> Result<(String, Vec<String>), (String, String)> {
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
            None,
            &scanner,
            &NeverCancel,
            &LocalFetchRegex::default(),
        );
        let page_time = started.elapsed();
        assert!(paged.content.is_some_and(|content| content.len() >= 8_000));
        let scanned = scanner.1.get();
        assert!(
            scanned < 8_000 + 4 * super::super::pagination::PAGE_SCAN_MARGIN_BYTES,
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
        assert!(next.r#continue.is_none());
        assert_eq!(next.restart.expect("restart").query.offset, Some(0));
        let wire = serde_json::to_value(&result).expect("wire");
        assert_eq!(wire["pagination"]["outOfRange"], true, "{wire}");
        assert!(wire.get("sourceLineRanges").is_none(), "{wire}");

        let bytes = LocalFetchQuery {
            chunk_type: Some(ChunkType::Bytes),
            ..lines_query(source.len(), 100)
        };
        let result = fetch(&source, &bytes);
        assert!(result.out_of_range && result.source_line_ranges.is_empty());
        // The last real page is not out of range.
        assert!(!fetch(&source, &lines_query(400, 100)).out_of_range);
    }
}
