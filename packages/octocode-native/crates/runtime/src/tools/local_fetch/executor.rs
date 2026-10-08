use super::extraction::{extract, line_count};
use super::pagination::{
    continuation, page, result_counts, sanitize_byte_page, sanitize_line_page,
};
use super::types::*;
use super::validation::{decode_text, is_binary};
use crate::security::scan::ContentScan;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::ToolId;
use std::fs;
use std::io::Read;
use std::path::Path;

/// In-memory read guard. Larger sources use bounded streaming windows up to
/// `MAX_STREAM_SOURCE_BYTES`; a growing small-file read stops at this guard.
pub(super) const MAX_SOURCE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Copy)]
enum SourceSizeLimit {
    Streaming,
    InMemoryGrowth,
}

fn source_too_large(path: &str, len: u64, limit: SourceSizeLimit) -> LocalFetchResult {
    let (message, hint) = match limit {
        SourceSizeLimit::Streaming => (
            format!(
                "File too large: {len} bytes (streaming ceiling: {} bytes / 1 GiB). localFetch cannot read this path, even in ranges.",
                super::large_source::MAX_STREAM_SOURCE_BYTES
            ),
            "No ranges, matchString, or fullContent lift this ceiling; choose a smaller text source or split it outside localFetch.",
        ),
        SourceSizeLimit::InMemoryGrowth => (
            format!(
                "Source grew past the in-memory read guard: at least {len} bytes (guard: {MAX_SOURCE_BYTES} bytes / 10 MiB); the read stopped."
            ),
            "Retry a bounded line window once the source stops changing; sources over 10 MiB stream, up to the 1 GiB ceiling.",
        ),
    };
    let mut result = LocalFetchResult::error(path.to_owned(), "fileTooLarge", message);
    result.source_bytes = Some(len as usize);
    result.is_partial = Some(true);
    result.terminal_limit = Some(true);
    result.partial_reasons = vec![PartialReason::FullContentSource];
    result.hints = vec![hint.into()];
    result
}

/// Read the rest of `reader` after the `bytes` already read from its start,
/// up to the in-memory guard plus one sentinel byte.
fn read_in_memory_source(reader: impl Read, mut bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let left = (MAX_SOURCE_BYTES + 1).saturating_sub(bytes.len() as u64);
    reader.take(left).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Bytes of the binary sniff: one read from the start of the file.
const BINARY_SAMPLE_BYTES: usize = 8192;

pub fn execute_local_fetch(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
    window: Option<usize>,
) -> LocalFetchResult {
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    let validated = match paths.validate_read(Path::new(q.path.as_str())) {
        Ok(path) => path,
        Err(failure) => return path_failure(q, failure),
    };
    let path = validated.canonical;
    let display_path = validated.display;
    let meta = match fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        Ok(m) if m.is_dir() => return not_a_file(q),
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
        return source_too_large(&q.path, meta.len(), SourceSizeLimit::Streaming);
    }
    // One open serves the binary sniff and the whole read: the sniffed
    // bytes start the buffer, sized from the stat.
    let in_memory = meta.len() <= MAX_SOURCE_BYTES;
    let opened = fs::File::open(&path).and_then(|mut file| {
        let capacity = if in_memory {
            usize::try_from(meta.len()).map_or(BINARY_SAMPLE_BYTES, |len| len + 1)
        } else {
            BINARY_SAMPLE_BYTES
        };
        let mut bytes = Vec::with_capacity(capacity.max(BINARY_SAMPLE_BYTES));
        let mut sample = [0_u8; BINARY_SAMPLE_BYTES];
        let read = file.read(&mut sample)?;
        bytes.extend_from_slice(&sample[..read]);
        Ok((file, bytes))
    });
    let (file, sample) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            return LocalFetchResult::error(
                q.path.to_string(),
                "fileReadFailed",
                error.to_string(),
            );
        }
    };
    if is_binary(&sample) {
        return refuse_binary(q, &display_path);
    }
    if !in_memory {
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
    // Read at most the cap (+1 sentinel byte). Re-check the length in case the
    // file grew past the ceiling between the stat above and this read (TOCTOU).
    let bytes = match read_in_memory_source(file, sample) {
        Ok(b) if b.len() as u64 > MAX_SOURCE_BYTES => {
            return source_too_large(&q.path, b.len() as u64, SourceSizeLimit::InMemoryGrowth);
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
        &SourceFacts {
            modified: meta.modified().ok().and_then(system_time_iso),
            window,
        },
        security,
        cancel,
        regex,
    );
    bind_continuation_to_source(&mut result);
    result
}

/// A directory named as a file: say so, and lead to its tree.
fn not_a_file(q: &LocalFetchQuery) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "notAFile",
        format!(
            "{} is a directory, not a file; list it with structureSearch, then read a file in it.",
            q.path
        ),
    );
    result.next = Some(NextCalls {
        view_tree: Some(
            crate::tools::result::Continuation::new(
                ToolId::StructureSearch,
                serde_json::json!({"operation": "tree", "path": q.path.as_str()}),
            )
            .build(),
        ),
        ..NextCalls::default()
    });
    result
}

