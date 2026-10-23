//! Secret safety of shown values: redaction of the scan, the re-read that
//! checks clipped values against their source bytes, and bounded line reads.

use super::types::*;
use super::{cursor::*, layout::*};
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use octocode_engine::signatures::MAX_PARSE_SIZE;
use sha2::{Digest, Sha256};

/// Upper bound (bytes) on a file re-read for the private-key block scan.
/// Matches the secret scanner's own content cap; larger files fall back to the
/// per-match window sanitizer rather than pay an unbounded read.
pub(super) const MAX_KEY_SCAN_BYTES: u64 = 10 * 1024 * 1024;

/// What a stored scan's file must still hash to (see `expected_digest`).
pub(super) type ExpectedDigest<'a> =
    dyn Fn(&std::path::Path) -> Option<Option<super::manifest::Digest>> + Sync + 'a;

/// What every stage of one page reads besides the scan: the query, the read
/// policy, the root the rows name paths under, the bytes a stored scan's
/// checks must read, the secret scanner and the cancel.
pub(super) struct PageScope<'a, C> {
    pub(super) query: &'a LocalSearchQuery,
    pub(super) paths: &'a crate::policy::path::PathPolicy,
    pub(super) output_root: &'a std::path::Path,
    pub(super) expected_digest: &'a ExpectedDigest<'a>,
    pub(super) security: &'a ContentSecurity,
    pub(super) cancel: &'a C,
    /// This query's share of the cores for the page's parallel reads (see
    /// [`crate::tools::workers::workers`]).
    pub(super) walk_threads: Option<u32>,
}

/// (path, line, column) of every match whose value secret redaction
/// changed: the returned text is then not verbatim source, and the row says
/// so via a `redactedMatches` warning.
pub(super) type Redacted = std::collections::HashSet<(String, u32, u32)>;

/// What redacting a fresh scan changed, so a later page of the same stored
/// scan replays it instead of sanitizing every value again. Sanitizing is a
/// pure function of a value and its source path; the only file input is a
/// key-block scan, so the record keeps the digest of each such read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redaction {
    /// The root the scan's paths were made relative to.
    pub(super) output_root: std::path::PathBuf,
    /// Files whose snippets looked like key material, by scan index, with
    /// the digest of the bytes their key-block scan read (`None`: no read).
    pub(super) key_files: Vec<(usize, Option<super::manifest::Digest>)>,
    /// Values the redaction replaced: file index, match index, new value.
    pub(super) changes: Vec<(usize, usize, String)>,
}

impl Redaction {
    /// Bytes the record adds to a stored scan.
    pub(super) fn weight(&self) -> usize {
        self.output_root.as_os_str().len()
            + self.key_files.len() * 40
            + self
                .changes
                .iter()
                .map(|(_, _, value)| value.len() + 16)
                .sum::<usize>()
    }
}

/// Make paths relative to `output_root` and redact secrets in every value.
/// A match on an interior base64 body line of a private key would leak the
/// key even though the match view holds no BEGIN/END marker (the anchored
/// built-in patterns need a complete block). Only when a snippet actually
/// looks like key material is the full file scanned for private-key block
/// ranges; matches whose window intersects a block are redacted. Innocent
/// base64 triggers a scan that finds no block and redacts nothing. A file
/// is read only after it passes the read policy.
///
/// `stored` is the redaction a fresh page made of this stored scan: it is
/// replayed when every key-block scan reads the same bytes again (see
/// [`replay_redaction`]); otherwise every value is redacted afresh. Returns
/// the redacted rows and the record of this redaction.
pub(super) fn redact_scan(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    stored: Option<&Redaction>,
) -> Result<(Redacted, Redaction), ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        expected_digest,
        cancel,
        ..
    } = *scope;
    if let Some(stored) = stored.filter(|stored| stored.output_root == output_root) {
        cancel.check().map_err(ToolError::cancelled)?;
        if let Some(redacted) =
            replay_redaction(query, parsed, output_root, expected_digest, paths, stored)?
        {
            return Ok((redacted, stored.clone()));
        }
    }
    let mut redacted = Redacted::new();
    let mut record = Redaction {
        output_root: output_root.to_path_buf(),
        ..Redaction::default()
    };
    // Files redact independently: on parallel workers, merged in file order
    // so the record (and the first failure) is the one a serial pass makes.
    let workers = crate::tools::workers::workers(scope.walk_threads, parsed.files.len(), None);
    let outcomes = crate::tools::workers::each_parallel(&mut parsed.files, workers, |file| {
        redact_file(scope, file)
    });
    for (file_index, outcome) in outcomes.into_iter().enumerate() {
        let FileRedaction {
            key_file,
            changes,
            path,
        } = outcome?;
        if let Some(digest) = key_file {
            record.key_files.push((file_index, digest));
        }
        for (match_index, line, column, value) in changes {
            redacted.insert((path.clone(), line, column));
            record.changes.push((file_index, match_index, value));
        }
    }
    Ok((redacted, record))
}

/// What redacting one file changed.
struct FileRedaction {
    /// Its key-block scan, with the digest of the bytes it read.
    key_file: Option<Option<super::manifest::Digest>>,
    /// The values replaced: match index, line, column, new value.
    changes: Vec<(usize, u32, u32, String)>,
    /// The file's row path.
    path: String,
}

