use super::extraction::{extract, line_count};
use super::pagination::{continuation, page, result_counts};
use super::types::*;
use super::validation::{is_binary, validate_request};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
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
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => return LocalFetchResult::error(q.path.clone(), "fileReadFailed", e.to_string()),
    };
    let source_sha256 = format!("{:x}", Sha256::digest(&bytes));
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.clone(), "cancelled", e);
    }
    // Node's UTF-8 decoder replaces malformed sequences; binary detection is a
    // separate heuristic and must not turn an otherwise textual file into an error.
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let source_chars = raw.encode_utf16().count();
    let source_bytes = raw.len();
    let total_lines = line_count(&raw);
    let mut warnings = vec![];
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
            warnings.push(format!("No smaller outline is available for {}; using the standard content view. The outline may be unsupported, oversized, or unavailable for this source.",q.path));
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
    let (safe, security_warnings) = match security.sanitize(&selected, &path) {
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
    let source_ranges = if !safe.is_empty() && content_view == MinifyMode::None && safe == selected
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
    let modified = meta.modified().ok().and_then(system_time_iso);
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
fn bounded_continuation(q: &LocalFetchRequest) -> NextCalls {
    let mut query = q.clone();
    query.full_content = None;
    query.chunk_type = Some(ChunkType::Lines);
    query.offset = Some(0);
    query.limit = Some(100);
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