/// A missing file's listing by its stem (the name without its
/// extension) under its closest existing directory: a moved file or a
/// changed extension shows at any depth.
fn missing_file_listing(q: &LocalFetchQuery, nearest: &str) -> Option<serde_json::Value> {
    let name = Path::new(q.path.as_str()).file_name()?.to_string_lossy();
    let stem = name.split_once('.').map_or(name.as_ref(), |(stem, _)| stem);
    (!stem.is_empty()).then(|| {
        crate::tools::result::Continuation::new(
            ToolId::StructureSearch,
            serde_json::json!({"operation": "files", "path": nearest, "include": [stem]}),
        )
        .why("Find the file by its name under its closest existing directory.")
        .build()
    })
}

/// The row for a path the policy refused or could not resolve.
fn path_failure(q: &LocalFetchQuery, failure: PathFailure) -> LocalFetchResult {
    if failure.directory {
        return not_a_file(q);
    }
    let display = failure.safe_path.as_deref().unwrap_or(&q.path);
    let message = if failure.sparse_checkout {
        format!(
            "Path does not exist: {display}. It lies in a sparse checkout, so it may exist outside the checked-out paths: re-clone with a path that covers it, or read it with ghGetFileContent."
        )
    } else if failure.resource_missing {
        format!("Path does not exist: {display}")
    } else {
        failure.message
    };
    let code = if matches!(
        failure.code.as_str(),
        "outsideAllowedRoots"
            | "symlinkEscape"
            | "permissionDenied"
            | crate::policy::PATH_POLICY_DENIED
    ) {
        failure.code.as_str()
    } else if failure.resource_missing {
        // The not-found code every local tool reports.
        "pathNotFound"
    } else {
        "fileAccessFailed"
    };
    let mut result = LocalFetchResult::error(q.path.to_string(), code, message);
    result.resource_missing = failure.resource_missing;
    if failure.resource_missing
        && let Some(listing) = failure
            .nearest_dir
            .as_deref()
            .and_then(|nearest| missing_file_listing(q, nearest))
    {
        result.next = Some(NextCalls {
            find_file: Some(listing),
            ..NextCalls::default()
        });
    }
    result
}

/// The refusal for a file whose leading bytes are binary.
fn refuse_binary(q: &LocalFetchQuery, display_path: &str) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "binaryFileUnsupported",
        format!(
            "Binary file unsupported: {display_path}. Read a text source file, or use structureSearch operation:\"files\" for file metadata."
        ),
    );
    // No range or matchString can make binary bytes readable; say so
    // instead of the generic "remove matchString" recovery.
    result.hints =
        vec!["Do not retry this path with other ranges; choose a text source file instead.".into()];
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
    facts: &SourceFacts,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    let mut source = match prepare_source(q, bytes, source_path, security, cancel) {
        Ok(source) => source,
        Err(result) => return *result,
    };
    let mut ext = match extract(q, &source.text, regex) {
        Ok(extraction) => extraction,
        Err(error) => return extraction_error(q, error),
    };
    if ext.count == Some(0) {
        return no_match(q, &source);
    }
    let view = select_view(q, &mut ext, &mut source.warnings);
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    // A complete view over the limit returns its first bounded page inline.
    let bounded_view;
    let view_limited =
        q.full_content == Some(true) && facts.window.is_some_and(|window| view.text.len() > window);
    let q = if view_limited {
        bounded_view = bounded_query(q);
        &bounded_view
    } else {
        q
    };
    let head_view = head_query(q, view.mode, source.total_lines);
    let head = head_view.is_some();
    let page_q = head_view.as_ref().unwrap_or(q);
    let scanned = match scan_page(
        q,
        page_q,
        &view.text,
        source_path,
        security,
        &source,
        ext.start,
    ) {
        Ok(scanned) => scanned,
        Err(result) => return *result,
    };
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    let mut warnings = std::mem::take(&mut source.warnings);
    warnings.extend(std::mem::take(&mut ext.warnings));
    warnings.extend(scanned.warnings);
    let pg = scanned.page;
    warn_redacted(&pg.text, scanned.redacted, &source, &mut warnings);
    let out_of_range = pg.out_of_range;
    let mut next = page_next(q, &pg, head, &mut warnings);
    // Follow-ups to data the extraction selected but did not return.
    let follow_ups = std::mem::take(&mut ext.next);
    if !out_of_range && !follow_ups.is_empty() {
        next.get_or_insert_with(NextCalls::default)
            .absorb(follow_ups);
    }
    read_scope_warnings(q, &pg, head, view.mode, source.total_lines, &mut warnings);
    // Redaction (source-wide for matchString, or on the page) replaces text
    // within lines; the anchors stay and the warning above says the text is
    // not verbatim.
    let source_ranges =
        if !out_of_range && !scanned.empty && view.mode == MinifyMode::None && scanned.lines_kept {
            page_source_ranges(ext.source_lines.as_deref(), pg.view_lines)
        } else {
            vec![]
        };
    let blocks = if out_of_range {
        vec![]
    } else {
        std::mem::take(&mut ext.blocks)
    };
    let mut matched_lines = std::mem::take(&mut ext.matched_lines);
    matched_lines.retain(|n| source_ranges.iter().any(|r| *n >= r.start && *n <= r.end));
    let (chars, ret_bytes, ret_lines) = result_counts(&pg.text);
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(source.sha256),
        content: Some(pg.text),
        content_view: Some(view.mode),
        minify_fallback: view.fallback,
        warnings,
        total_lines: Some(source.total_lines),
        start_line: ext.start,
        end_line: ext.end,
        source_line_ranges: source_ranges,
        match_ranges: ext.match_ranges,
        matched_lines,
        blocks,
        selected_match_count: ext.count,
        modified: (view.mode != MinifyMode::Symbols)
            .then(|| facts.modified.clone())
            .flatten(),
        source_chars: Some(source.chars),
        source_bytes: Some(source.bytes),
        returned_chars: Some(chars),
        returned_bytes: Some(ret_bytes),
        returned_lines: Some(ret_lines),
        pagination: Some(pg.pagination),
        // A capped regex scan selected a prefix of the matches (H1).
        is_partial: ((next.as_ref().is_some_and(NextCalls::leaves_more) && !out_of_range)
            || ext.match_limited)
            .then_some(true),
        partial_reasons: if view_limited {
            vec![PartialReason::FullContent]
        } else {
            vec![]
        },
        out_of_range,
        next,
        ..LocalFetchResult::blank("success")
    }
}