/// Make one file's path relative to the output root and redact secrets in
/// its values (see [`redact_scan`]).
fn redact_file(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
) -> Result<FileRedaction, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        expected_digest,
        security,
        ..
    } = *scope;
    if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
        file.path = relative.to_string_lossy().into_owned();
    }
    let mut key_material = false;
    visit_values(scope, file, 0..file.matches.len(), |_, hit| {
        key_material |= crate::security::snippet_may_hold_key_material(&hit.value);
        if hit.deferred.is_some() {
            hit.value = String::new();
        }
        Ok(())
    })?;
    let source_path = output_root.join(&file.path);
    let mut key_file = None;
    let key_source = if key_material {
        validate_matched(paths, &source_path)?;
        let bytes = read_key_scan_bytes(&source_path);
        let digest = bytes
            .as_ref()
            .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)));
        key_file = Some(digest);
        if let (Some(digest), Some(expected)) = (digest, expected_digest(&source_path))
            && expected != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        bytes
            .map(|bytes| {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                (crate::security::private_key_block_line_ranges(&text), text)
            })
            .filter(|(ranges, _)| !ranges.is_empty())
    } else {
        None
    };
    let key_lines = key_source
        .as_ref()
        .map(|(ranges, text)| (ranges, text.lines().collect::<Vec<_>>()));
    let mut changes = Vec::new();
    visit_values(
        scope,
        file,
        0..file.matches.len(),
        |match_index, matched| {
            let changed = if let Some((ranges, lines)) = &key_lines
                && crate::security::match_window_intersects_key_block(
                    matched.line,
                    &matched.value,
                    ranges,
                ) {
                let kept = redact_key_block_lines(matched.line, &matched.value, ranges, |n| {
                    lines.get((n as usize).checked_sub(1)?).copied()
                });
                let sanitized = security.sanitize_text(&kept, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = sanitized.content;
                changed
            } else {
                let sanitized = security.sanitize_text(&matched.value, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = sanitized.content;
                changed
            };
            if matched.deferred.is_some() {
                matched.deferred = None;
                let price = super::layout::row_chars(matched, None, false);
                matched.deferred = Some(u32_of_usize(price));
                if !changed {
                    matched.value = String::new();
                }
            }
            if changed {
                changes.push((
                    match_index,
                    matched.line,
                    matched.column,
                    matched.value.clone(),
                ));
            }
            Ok(())
        },
    )?;
    Ok(FileRedaction {
        key_file,
        changes,
        path: file.path.clone(),
    })
}

/// `value` (a match row whose `line` is the match) with each of its lines
/// that lies inside a private-key block replaced by the key placeholder,
/// so context lines beside a block stay readable. The value's first source
/// line is found by aligning its lines with `source` (1-based lines); a
/// line is redacted when any alignment puts it inside a block. A value no
/// alignment fits is redacted whole: fail closed.
fn redact_key_block_lines<'a>(
    line: u32,
    value: &str,
    ranges: &[(u32, u32)],
    source: impl Fn(u32) -> Option<&'a str>,
) -> String {
    let placeholder = crate::security::key_fragment_placeholder();
    let value_lines: Vec<&str> = value.lines().collect();
    let in_block = |n: u32| ranges.iter().any(|&(s, e)| s <= n && n <= e);
    // A line a sanitizer already redacted fits where its unredacted
    // segments do.
    let fits = |value_line: &str, n: u32| {
        let core = strip_clip_markers(value_line).trim();
        source(n).is_some_and(|source| {
            octocode_engine::security::placeholder_regex()
                .split(core)
                .map(str::trim)
                .all(|segment| segment.is_empty() || source.contains(segment))
        })
    };
    let mut redact = vec![false; value_lines.len()];
    let mut aligned = false;
    for offset in 0..value_lines.len().max(1) as u32 {
        let Some(first) = line.checked_sub(offset).filter(|first| *first >= 1) else {
            break;
        };
        if !value_lines
            .iter()
            .enumerate()
            .all(|(i, value_line)| fits(value_line, first + i as u32))
        {
            continue;
        }
        aligned = true;
        for (i, flag) in redact.iter_mut().enumerate() {
            *flag |= in_block(first + i as u32);
        }
    }
    if !aligned {
        return placeholder;
    }
    let mut out = value_lines
        .iter()
        .zip(&redact)
        .map(|(value_line, redact)| {
            if *redact {
                placeholder.as_str()
            } else {
                value_line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if value.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The bytes a key-block scan reads: the whole file within
/// [`MAX_KEY_SCAN_BYTES`], or `None`.
fn read_key_scan_bytes(source: &std::path::Path) -> Option<Vec<u8>> {
    octocode_engine::text::read_source(source, MAX_KEY_SCAN_BYTES)
        .ok()
        .map(|source| source.bytes)
}

/// Replay `stored` on the stored scan it was made from. Its key-block scans
/// read each file again, under the read policy and bound to the stored
/// digest exactly as a fresh redaction is (a changed file restarts the
/// snapshot). When each read hashes to the bytes the record was made from,
/// the key-block ranges, and so every replaced value, are the ones a fresh
/// redaction computes; `None` (no read, other bytes) redacts afresh.
fn replay_redaction(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    paths: &crate::policy::path::PathPolicy,
    stored: &Redaction,
) -> Result<Option<Redacted>, ToolError> {
    for &(index, made_from) in &stored.key_files {
        let Some(file) = parsed.files.get(index) else {
            return Ok(None);
        };
        let source_path = output_root.join(&file.path);
        validate_matched(paths, &source_path)?;
        let Some(bytes) = read_key_scan_bytes(&source_path) else {
            return Ok(None);
        };
        let digest = <[u8; 32]>::from(Sha256::digest(&bytes));
        if let Some(expected) = expected_digest(&source_path)
            && expected != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        if made_from != Some(digest) {
            return Ok(None);
        }
    }
    if stored.changes.iter().any(|&(file, row, _)| {
        parsed
            .files
            .get(file)
            .is_none_or(|file| row >= file.matches.len())
    }) {
        return Ok(None);
    }
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
    }
    let mut redacted = Redacted::new();
    for (file, row, value) in &stored.changes {
        let file = &mut parsed.files[*file];
        let matched = &mut file.matches[*row];
        matched.value.clone_from(value);
        redacted.insert((file.path.clone(), matched.line, matched.column));
    }
    Ok(Some(redacted))
}

/// A matched file must pass the read policy before a page reads it or shows
/// its rows.
pub(super) fn validate_matched(
    paths: &crate::policy::path::PathPolicy,
    source: &std::path::Path,
) -> Result<(), ToolError> {
    paths.validate_read(source).map(drop).map_err(|error| {
        ToolError::new(
            error.shared_code(),
            "Search encountered a path denied by the active path policy",
        )
    })
}

pub(super) fn verify_candidate(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    redacted: &mut Redacted,
) -> Result<bool, ToolError> {
    scope.cancel.check().map_err(ToolError::cancelled)?;
    let source = scope.output_root.join(&file.path);
    validate_matched(scope.paths, &source)?;
    materialize(scope, file, 0..file.matches.len())?;
    let before = file
        .matches
        .iter()
        .map(|matched| matched.value.clone())
        .collect::<Vec<_>>();
    let rows = 0..file.matches.len();
    let verification = guard_clipped_secrets(
        file,
        &source,
        (scope.expected_digest)(&source),
        rows,
        &ValueCheck {
            security: scope.security,
            match_only: scope.query.result_view == LocalSearchQueryResultView::MatchOnly,
            cancel: scope.cancel,
        },
    )
    .map_err(ToolError::cancelled)?;
    for (matched, before) in file.matches.iter_mut().zip(before) {
        if matched.value != before {
            matched.deferred = None;
            redacted.insert((file.path.clone(), matched.line, matched.column));
        }
    }
    match verification {
        Verification::Verified => Ok(true),
        Verification::Unverified => Ok(false),
        Verification::Changed => Err(changed_since_scan(scope.query)),
    }
}

/// Outlines of the page's first shown files, keyed by source path, parsed
/// from the bytes their secret check read (`None`: no outline).
pub(super) type Outlines =
    std::collections::HashMap<std::path::PathBuf, Option<super::enclosing::Outline>>;

/// Re-read the shown files. A page from a stored scan shows each of its
/// files only while that file still hashes to the stored bytes; text views
/// prove it in the secret check, which reads the same bytes, and every
/// shown file with rows is outlined into `outlines`:
/// taken from the page cut's outlines (`cut`) when it parsed the file, else
/// parsed from the secret check's read. Returns whether some values were
/// redacted because their source could not be re-read.
pub(super) fn verify_shown(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    layout: &Layout,
    redacted: &mut Redacted,
    (cut, outlines): (&mut CutOutlines, &mut Outlines),
) -> Result<bool, ToolError> {
    let PageScope {
        query,
        output_root,
        expected_digest,
        security,
        cancel,
        ..
    } = *scope;
    let check = ValueCheck {
        security,
        match_only: query.result_view == LocalSearchQueryResultView::MatchOnly,
        cancel,
    };
    if layout.list {
        for (index, _) in &layout.shown {
            let source = output_root.join(&parsed.files[*index].path);
            if let Some(expected) = expected_digest(&source)
                && (expected.is_none() || expected != super::manifest::digest_file(&source).ok())
            {
                return Err(changed_since_scan(query));
            }
        }
        return Ok(false);
    }
    let mut unverified = false;
    // Outlines are parsed after the checks, in parallel: one tree-sitter
    // parse per shown file was the page's slowest serial step.
    let mut to_outline: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    for (index, rows) in &layout.shown {
        let file = &mut parsed.files[*index];
        materialize(scope, file, rows.clone())?;
        cancel.check().map_err(ToolError::cancelled)?;
        // The check rewrites only the shown rows, so only they are compared.
        let end = rows.end.min(file.matches.len());
        let start = rows.start.min(end);
        let before = file.matches[start..end]
            .iter()
            .map(|matched| matched.value.clone())
            .collect::<Vec<_>>();
        let source = output_root.join(&file.path);
        // The page's first files are outlined for enclosing names: read
        // each once, for both its secret check and its outline.
        let shows_rows = rows.start.min(file.matches.len()) < rows.end.min(file.matches.len());
        let outlined = shows_rows;
        let parsed_by_cut = outlined.then(|| cut.remove(index)).flatten();
        let bytes = (outlined && parsed_by_cut.is_none())
            .then(|| crate::tools::source::read_bounded(&source, MAX_PARSE_SIZE).ok())
            .flatten();
        let expected = expected_digest(&source);
        let verification = match &bytes {
            Some(bytes) => guard_read(
                file,
                &source,
                || Ok(bytes.as_slice()),
                expected,
                rows.clone(),
                &check,
            ),
            None => guard_clipped_secrets(file, &source, expected, rows.clone(), &check),
        };
        match verification.map_err(ToolError::cancelled)? {
            Verification::Verified => {}
            Verification::Unverified => unverified = true,
            Verification::Changed => return Err(changed_since_scan(query)),
        }
        match (parsed_by_cut, bytes) {
            (Some(outline), _) => {
                outlines.insert(source, outline);
            }
            (None, Some(bytes)) => to_outline.push((source, bytes)),
            (None, None) => {}
        }
        for (matched, before) in file.matches[start..end].iter().zip(before) {
            if matched.value != before {
                redacted.insert((file.path.clone(), matched.line, matched.column));
            }
        }
    }
    outlines.extend(parallel_outlines(to_outline, scope));
    Ok(unverified)
}

/// The outline of each read file, parsed on scoped threads; order-free.
fn parallel_outlines(
    files: Vec<(std::path::PathBuf, Vec<u8>)>,
    scope: &PageScope<'_, impl CancellationCheck>,
) -> Vec<(std::path::PathBuf, Option<super::enclosing::Outline>)> {
    let security = scope.security;
    if files.len() < 2 {
        return files
            .into_iter()
            .map(|(source, bytes)| {
                let outline = super::enclosing::outline_of(&bytes, &source, security);
                (source, outline)
            })
            .collect();
    }
    let workers = crate::tools::workers::workers(scope.walk_threads, files.len(), None);
    let per_worker = files.len().div_ceil(workers);
    let mut files = files;
    std::thread::scope(|scope| {
        let mut tasks = Vec::with_capacity(workers);
        while !files.is_empty() {
            let chunk: Vec<_> = files.drain(..per_worker.min(files.len())).collect();
            tasks.push(scope.spawn(move || {
                chunk
                    .into_iter()
                    .map(|(source, bytes)| {
                        let outline = super::enclosing::outline_of(&bytes, &source, security);
                        (source, outline)
                    })
                    .collect::<Vec<_>>()
            }));
        }
        // A worker that panicked leaves its files without enclosing names.
        tasks
            .into_iter()
            .filter_map(|task| task.join().ok())
            .flatten()
            .collect()
    })
}

/// Outlines the files a page cut prices names by (see
/// [`super::layout::NamePricing`]), each read within the parse cap once the
/// read policy passes it and parsed in parallel; each parsed outline is
/// kept in `cut` for the page's shown files. A file the policy denies or
/// that cannot be read is unread (its rows reserve). A stored scan's file
/// that no longer hashes to the bytes it matched restarts, as on the page
/// that shows it, so a continuation never cuts other pages.
pub(super) fn outline_for_cut(
    scope: &PageScope<'_, impl CancellationCheck>,
    files: &[(usize, &octocode_engine::types::TextSearchFile)],
    cut: &mut CutOutlines,
) -> Result<Vec<(usize, FilePrice)>, ToolError> {
    let mut indexes = std::collections::HashMap::new();
    let mut reads = Vec::new();
    for &(index, file) in files {
        scope.cancel.check().map_err(ToolError::cancelled)?;
        let source = scope.output_root.join(&file.path);
        if validate_matched(scope.paths, &source).is_err() {
            continue;
        }
        let Ok(bytes) = crate::tools::source::read_bounded(&source, MAX_PARSE_SIZE) else {
            continue;
        };
        if let Some(expected) = (scope.expected_digest)(&source)
            && expected != Some(<[u8; 32]>::from(Sha256::digest(&bytes)))
        {
            return Err(changed_since_scan(scope.query));
        }
        indexes.insert(source.clone(), (index, file));
        reads.push((source, bytes));
    }
    Ok(parallel_outlines(reads, scope)
        .into_iter()
        .filter_map(|(source, outline)| {
            let &(index, file) = indexes.get(&source)?;
            let price = FilePrice::of(file, outline.as_ref());
            cut.insert(index, outline);
            Some((index, price))
        })
        .collect())
}

/// Outlines a page cut parsed, by file index (see [`outline_for_cut`]).
pub(super) type CutOutlines = std::collections::HashMap<usize, Option<super::enclosing::Outline>>;

pub(super) use crate::security::cut::strip_clip_markers;

/// Security: the engine clips values (matchOnly spans, matchContentLength,
/// long-line windows) *before* sanitization, so a clipped
/// secret no longer matches any secret pattern and leaks verbatim. For each
/// shown match, sanitize the full source lines the value was cut from; a value
/// line not literally present in that sanitized text overlapped a redaction and
/// is replaced (placeholder for spans, the sanitized match line otherwise).
/// Private-key blocks are detected from the whole file prefix, streamed once
/// without retaining it; only the shown matches' neighborhoods are kept.
///
/// Fails closed: when the source cannot be re-read, no longer holds a shown
/// line, or its neighborhood exceeds the retained-byte budget, the value is
/// replaced by a placeholder and the result is `Unverified`. With `expected`
/// (values from a stored scan), the whole source is hashed while it is read;
/// bytes that differ from the stored digest, or a missing digest, return
/// `Changed` and the values must not be shown. Cancellation stops the read
/// and returns the reason.
pub(super) fn guard_clipped_secrets(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    expected: Option<Option<super::manifest::Digest>>,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    guard_read(
        file,
        source,
        || octocode_engine::text::open_regular(source).map(|(file, _)| file),
        expected,
        shown,
        check,
    )
}

/// How shown values are checked against their source: the secret scanner,
/// whether values are matchOnly spans (an exposed one becomes a bare
/// placeholder), and the cancel that stops the read.
pub(super) struct ValueCheck<'a, C> {
    pub(super) security: &'a ContentSecurity,
    pub(super) match_only: bool,
    pub(super) cancel: &'a C,
}

/// [`guard_clipped_secrets`] over the reader `open` returns, opened only
/// when a shown value needs its source lines.
pub(super) fn guard_read<R: std::io::Read>(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    open: impl FnOnce() -> std::io::Result<R>,
    expected: Option<Option<super::manifest::Digest>>,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    let ValueCheck {
        security,
        match_only,
        cancel,
    } = *check;
    if expected == Some(None) {
        return Ok(Verification::Changed);
    }
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let shown = &mut file.matches[start..end];
    // 1-based inclusive source lines each shown value needs re-sanitized.
    let windows = shown
        .iter()
        .map(|m| {
            let span = m.value.lines().count().max(1);
            let line = m.line as usize;
            (line.saturating_sub(span).max(1), line + span)
        })
        .collect::<Vec<_>>();
    let Some(last_line) = windows.iter().map(|&(_, hi)| hi).max() else {
        return Ok(Verification::Verified);
    };
    let read = open().map_err(VerifyReadError::from).and_then(|opened| {
        let mut reader = std::io::BufReader::new(HashingReader {
            inner: opened,
            hasher: expected.map(|_| Sha256::new()),
        });
        let read = read_verification_lines(&mut reader, &windows, last_line, cancel)?;
        let digest = drain_digest(reader, cancel)?;
        Ok((read, digest))
    });
    let read = match read {
        Ok((_, Some(digest))) if expected != Some(Some(digest)) => {
            return Ok(Verification::Changed);
        }
        Ok((read, _)) if !read.unclassified => read,
        Ok(_) | Err(VerifyReadError::Io) => {
            for matched in shown.iter_mut() {
                matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            }
            return Ok(Verification::Unverified);
        }
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    let mut verified = Verification::Verified;
    for (matched, &(lo, hi)) in shown.iter_mut().zip(&windows) {
        if !read.key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &read.key_ranges,
            )
        {
            let kept =
                redact_key_block_lines(matched.line, &matched.value, &read.key_ranges, |n| {
                    read.retained.get(&(n as usize)).map(String::as_str)
                });
            matched.value = security.sanitize_text(&kept, Some(source)).content;
            continue;
        }
        let line = matched.line as usize;
        if line == 0 {
            continue;
        }
        if line > read.lines_read {
            // The source shrank or was replaced since the search.
            matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        }
        let hi = hi.min(read.lines_read);
        let Some(window) = (lo..=hi)
            .map(|n| read.retained.get(&n).map(String::as_str))
            .collect::<Option<Vec<_>>>()
        else {
            matched.value = OVERSIZED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let sanitized = security.sanitize_text(&window.join("\n"), Some(source));
        if !sanitized.has_secrets {
            continue;
        }
        if crate::security::cut::exposes(&matched.value, &sanitized.content) {
            matched.value = if match_only {
                "[REDACTED]".to_owned()
            } else {
                security
                    .sanitize_text(window[line - lo], Some(source))
                    .content
            };
        }
    }
    Ok(verified)
}

/// Outcome of [`guard_clipped_secrets`] for one file's shown matches.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verification {
    /// Every shown value was checked against the source.
    Verified,
    /// Some value was replaced by a placeholder because it could not be checked.
    Unverified,
    /// The source is not the stored scan's bytes; nothing may be shown.
    Changed,
}

/// Passes reads through, hashing them when a digest is wanted.
pub(super) struct HashingReader<R> {
    pub(super) inner: R,
    pub(super) hasher: Option<Sha256>,
}

impl<R: std::io::Read> std::io::Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if let Some(hasher) = &mut self.hasher {
            hasher.update(&buf[..read]);
        }
        Ok(read)
    }
}

