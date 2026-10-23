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

mod guard;
mod lines;
mod redaction;
#[cfg(test)]
mod verification_tests;

pub(super) use guard::*;
pub(super) use lines::*;
pub(super) use redaction::*;

/// Upper bound (bytes) on a file re-read for the private-key block scan.
/// Matches the secret scanner's own content cap; larger files fall back to the
/// per-match window sanitizer rather than pay an unbounded read.
pub(super) const MAX_KEY_SCAN_BYTES: u64 = 10 * 1024 * 1024;

/// What a stored scan's file must still hash to (see `expected_digest`).
pub(super) type ExpectedDigest<'a> =
    dyn Fn(&std::path::Path) -> Option<Option<super::sources::Digest>> + Sync + 'a;

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
    pub(super) key_files: Vec<(usize, Option<super::sources::Digest>)>,
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
                && (expected.is_none() || expected != super::sources::digest_file(&source).ok())
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
            .then(|| super::sources::read_entry(&source, MAX_PARSE_SIZE))
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
        let Some(bytes) = super::sources::read_entry(&source, MAX_PARSE_SIZE) else {
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