/// The head page of an untargeted first read of a large file: paging
/// through every line is rarely what an unanchored read needs, and the head
/// plus a locate (or an anchored read) costs a fraction of a page.
fn head_query(
    q: &LocalFetchQuery,
    mode: MinifyMode,
    total_lines: usize,
) -> Option<LocalFetchQuery> {
    (unanchored_first_read(q) && mode == MinifyMode::None && total_lines >= LARGE_READ_LINES).then(
        || LocalFetchQuery {
            unit: Some(WindowUnit::Lines),
            length: wire_positive(HEAD_LINES),
            ..q.clone()
        },
    )
}

/// The decoded source, redacted before any window is cut from it.
struct Source {
    sha256: String,
    text: String,
    chars: usize,
    bytes: usize,
    total_lines: usize,
    key_blocks_redacted: bool,
    match_redacted: bool,
    warnings: Vec<String>,
}

/// Decode and redact the source; a stale snapshot, a cancel or a failed
/// redaction ends the read.
fn prepare_source(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Source, Box<LocalFetchResult>> {
    let sha256 = crate::digest::sha256(bytes);
    if let Some(expected) = q.snapshot.as_deref()
        && **expected != sha256
    {
        return Err(Box::new(stale_snapshot(q)));
    }
    cancel
        .check()
        .map_err(|e| Box::new(LocalFetchResult::error(q.path.to_string(), "cancelled", e)))?;
    // Binary detection is a separate heuristic; text that is not UTF-8 is
    // decoded (Latin-1 or lossy UTF-8) with a warning, never refused.
    let (raw, decode_warning) = decode_text(bytes);
    let chars = raw.encode_utf16().count();
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
    let (text, match_redacted) = if q.match_string.is_some() {
        redact_source_lines(&raw, source_path, security).map_err(|(code, message)| {
            Box::new(LocalFetchResult::error(q.path.to_string(), &code, message))
        })?
    } else {
        (raw, false)
    };
    Ok(Source {
        sha256,
        text,
        chars,
        bytes: source_bytes,
        total_lines,
        key_blocks_redacted,
        match_redacted,
        warnings,
    })
}

fn extraction_error(q: &LocalFetchQuery, error: String) -> LocalFetchResult {
    let code = if error.starts_with("Invalid regex") {
        "invalidPattern"
    } else if error.starts_with("Regex execution unavailable") {
        "executionFailed"
    } else {
        "invalidInput"
    };
    LocalFetchResult::error(q.path.to_string(), code, error)
}

/// A matchString that selected no line: an empty row with one hint.
fn no_match(q: &LocalFetchQuery, source: &Source) -> LocalFetchResult {
    // The response keeps one hint per row. In a file with redactions a
    // shorter token cannot reach text matched against a placeholder, so
    // that explanation replaces the generic advice. It depends only on the
    // file, never on the guess (no oracle), and reveals no more than a
    // plain fetch (which shows the placeholders).
    let redacted = source.match_redacted || source.key_blocks_redacted;
    let case_sensitive = q
        .match_strings()
        .iter()
        .any(|pattern| q.case_sensitive_for(pattern));
    let hint = if redacted {
        REDACTED_MATCH_HINT.into()
    } else {
        no_match_hint(q.is_regex(), case_sensitive, ToolId::LocalSearch.as_str())
    };
    // A case-sensitive miss reruns as the same read, case-insensitive.
    let ignore_case = (case_sensitive && !redacted).then(|| {
        let mut query = q.clone();
        query.case_mode = Some(ReadCaseMode::Insensitive);
        Continuation {
            query,
            reason: Some("Match the text case-insensitively.".into()),
        }
    });
    // The text may live in another file of this directory: one localSearch
    // there finds it (not offered when redactions hide the text).
    let search = (!(source.match_redacted || source.key_blocks_redacted))
        .then(|| q.match_strings().first().map(|text| (*text).to_owned()))
        .flatten()
        .map(|text| {
            let dir = Path::new(q.path.as_str())
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .filter(|dir| !dir.is_empty())
                .unwrap_or_else(|| ".".to_owned());
            let mut row = serde_json::json!({"path": dir, "matchString": text});
            if q.is_regex() {
                row["regex"] = serde_json::json!("rust");
            }
            crate::tools::result::Continuation::new(ToolId::LocalSearch, row)
                .why("Search the file's directory for the text.")
                .build()
        });
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(String::new()),
        content_view: Some(MinifyMode::None),
        next: Some(NextCalls {
            text_search: search,
            ignore_case,
            ..NextCalls::default()
        })
        .filter(|next| !next.is_empty()),
        hints: vec![hint],
        total_lines: Some(source.total_lines),
        selected_match_count: Some(0),
        source_chars: Some(source.chars),
        source_bytes: Some(source.bytes),
        returned_chars: Some(0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        ..LocalFetchResult::blank("empty")
    }
}

/// The selected text in the view it is returned in.
struct View {
    text: String,
    mode: MinifyMode,
    fallback: Option<MinifyFallback>,
}

/// Apply the requested `minify` view to the extracted text. A match read
/// stays verbatim evidence; an outline the file type cannot give falls
/// back to the standard view.
fn select_view(
    q: &LocalFetchQuery,
    ext: &mut super::extraction::Extraction,
    warnings: &mut Vec<String>,
) -> View {
    let requested = q.minify_mode();
    let selected = std::mem::take(&mut ext.text);
    if q.match_string.is_some() && requested != MinifyMode::None {
        return View {
            text: selected,
            mode: MinifyMode::None,
            fallback: Some(MinifyFallback {
                requested,
                applied: MinifyMode::None,
                reason: "match-evidence".into(),
            }),
        };
    }
    // Every transformed view keeps the citation gutter (`N\t`): the standard
    // view numbers each kept line with its source line, the outline uses the
    // same separator in every language.
    let standard_view = |selected: &str| {
        let view = octocode_engine::portable::apply_content_view_minification(selected, &q.path);
        crate::tools::numbered::number_view(selected, ext.source_lines.as_deref(), &view)
    };
    let view = |text, mode| View {
        text,
        mode,
        fallback: None,
    };
    match requested {
        MinifyMode::None => view(selected, MinifyMode::None),
        MinifyMode::Standard => view(standard_view(&selected), MinifyMode::Standard),
        MinifyMode::Symbols => {
            if let Some(s) = octocode_engine::portable::extract_signatures(&selected, &q.path) {
                let outline =
                    octocode_engine::portable::apply_content_view_minification(&s, &q.path);
                view(
                    declaration_ranges(
                        crate::tools::numbered::tab_gutter(&outline),
                        &selected,
                        &q.path,
                    ),
                    MinifyMode::Symbols,
                )
            } else if let Some(outline) =
                crate::content::markdown_heading_outline(&selected, &q.path)
            {
                view(
                    declaration_ranges(
                        crate::tools::numbered::tab_gutter(&outline),
                        &selected,
                        &q.path,
                    ),
                    MinifyMode::Symbols,
                )
            } else {
                warnings.push(format!("No smaller outline is available for {}; using the standard content view. The outline may be unsupported, oversized, or the source may be minified/bundled (single giant lines) — read specific line ranges instead.",q.path));
                View {
                    text: standard_view(&selected),
                    mode: MinifyMode::Standard,
                    fallback: Some(MinifyFallback {
                        requested: MinifyMode::Symbols,
                        applied: MinifyMode::Standard,
                        reason: "outline-unavailable".into(),
                    }),
                }
            }
        }
    }
}

/// An outline whose declaration heads carry their line span: the gutter
/// of a multi-line declaration's first shown line reads `start-end`, so a
/// body is one `ranges` read away. The gutter is metadata, not source text;
/// the TAB still separates the source.
fn declaration_ranges(outline: String, source: &str, path: &str) -> String {
    let Some(spans) = super::block::declaration_spans(source, path) else {
        return outline;
    };
    let mut ends = std::collections::HashMap::<usize, usize>::new();
    for (start, end) in spans {
        let entry = ends.entry(start).or_insert(end);
        *entry = (*entry).max(end);
    }
    let mut out = String::with_capacity(outline.len() + ends.len() * 6);
    for record in outline.split_inclusive('\n') {
        let head = record
            .split_once(crate::tools::numbered::SEPARATOR)
            .and_then(|(number, _)| number.parse::<usize>().ok())
            .and_then(|line| Some((line, ends.remove(&line)?)));
        match head {
            Some((line, end)) => {
                out.push_str(&format!("{line}-{end}"));
                out.push_str(&record[line.to_string().len()..]);
            }
            None => out.push_str(record),
        }
    }
    out
}

/// One returned page after the secret scan.
struct ScannedPage {
    page: super::pagination::Page,
    redacted: bool,
    /// The scanned view is empty.
    empty: bool,
    /// The scan kept the view's line count, so the page maps onto source lines.
    lines_kept: bool,
    warnings: Vec<String>,
}

/// Cut the page and scan it for secrets. A line or byte page is scanned on
/// its own window (see `sanitize_line_page` / `sanitize_byte_page`); a
/// complete view, or a window whose scan fails, takes the whole-view scan,
/// which owns the typed security-limit recovery.
fn scan_page(
    q: &LocalFetchQuery,
    page_q: &LocalFetchQuery,
    selected: &str,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    source: &Source,
    first_line: Option<usize>,
) -> Result<ScannedPage, Box<LocalFetchResult>> {
    let pagination_error = |e| {
        Box::new(LocalFetchResult::error(
            q.path.to_string(),
            "invalidPagination",
            e,
        ))
    };
    if q.full_content != Some(true) {
        let windowed = match page(selected, page_q).map_err(pagination_error)? {
            raw if raw.pagination.unit == WindowUnit::Lines => {
                sanitize_line_page(selected, raw, source_path, security).unwrap_or(None)
            }
            raw => sanitize_byte_page(selected, raw, source_path, security).unwrap_or(None),
        };
        // Page sanitizers keep line counts, so a redacted page still maps
        // onto source lines.
        if let Some((page, redacted)) = windowed {
            return Ok(ScannedPage {
                page,
                redacted,
                empty: selected.is_empty(),
                lines_kept: true,
                warnings: vec![],
            });
        }
    }
    let (safe, warnings) = match security.sanitize(selected, source_path) {
        Ok(scanned) => scanned,
        Err((code, _)) if code == "contentSecurityLimit" => {
            return Err(Box::new(security_limit(q, &code, source, first_line)));
        }
        Err((code, message)) => {
            return Err(Box::new(LocalFetchResult::error(
                q.path.to_string(),
                &code,
                message,
            )));
        }
    };
    let page = page(&safe, page_q).map_err(pagination_error)?;
    // A whole-view scan maps onto source lines only when it kept the count.
    let lines_kept = safe == selected || line_count(&safe) == line_count(selected);
    Ok(ScannedPage {
        page,
        redacted: safe != selected,
        empty: safe.is_empty(),
        lines_kept,
        warnings,
    })
}

/// The selected view is too large to scan: a terminal row that offers one
/// source line instead.
fn security_limit(
    q: &LocalFetchQuery,
    code: &str,
    source: &Source,
    first_line: Option<usize>,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        code,
        "The selected content view exceeds the secret scanner size limit. Byte windows cannot safely split unscanned content. Select a smaller source-line range.".into(),
    );
    result.path = q.path.to_string();
    result.total_lines = Some(source.total_lines);
    result.source_chars = Some(source.chars);
    result.source_bytes = Some(source.bytes);
    result.is_partial = Some(true);
    result.terminal_limit = Some(true);
    result.partial_reasons = vec![PartialReason::SecuritySelectedView];
    if source.total_lines > 1 && (q.start_line().is_none() || q.start_line() != q.end_line()) {
        let line = first_line.or(q.start_line()).unwrap_or(1);
        result.next = Some(NextCalls {
            read_bounded_lines: Some(Continuation {
                query: single_line_query(q, line),
                reason: Some("The selected view is too large to scan safely. Read one source line; this starts a different source-line view, not a…".into()),
            }),
            ..NextCalls::default()
        });
    }
    result
}