/// Read the rest of a hashed source and return its digest; `None` when the
/// reader was not hashing.
pub(super) fn drain_digest<R: std::io::Read>(
    mut reader: std::io::BufReader<HashingReader<R>>,
    cancel: &impl CancellationCheck,
) -> Result<Option<super::manifest::Digest>, VerifyReadError> {
    use std::io::BufRead;
    if reader.get_ref().hasher.is_none() {
        return Ok(None);
    }
    let mut since_check = 0usize;
    loop {
        let len = reader.fill_buf()?.len();
        if len == 0 {
            break;
        }
        reader.consume(len);
        since_check += len;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
    }
    Ok(reader
        .into_inner()
        .hasher
        .map(|hasher| hasher.finalize().into()))
}

/// Value shown in place of a match whose source could not be re-read for the
/// clipped-secret check.
pub(super) const UNVERIFIED_PLACEHOLDER: &str = "[REDACTED: source unreadable for secret check]";

/// Value shown in place of a match whose source neighborhood exceeds
/// [`MAX_VERIFY_RETAINED_BYTES`].
pub(super) const OVERSIZED_PLACEHOLDER: &str =
    "[REDACTED: source lines too large for secret check]";

/// Source bytes one file's verification may retain for the shown matches'
/// neighborhoods; the rest of the prefix is streamed for key-block state only.
pub(super) const MAX_VERIFY_RETAINED_BYTES: usize = 16 * 1024 * 1024;

/// Bytes of a line outside every neighborhood buffered to classify it; a
/// longer line is consumed in chunks without being stored.
pub(super) const MAX_PROBE_LINE_BYTES: usize = 64 * 1024;

/// Lines (or 1 MiB chunks of one long line) between cancellation checks.
pub(super) const VERIFY_CANCEL_EVERY: usize = 4096;

pub(super) const VERIFY_CHUNK_BYTES: usize = 1024 * 1024;

pub(super) enum VerifyReadError {
    /// The source could not be opened or read.
    Io,
    Cancelled(String),
}

impl From<std::io::Error> for VerifyReadError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

/// One streamed pass over a source prefix for the clipped-secret check.
pub(super) struct VerificationLines {
    /// Private-key block ranges over every line read.
    pub(super) key_ranges: Vec<(u32, u32)>,
    /// Neighborhood lines (1-based) that fit the retained-byte budget.
    pub(super) retained: std::collections::BTreeMap<usize, String>,
    pub(super) retained_bytes: usize,
    pub(super) lines_read: usize,
    /// A line too long to buffer mentions `PRIVATE KEY`, so key-block state
    /// is unknown and nothing read from this source can be trusted.
    pub(super) unclassified: bool,
}

/// Stream up to `limit` lines of `reader`, tracking private-key blocks over
/// all of them and keeping only lines inside `windows` (1-based inclusive),
/// within [`MAX_VERIFY_RETAINED_BYTES`]. Any read failure is an error (a
/// short source just yields fewer lines).
pub(super) fn read_verification_lines(
    reader: &mut impl std::io::BufRead,
    windows: &[(usize, usize)],
    limit: usize,
    cancel: &impl CancellationCheck,
) -> Result<VerificationLines, VerifyReadError> {
    let mut tracker = crate::security::KeyBlockTracker::default();
    let mut read = VerificationLines {
        key_ranges: Vec::new(),
        retained: std::collections::BTreeMap::new(),
        retained_bytes: 0,
        lines_read: 0,
        unclassified: false,
    };
    let mut buf = Vec::new();
    while read.lines_read < limit {
        let number = read.lines_read + 1;
        if number.is_multiple_of(VERIFY_CANCEL_EVERY) {
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        let wanted = windows.iter().any(|&(lo, hi)| lo <= number && number <= hi);
        let cap = if wanted {
            MAX_VERIFY_RETAINED_BYTES.saturating_sub(read.retained_bytes)
        } else {
            MAX_PROBE_LINE_BYTES
        };
        buf.clear();
        let Some(line) = read_bounded_line(reader, &mut buf, cap, cancel)? else {
            break;
        };
        read.lines_read = number;
        match line {
            BoundedLine::Whole => {
                let text = String::from_utf8_lossy(&buf);
                let text = text.trim_end_matches(['\n', '\r']);
                tracker.push(text);
                if wanted {
                    read.retained_bytes += text.len();
                    read.retained.insert(number, text.to_owned());
                }
            }
            BoundedLine::Overflow { mentions_key } => {
                // Every key boundary contains `PRIVATE KEY`; a long line
                // without it is an ordinary line for the block state.
                read.unclassified |= mentions_key;
                tracker.push("");
            }
        }
    }
    read.key_ranges = tracker.finish();
    Ok(read)
}

pub(super) enum BoundedLine {
    /// The whole line (with its terminator) is in the buffer.
    Whole,
    /// The line exceeded the cap and was consumed without being kept.
    Overflow { mentions_key: bool },
}

/// Read one line into `buf` if it fits `cap` bytes; otherwise consume it in
/// chunks, noting whether it mentions `PRIVATE KEY`. `None` at end of file.
pub(super) fn read_bounded_line(
    reader: &mut impl std::io::BufRead,
    buf: &mut Vec<u8>,
    cap: usize,
    cancel: &impl CancellationCheck,
) -> Result<Option<BoundedLine>, VerifyReadError> {
    const NEEDLE: &[u8] = b"PRIVATE KEY";
    let mut overflow = false;
    let mut mentions_key = false;
    let mut consumed = 0usize;
    let mut since_check = 0usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        let (take, done) = match chunk.iter().position(|&byte| byte == b'\n') {
            Some(at) => (at + 1, true),
            None => (chunk.len(), false),
        };
        let part = &chunk[..take];
        if !overflow && buf.len() + take > cap {
            overflow = true;
        }
        if overflow {
            // Keep a needle-length tail so a mention split across chunks
            // is still seen.
            buf.extend_from_slice(part);
            mentions_key |= buf.windows(NEEDLE.len()).any(|w| w == NEEDLE);
            let keep = buf.len().min(NEEDLE.len() - 1);
            buf.drain(..buf.len() - keep);
        } else {
            buf.extend_from_slice(part);
        }
        reader.consume(take);
        consumed += take;
        since_check += take;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        if done {
            break;
        }
    }
    if consumed == 0 {
        return Ok(None);
    }
    Ok(Some(if overflow {
        buf.clear();
        BoundedLine::Overflow { mentions_key }
    } else {
        BoundedLine::Whole
    }))
}