/// Redacted text is not the source: say so, as localSearch does. Count only
/// when a redaction pass ran, so literal placeholder text in a clean file is
/// not misreported.
fn warn_redacted(text: &str, view_redacted: bool, source: &Source, warnings: &mut Vec<String>) {
    let redactions = if view_redacted || source.match_redacted || source.key_blocks_redacted {
        text.matches("[REDACTED").count()
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
}

/// The page's own continuation: the next page, or a restart for an offset
/// past the end of the view.
fn page_next(
    q: &LocalFetchQuery,
    pg: &super::pagination::Page,
    head: bool,
    warnings: &mut Vec<String>,
) -> Option<NextCalls> {
    if !pg.out_of_range {
        let mut next = continuation(q, &pg.pagination);
        // Paging on from the head uses the default page size.
        if head && let Some(read) = next.as_mut().and_then(|next| next.r#continue.as_mut()) {
            read.query.length = None;
            read.query.unit = None;
        }
        return next;
    }
    let (total, unit) = match pg.pagination.unit {
        WindowUnit::Lines => (pg.pagination.total_lines, "lines"),
        WindowUnit::Bytes => (pg.pagination.total_bytes, "bytes"),
    };
    warnings.push(format!(
        "offset {} is past the end of the selected view ({total} {unit}); nothing was returned. Follow next.restart to read from the start.",
        q.offset().unwrap_or(0),
    ));
    let mut query = q.clone();
    query.offset = Some(0);
    Some(NextCalls {
        restart: Some(Continuation {
            query,
            reason: Some(
                "Offset is past the end of the selected view; restart from offset 0.".into(),
            ),
        }),
        ..NextCalls::default()
    })
}

/// Notes for a read with no anchor: a large file's head, or a mid-size
/// file's first page. Rows with content keep no prose hint, so these are
/// warnings.
fn read_scope_warnings(
    q: &LocalFetchQuery,
    pg: &super::pagination::Page,
    head: bool,
    mode: MinifyMode,
    total_lines: usize,
    warnings: &mut Vec<String>,
) {
    if head && pg.pagination.has_more {
        warnings.push(format!(
            "Large file ({total_lines} lines) read without an anchor: returned its first {} lines. Target the answer with matchString or ranges; next.continue pages on, fullContent:true reads it whole.",
            pg.view_lines.1
        ));
    }
    if !head
        && !pg.out_of_range
        && pg.pagination.has_more
        && mode == MinifyMode::None
        && unanchored_first_read(q)
    {
        warnings.push(format!(
            "Unanchored read: lines {}-{} of {total_lines}. Read the deciding lines with matchString or ranges.",
            pg.view_lines.0, pg.view_lines.1
        ));
    }
}

/// The source lines a page shows: through the view-to-source line map when
/// the extraction kept one, else the page's own line span.
fn page_source_ranges(
    source_lines: Option<&[usize]>,
    view_lines: (usize, usize),
) -> Vec<LineRange> {
    let Some(lines) = source_lines else {
        return vec![LineRange {
            start: view_lines.0,
            end: view_lines.1,
        }];
    };
    let page_lines: Vec<usize> = lines
        [view_lines.0.saturating_sub(1)..view_lines.1.min(lines.len())]
        .iter()
        .copied()
        .filter(|line| *line != super::extraction::OMISSION_LINE)
        .collect();
    compress_ranges(&page_lines)
}

/// A first read with nothing selecting what to return: no match, range,
/// block, page cursor, page size, or whole-file request.
fn unanchored_first_read(q: &LocalFetchQuery) -> bool {
    q.match_string.is_none()
        && !q.has_ranges()
        && q.start_line().is_none()
        && q.end_line().is_none()
        && !q.block()
        && q.full_content != Some(true)
        && q.offset().is_none()
        && q.window_length().is_none()
        && q.unit.is_none()
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
    let mut query = LocalFetchQuery {
        minify: Some(MinifyMode::None),
        full_content: None,
        match_string: None,
        regex: None,
        case_mode: None,
        context_lines: None,
        context_bytes: None,
        unit: None,
        offset: None,
        length: None,
        ..q.clone()
    };
    query.set_line_span(line, line);
    query
}

/// The continued source changed since its page was cut. Pages of two file
/// versions must not be combined; restart the same view from the start.
fn stale_snapshot(q: &LocalFetchQuery) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "staleSnapshot",
        crate::response::pages::STALE_SNAPSHOT_ERROR.into(),
    );
    let mut query = q.clone();
    query.snapshot = None;
    query.offset = None;
    result.next = Some(NextCalls {
        r#continue: None,
        read_bounded_lines: None,
        restart: Some(Continuation {
            query,
            reason: Some("The source changed; restart this view on the current version.".into()),
        }),
        ..NextCalls::default()
    });
    result
}