pub(super) fn materialize(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    rows: std::ops::Range<usize>,
) -> Result<(), ToolError> {
    visit_values(scope, file, rows, |_, _| Ok(()))
}

/// Reconstruct one value at a time, then release or consume it before reading
/// the next. Every read is policy-checked and hashes the complete source; no
/// result escapes when that digest differs from the search's bytes.
fn visit_values(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    rows: std::ops::Range<usize>,
    mut visit: impl FnMut(usize, &mut octocode_engine::types::TextSearchMatch) -> Result<(), ToolError>,
) -> Result<(), ToolError> {
    use std::io::BufRead;
    // One check per file: a row's visit is bounded work, and every worker
    // polling the shared cancel per row contended on its lock.
    scope.cancel.check().map_err(ToolError::cancelled)?;
    let mut wanted = Vec::new();
    for index in rows {
        let Some(hit) = file.matches.get_mut(index) else {
            continue;
        };
        if hit.deferred.is_some() && hit.value.is_empty() {
            wanted.push(index);
        } else {
            visit(index, hit)?;
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    wanted.sort_unstable_by_key(|&i| file.matches[i].line);
    let path = scope.output_root.join(&file.path);
    validate_matched(scope.paths, &path)?;
    let source = file.source.ok_or_else(|| changed_since_scan(scope.query))?;
    let (input, _) =
        octocode_engine::text::open_regular(&path).map_err(|_| changed_since_scan(scope.query))?;
    let mut input = std::io::BufReader::new(CheckedRead {
        input,
        cancel: scope.cancel,
    });
    let mut digest = Sha256::new();
    let mut bytes = Vec::new();
    let mut line = 0u32;
    let mut at = 0;
    let mut size = 0u64;
    // `CheckedRead` polls the cancel on every read of the file, so the
    // lines served from its buffer need no check of their own.
    while at < wanted.len() {
        bytes.clear();
        // Match the engine's line-buffer ceiling, including a terminating newline.
        let read = std::io::Read::take(&mut input, 64 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| {
                scope
                    .cancel
                    .check()
                    .err()
                    .map_or_else(|| changed_since_scan(scope.query), ToolError::cancelled)
            })?;
        if read == 0 {
            break;
        }
        if read > 64 * 1024 * 1024 {
            return Err(changed_since_scan(scope.query));
        }
        digest.update(&bytes);
        size += read as u64;
        line = line.saturating_add(1);
        while let Some(&index) = wanted.get(at).filter(|&&i| file.matches[i].line == line) {
            let value_bytes = if line == 1 {
                &bytes[octocode_engine::text::bom_len(&bytes)..]
            } else {
                &bytes
            };
            let value_bytes = value_bytes
                .split(|byte| *byte == 0)
                .next()
                .unwrap_or_default();
            let text = String::from_utf8_lossy(value_bytes);
            let text = text
                .strip_suffix('\n')
                .map(|s| s.strip_suffix('\r').unwrap_or(s))
                .unwrap_or(&text);
            let hit = &mut file.matches[index];
            hit.value = octocode_engine::search::text_parser::line_match(
                text,
                line,
                hit.column,
                super::executor::effective_match_content_length(scope.query) as usize,
            )
            .value;
            visit(index, hit)?;
            at += 1;
        }
    }
    // The remaining source still binds the snapshot, but has no values to
    // reconstruct. A binary tail can be larger than the searcher's line cap.
    drop(bytes);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        scope.cancel.check().map_err(ToolError::cancelled)?;
        let read = std::io::Read::read(&mut input, &mut chunk).map_err(|_| {
            scope
                .cancel
                .check()
                .err()
                .map_or_else(|| changed_since_scan(scope.query), ToolError::cancelled)
        })?;
        if read == 0 {
            break;
        }
        digest.update(&chunk[..read]);
        size += read as u64;
    }
    if at != wanted.len()
        || size != source.size
        || <[u8; 32]>::from(digest.finalize()) != source.digest
    {
        return Err(changed_since_scan(scope.query));
    }
    Ok(())
}

struct CheckedRead<'a, C> {
    input: std::fs::File,
    cancel: &'a C,
}

impl<C: CancellationCheck> std::io::Read for CheckedRead<'_, C> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        std::io::Read::read(&mut self.input, bytes)
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;

    fn hit(line: u32, value: &str) -> octocode_engine::types::TextSearchMatch {
        octocode_engine::types::TextSearchMatch {
            deferred: None,
            line,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: value.into(),
            value_digest: Sha256::digest(value.trim().as_bytes()).into(),
            count: None,
            rank: None,
            original_chars: None,

            original_bytes: None,
        }
    }

    /// Files redact on parallel workers, yet the record lists key-block
    /// reads and replaced values in file and match order, every time, and
    /// each replaced value is marked redacted on its row.
    #[test]
    fn a_parallel_redaction_records_in_file_order() {
        let dir = tempfile::tempdir().expect("fixture");
        let (paths, security) = crate::tools::test_support::simple_policy(dir.path());
        let key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n";
        let mut files = Vec::new();
        for index in 0..48 {
            let path = dir.path().join(format!("f{index:02}.ts"));
            let mut body = String::from("const plain = 1;\n");
            let mut matches = vec![hit(1, "const plain = 1;")];
            if index % 3 == 0 {
                let secret =
                    format!("const t = \"ghp_{index:02}ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567\";");
                body.push_str(&secret);
                body.push('\n');
                matches.push(hit(2, &secret));
            }
            if index % 7 == 0 {
                body.push_str(key);
                matches.push(hit(
                    4,
                    "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                ));
            }
            std::fs::write(&path, body).expect("fixture");
            files.push(octocode_engine::types::TextSearchFile {
                path: path.to_string_lossy().into_owned(),
                match_count: matches.len() as u32,
                matches,
                source: None,
            });
        }
        let scan = octocode_engine::types::TextSearchResult {
            files,
            stats: Default::default(),
        };
        let query: LocalSearchQuery = serde_json::from_value(serde_json::json!({
            "path": dir.path(), "matchString": "const", "mainGoal": "test", "reasoning": "test"
        }))
        .expect("query");
        let expected = |_: &std::path::Path| None;
        let scope = PageScope {
            query: &query,
            paths: &paths,
            output_root: dir.path(),
            expected_digest: &expected,
            security: &security,
            cancel: &NeverCancel,
            walk_threads: None,
        };
        let redact = || {
            let mut parsed = scan.clone();
            let (redacted, record) = redact_scan(&scope, &mut parsed, None).expect("redaction");
            (redacted, record, parsed)
        };
        let (redacted, record, parsed) = redact();
        for _ in 0..3 {
            let (again, again_record, again_parsed) = redact();
            assert_eq!(again, redacted);
            assert_eq!(again_record, record);
            assert_eq!(
                format!("{:?}", again_parsed.files),
                format!("{:?}", parsed.files)
            );
        }
        let order = record
            .changes
            .iter()
            .map(|(file, row, _)| (*file, *row))
            .collect::<Vec<_>>();
        assert!(order.is_sorted(), "{order:?}");
        assert!(record.key_files.iter().map(|(file, _)| *file).is_sorted());
        assert_eq!(record.key_files.len(), 7, "{:?}", record.key_files);
        assert!(record.changes.len() >= 16 + 7, "{:?}", record.changes);
        assert_eq!(redacted.len(), record.changes.len());
        assert!(parsed.files.iter().all(|file| !file.path.starts_with('/')));
        for (file, row, value) in &record.changes {
            assert_eq!(&parsed.files[*file].matches[*row].value, value);
        }
    }

    fn open(source: &std::path::Path) -> std::io::BufReader<std::fs::File> {
        std::io::BufReader::new(std::fs::File::open(source).expect("fixture"))
    }

    fn file_with(
        matches: Vec<octocode_engine::types::TextSearchMatch>,
    ) -> octocode_engine::types::TextSearchFile {
        octocode_engine::types::TextSearchFile {
            path: "source.txt".into(),
            match_count: matches.len() as u32,
            matches,
            source: None,
        }
    }

    #[test]
    fn a_late_hit_retains_only_its_neighborhood_with_exact_coordinates() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let filler = "émoji ✓ filler line that is not near the hit\n".repeat(60_000);
        std::fs::write(&source, format!("{filler}before\nthe hit ✓\nafter\ntail\n"))
            .expect("fixture");
        let hit_line = 60_002;
        let read = read_verification_lines(
            &mut open(&source),
            &[(hit_line - 1, hit_line + 1)],
            hit_line + 1,
            &NeverCancel,
        )
        .unwrap_or_else(|_| panic!("readable"));
        assert_eq!(read.lines_read, hit_line + 1);
        assert_eq!(read.retained.len(), 3);
        assert_eq!(read.retained[&hit_line], "the hit ✓");
        assert!(read.retained_bytes < 64, "{}", read.retained_bytes);
        assert!(read.key_ranges.is_empty());
    }

    #[test]
    fn streamed_key_state_matches_the_whole_file_scan() {
        let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for content in [
            format!(
                "a\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\nb\n"
            ),
            format!("a\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n"),
            format!(
                "{}\n-----BEGIN EC PRIVATE KEY-----\n{body}\r\n-----END EC PRIVATE KEY-----\r\n",
                "x".repeat(200_000)
            ),
        ] {
            let dir = tempfile::tempdir().expect("fixture");
            let source = dir.path().join("key.txt");
            std::fs::write(&source, &content).expect("fixture");
            let read = read_verification_lines(&mut open(&source), &[], usize::MAX, &NeverCancel)
                .unwrap_or_else(|_| panic!("readable"));
            assert!(read.retained.is_empty());
            assert!(!read.unclassified);
            assert_eq!(
                read.key_ranges,
                crate::security::private_key_block_line_ranges(&content)
            );
        }
    }

    #[test]
    fn an_unbufferable_line_mentioning_a_private_key_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = format!("{}PRIVATE KEY{}", " ".repeat(200_000), "-".repeat(10));
        std::fs::write(
            &source,
            format!("{giant}\nspacer\nspacer\nspacer\nMIIEpQIBAAKCAQEAinteriorKeyBody\n"),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "…interiorKeyBo")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                match_only: true,
                cancel: &NeverCancel,
            },
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, UNVERIFIED_PLACEHOLDER);
        // The same long line without a key mention is an ordinary line.
        std::fs::write(
            &source,
            format!(
                "{}\nspacer\nspacer\nspacer\nplain text here\n",
                " ".repeat(200_000)
            ),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "plain text")]);
        assert_eq!(
            guard_clipped_secrets(
                &mut file,
                &source,
                None,
                0..10,
                &ValueCheck {
                    security: &ContentSecurity::new(),
                    match_only: true,
                    cancel: &NeverCancel,
                },
            )
            .expect("not cancelled"),
            Verification::Verified
        );
        assert_eq!(file.matches[0].value, "plain text");
    }

    /// Values from a stored scan are checked only against the bytes the scan
    /// was stored with; other bytes, or no stored digest, mean `Changed`.
    #[test]
    fn a_stored_digest_binds_the_check_to_the_stored_bytes() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let stored = "one\nthe hit\nthree\n";
        std::fs::write(&source, stored).expect("fixture");
        let digest: super::super::manifest::Digest = Sha256::digest(stored.as_bytes()).into();
        let check = |expected| {
            let mut file = file_with(vec![hit(2, "the hit")]);
            let outcome = guard_clipped_secrets(
                &mut file,
                &source,
                expected,
                0..10,
                &ValueCheck {
                    security: &ContentSecurity::new(),
                    match_only: false,
                    cancel: &NeverCancel,
                },
            )
            .expect("not cancelled");
            (outcome, file.matches[0].value.clone())
        };
        assert_eq!(
            check(Some(Some(digest))),
            (Verification::Verified, "the hit".to_owned())
        );
        assert_eq!(check(Some(None)).0, Verification::Changed);
        std::fs::write(&source, "one\nthe hit\nthre3\n").expect("fixture");
        assert_eq!(check(Some(Some(digest))).0, Verification::Changed);
        assert_eq!(check(None).0, Verification::Verified);
    }

    #[test]
    fn a_neighborhood_over_the_retained_budget_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = "a".repeat(MAX_VERIFY_RETAINED_BYTES + 1);
        std::fs::write(&source, format!("{giant}\n")).expect("fixture");
        let mut file = file_with(vec![hit(1, "…aaaa…")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                match_only: false,
                cancel: &NeverCancel,
            },
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, OVERSIZED_PLACEHOLDER);
    }

    #[test]
    fn verification_stops_when_cancelled() {
        struct Cancelled;
        impl CancellationCheck for Cancelled {
            fn check(&self) -> Result<(), String> {
                Err("Cancelled".into())
            }
        }
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        std::fs::write(&source, "line\n".repeat(VERIFY_CANCEL_EVERY * 2)).expect("fixture");
        let mut file = file_with(vec![hit((VERIFY_CANCEL_EVERY * 2) as u32, "line")]);
        let reason = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                match_only: false,
                cancel: &Cancelled,
            },
        )
        .expect_err("cancelled during the stream");
        assert_eq!(reason, "Cancelled");
    }

    /// Context lines beside a key block stay readable; only the lines
    /// inside the block are redacted.
    #[test]
    fn key_block_redaction_is_line_precise() {
        let source = [
            "const filler393 = \"match_me 393\";",
            "const filler394 = \"match_me 394\";",
            "-----BEGIN RSA PRIVATE KEY-----",
            "MIIEowIBAAKCAQEAexamplebodyexamplebodyexample",
            "-----END RSA PRIVATE KEY-----",
        ];
        let at = |n: u32| source.get((n as usize).checked_sub(1)?).copied();
        let ranges = [(3, 5)];
        let placeholder = crate::security::key_fragment_placeholder();
        // Match on line 2 with one context line each side (lines 1-3).
        let value = format!("{}\n{}\n{}", source[0], source[1], source[2]);
        assert_eq!(
            redact_key_block_lines(2, &value, &ranges, at),
            format!("{}\n{}\n{placeholder}", source[0], source[1])
        );
        // An already-redacted value still aligns.
        let redacted = format!("{}\n{placeholder}\n{placeholder}", source[1]);
        assert_eq!(redact_key_block_lines(3, &redacted, &ranges, at), redacted);
        // A value no alignment fits fails closed.
        assert_eq!(
            redact_key_block_lines(2, "not in the source\nMIIEowIB", &ranges, at),
            placeholder
        );
    }
}