/// The first bounded line page of the same view as a complete read.
fn bounded_query(q: &LocalFetchQuery) -> LocalFetchQuery {
    let mut query = q.clone();
    query.full_content = None;
    query.unit = Some(WindowUnit::Lines);
    query.offset = Some(0);
    query.length = wire_positive(DEFAULT_LINE_CHUNK);
    query
}

pub(crate) fn compress_ranges(lines: &[usize]) -> Vec<LineRange> {
    crate::tools::line_spans::runs(lines.iter().copied())
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}
fn system_time_iso(t: std::time::SystemTime) -> Option<String> {
    let elapsed = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(crate::civil_date::iso8601_millis(
        i64::try_from(elapsed.as_millis()).ok()?,
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

    use super::super::tests::{Paths, Temp, q};
    use crate::security::scan::Passthrough;

    /// Read `req` under `dir` with a passthrough scan.
    fn fetch(req: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
        execute_local_fetch(
            req,
            paths,
            &Passthrough,
            &NeverCancel,
            &LocalFetchRegex::default(),
            None,
        )
    }

    #[test]
    fn source_above_the_streaming_ceiling_returns_file_too_large() {
        let temp = Temp::new();
        let dir = temp.0.clone();
        let path = dir.join("huge.txt");
        // Sparse file just past the streaming ceiling — the size guard fires
        // before any bytes are read, so this stays cheap.
        let file = fs::File::create(&path).expect("create file");
        let len = super::super::large_source::MAX_STREAM_SOURCE_BYTES + 1;
        file.set_len(len).expect("grow file");
        drop(file);

        let req = q(&path);
        let result = fetch(&req, &Paths(dir.clone()));

        assert_eq!(result.status, "error");
        assert_eq!(result.error_code.as_deref(), Some("fileTooLarge"));
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
        let bytes =
            read_in_memory_source(std::io::Cursor::new(source), Vec::new()).expect("bounded read");
        assert_eq!(bytes.len() as u64, MAX_SOURCE_BYTES + 1);
        let result = source_too_large(
            "growing.txt",
            bytes.len() as u64,
            SourceSizeLimit::InMemoryGrowth,
        );
        assert_eq!(result.error_code.as_deref(), Some("fileTooLarge"));
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

    // Past the whole-file ceiling a plain read streams a bounded line window:
    // exact whole-file totals, a digest-bound continuation that advances,
    // and matchString redirected to localSearch.
    #[test]
    fn oversized_text_source_is_served_as_streamed_line_windows() {
        let temp = Temp::new();
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
            ranges: vec![format!("{}-{}", 1, 3_000).parse().expect("range")],
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
            ranges: vec![format!("{}-{}", lines, lines).parse().expect("range")],
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
        // The localSearch is an executable lead, and the tip stays short.
        let search = matched
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("textSearch lead");
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
        let temp = Temp::new();
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
                ..LocalFetchQuery::test_default()
            },
            &Paths(dir.clone()),
        );
        assert_eq!(result.status, "success", "{result:?}");
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

    // A line chunk of an oversized source larger than one response page
    // pages inside the chunk and then goes on past it: every line from the
    // first is reached once, in order, across the chunk boundary.
    #[test]
    fn oversized_source_line_chunks_continue_past_a_paged_chunk() {
        let temp = Temp::new();
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
            length: super::super::types::wire_positive(1_000),
            ..LocalFetchQuery::test_default()
        };
        let mut next_line = 1usize;
        for _ in 0..12 {
            let page = fetch(&query, &Paths(dir.clone()));
            assert_eq!(page.status, "success", "{page:?}");
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
                .and_then(|next| next.r#continue.clone())
                .unwrap_or_else(|| panic!("no continuation after line {}", next_line - 1))
                .query;
        }
        assert!(next_line > 2_001, "walked past two chunks: {next_line}");
    }

    #[test]
    fn at_limit_plain_read_is_allowed() {
        let temp = Temp::new();
        let dir = temp.0.clone();
        let path = dir.join("ok.txt");
        fs::write(&path, "hello\nworld\n").expect("write file");

        let req = q(&path);
        let result = fetch(&req, &Paths(dir.clone()));

        assert_eq!(result.status, "success");
        assert_eq!(result.error_code, None);
    }

    // A bounded read of an interior body line of a private key must not
    // leak the key, even when the file is NOT key-named and the selected window
    // contains no BEGIN/END marker (so the anchored full-block patterns cannot
    // fire). The `Passthrough` scan, proving the full-file block guard
    // — not the window sanitizer — closes the leak.
    #[test]
    fn interior_private_key_window_does_not_leak_in_non_key_named_file() {
        let temp = Temp::new();
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
            ..LocalFetchQuery::test_default()
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
        let temp = Temp::new();
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
                match_string: Some(needle.parse().expect("match string")),
                context_lines: Some(0),
                ..LocalFetchQuery::test_default()
            };
            execute_local_fetch(
                &req,
                &Paths(dir.clone()),
                &security,
                &NeverCancel,
                &LocalFetchRegex::default(),
                None,
            )
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
            &LocalFetchRegex::default(),
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
        let temp = Temp::new();
        let dir = temp.0.clone();
        let path = dir.join("data.txt");
        let blob = "aGVsbG8gd29ybGQgdGhpcyBpcyBqdXN0IGEgbG9uZyBiYXNlNjQgYmxvYg==";
        let file = format!("header\n{blob}\nfooter\n");
        fs::write(&path, &file).expect("write file");

        let req = LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![format!("{}-{}", 2, 2).parse().expect("range")],
            ..LocalFetchQuery::test_default()
        };
        let result = fetch(&req, &Paths(dir.clone()));

        let content = result.content.clone().unwrap_or_default();
        assert!(
            content.contains(blob),
            "innocent base64 was wrongly redacted: {content}"
        );
    }
}

/// Why a matchString can miss text that is visibly in the file.
const REDACTED_MATCH_HINT: &str = "No visible line matches; this file has secrets, and matchString runs on [REDACTED…] placeholders, never secret text.";

/// Next step for a matchString that selected no line, tuned to how it matched.
/// `finder` names the search tool that locates the file containing the text.
/// Kept under the 120-char guidance cap.
pub(crate) fn no_match_hint(regex: bool, case_sensitive: bool, finder: &str) -> String {
    match (regex, case_sensitive) {
        (false, false) => {
            format!("No line contains this text; try a shorter token, regex:\"rust\", or {finder}.")
        }
        (false, true) => {
            format!("No line contains this case-sensitive text; run hints.ignoreCase, or {finder}.")
        }
        (true, false) => {
            format!("No line matches this regex (^/$ per line); simplify it or use {finder}.")
        }
        (true, true) => {
            format!("No line matches this case-sensitive regex; run hints.ignoreCase, or {finder}.")
        }
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
            &SourceFacts::default(),
            &security(),
            &NeverCancel,
            &LocalFetchRegex::default(),
        )
    }
    fn lines_query(offset: usize, chunk: usize) -> LocalFetchQuery {
        LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            unit: Some(WindowUnit::Lines),
            offset: Some(wire_count(offset)),
            length: wire_positive(chunk),
            ..LocalFetchQuery::test_default()
        }
    }
    fn filler(n: usize) -> String {
        (1..=n)
            .map(|i| format!("const filler_{i} = {i};\n"))
            .collect()
    }

    /// A large file read with no anchor returns a short head instead of a
    /// full page: the read is not targeted yet. Paging on (default page
    /// size) and whole-file reads are unchanged; small files and anchored
    /// reads keep their pages.
    #[test]
    fn an_unanchored_read_of_a_large_file_returns_a_head() {
        let source = filler(LARGE_READ_LINES);
        let plain = LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            ..LocalFetchQuery::test_default()
        };
        let head = fetch(&source, &plain);
        assert_eq!(head.returned_lines, Some(HEAD_LINES), "{head:?}");
        assert!(
            head.warnings
                .iter()
                .any(|warning| warning.contains("first 50 lines")),
            "{:?}",
            head.warnings
        );
        let next = head
            .next
            .and_then(|next| next.r#continue)
            .expect("continue");
        assert_eq!(next.query.offset(), Some(HEAD_LINES));
        assert_eq!(next.query.window_length(), None);
        let page = fetch(&source, &next.query);
        assert!(page.returned_lines.expect("lines") > HEAD_LINES, "{page:?}");
        // Whole-file, anchored, and small reads are unchanged.
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
        let small = fetch(&filler(LARGE_READ_LINES - 1), &plain);
        assert!(small.returned_lines.expect("lines") > HEAD_LINES);
        // A smaller file keeps its full page; only the deciding-lines note.
        assert!(
            small
                .warnings
                .iter()
                .all(|warning| warning.starts_with("Unanchored read: lines 1-")),
            "{:?}",
            small.warnings
        );
    }

    /// A mid-size file read without an anchor returns its first page with a
    /// note toward the deciding lines; complete and anchored reads get none.
    #[test]
    fn an_unanchored_partial_read_points_at_the_deciding_lines() {
        let plain = LocalFetchQuery {
            path: "/fixture/app.ts".parse().expect("path"),
            ..LocalFetchQuery::test_default()
        };
        let page = fetch(&filler(LARGE_READ_LINES - 1), &plain);
        assert!(page.next.is_some(), "{page:?}");
        let tip = page
            .warnings
            .iter()
            .find(|warning| warning.starts_with("Unanchored read: lines 1-"))
            .unwrap_or_else(|| panic!("{:?}", page.warnings));
        assert!(tip.contains("matchString") && tip.len() <= 120, "{tip}");
        let whole = fetch(&filler(20), &plain);
        assert!(whole.warnings.is_empty(), "{:?}", whole.warnings);
        let ranged = fetch(
            &filler(LARGE_READ_LINES - 1),
            &LocalFetchQuery {
                ranges: vec![format!("{}-{}", 10, 20).parse().expect("range")],
                ..plain.clone()
            },
        );
        assert!(ranged.warnings.is_empty(), "{:?}", ranged.warnings);
    }

    /// The head warning names only calls every surface has: a clasify lead,
    /// when one applies, is its own `hints.clasify` entry.
    #[test]
    fn the_large_file_head_warning_names_no_optional_tool() {
        let head = fetch(
            &filler(LARGE_READ_LINES),
            &LocalFetchQuery {
                path: "/fixture/app.ts".parse().expect("path"),
                ..LocalFetchQuery::test_default()
            },
        );
        let warning = head
            .warnings
            .iter()
            .find(|warning| warning.contains("without an anchor"))
            .expect("head warning");
        assert!(warning.contains("matchString"), "{warning}");
        assert!(!warning.contains("clasify"), "{warning}");
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
            unit: Some(WindowUnit::Bytes),
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
            &SourceFacts::default(),
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
            unit: Some(WindowUnit::Bytes),
            ..lines_query(source.len(), 100)
        };
        let result = fetch(&source, &bytes);
        assert!(result.out_of_range && result.source_line_ranges.is_empty());
        // The last real page is not out of range.
        assert!(!fetch(&source, &lines_query(400, 100)).out_of_range);
    }
}
