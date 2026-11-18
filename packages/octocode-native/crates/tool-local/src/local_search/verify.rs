//! Secret safety of shown values: redaction of the scan, the re-read that
//! checks clipped values against their source bytes, and bounded line reads.
use crate::local_search::local_search_query;

use super::{cursor::*, layout::*};
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::num::saturating_u32;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::security::ContentSecurity;

mod decoded;
#[cfg(test)]
mod gone_tests;
#[cfg(test)]
mod grown_tests;
mod guard;
mod large;
pub mod lines;
mod redaction;
mod settle;
#[cfg(test)]
mod torn_tests;
#[cfg(test)]
mod verification_tests;

pub(super) use guard::{
    MAX_PROBE_LINE_BYTES, RowShape, VERIFY_CANCEL_EVERY, VERIFY_CHUNK_BYTES, ValueCheck,
    Verification, guard_read, guard_source_secrets,
};
#[cfg(test)]
pub(super) use lines::materialize;
use lines::visit_values;
pub(super) use lines::{VerifyReadError, materialize_noting, read_verification_lines};
pub(super) use settle::JudgedWindows;
#[cfg(test)]
pub(super) use settle::judged_on_this_thread;
// The special-file tests (unix-only) read the key-scan bytes directly.
#[cfg(all(test, unix))]
pub(super) use redaction::read_key_scan_bytes;
use redaction::redact_key_block_lines;
pub(super) use redaction::{check_key_scans, redact_scan, scan_inspector};

/// What a stored scan's file must still hash to (see `expected_digest`).
pub(super) type ExpectedDigest<'a> =
    dyn Fn(&std::path::Path) -> super::sources::Expected + Sync + 'a;

/// What every stage of one page reads besides the scan: the query, the read
/// policy, the root the rows name paths under, the bytes a stored scan's
/// checks must read, the secret scanner and the cancel.
pub(super) struct PageScope<'a, C> {
    pub(super) query: &'a LocalSearchQuery,
    pub(super) paths: &'a octocode_runtime_base::policy::path::PathPolicy,
    pub(super) output_root: &'a std::path::Path,
    pub(super) expected_digest: &'a ExpectedDigest<'a>,
    pub(super) security: &'a ContentSecurity,
    pub(super) cancel: &'a C,
    /// The scan's judged row windows (see [`JudgedWindows`]).
    pub(super) judged: &'a JudgedWindows,
}

/// (path, line, column) of every match whose value secret redaction
/// changed: the returned text is then not verbatim source, and the row says
/// so via a `redactedMatches` warning.
pub(super) type Redacted = std::collections::HashSet<(String, u32, u32)>;

/// What a check of one file's shown rows found: (line, column) of each row
/// whose value shows text the redaction wrote (a [`Redacted`] row, whether
/// the check cut that text or the search had: a window or span of the text a
/// read serves, whose value the check keeps as it is), and whether it cut
/// the rows from one judgment of the whole source.
#[derive(Debug, Default)]
pub(in crate::local_search) struct HiddenRows {
    rows: std::sync::Mutex<Vec<(u32, u32)>>,
    whole: std::sync::atomic::AtomicBool,
}

impl HiddenRows {
    /// Notes `row` as showing text the redaction wrote.
    pub(super) fn note(&self, row: &octocode_engine::types::TextSearchMatch) {
        self.rows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((row.line, row.column));
    }

    /// Notes that the check settled the rows against one judgment of the
    /// whole source, proved to be of the bytes their values were made from.
    pub(super) fn judged_whole(&self) {
        self.whole.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether the rows were settled against one judgment of the whole
    /// source ([`Self::judged_whole`]).
    pub(super) fn of_whole_source(&self) -> bool {
        self.whole.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Each noted row of the file at `path`, as a [`Redacted`] entry.
    fn into_redacted(self, path: &str) -> impl Iterator<Item = (String, u32, u32)> + '_ {
        self.rows
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .into_iter()
            .map(move |(line, column)| (path.to_owned(), line, column))
    }
}

/// Notes `source` as judged whole for the output gate, which then takes each
/// row the page shows of it as a cut of that judgment: only when the check
/// settled every row against it (`verification`, `hidden`) and the page
/// withheld none of them (`withheld`), whose text no judgment of it cut.
fn note_cut_whole(
    security: &ContentSecurity,
    source: &std::path::Path,
    (verification, hidden): (&Verification, &HiddenRows),
    withheld: bool,
) {
    if *verification == Verification::Verified && hidden.of_whole_source() && !withheld {
        security.note_judged_whole(source);
    }
}

/// (path, line, column) of every match whose value redacting a fresh scan
/// withheld because its file changed during the call (see
/// [`lines::WithheldRows`]): a page that shows one is unverified.
pub(super) type Withheld = std::collections::HashSet<(String, u32, u32)>;

/// Whether a row in `rows` of `file` is `withheld`.
fn shows_withheld(
    withheld: &Withheld,
    file: &octocode_engine::types::TextSearchFile,
    rows: &[octocode_engine::types::TextSearchMatch],
) -> bool {
    !withheld.is_empty()
        && rows
            .iter()
            .any(|hit| withheld.contains(&(file.path.clone(), hit.line, hit.column)))
}

/// What redacting a fresh scan changed, so a later page of the same stored
/// scan replays it instead of sanitizing every value again. Sanitizing is a
/// pure function of a value and its source path; the only file input is a
/// key-block scan, so the record keeps the digest of each such read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redaction {
    /// The root the scan's paths were made relative to.
    pub(super) output_root: std::path::PathBuf,
    /// Files whose snippets looked like key material (see [`KeyFile`]).
    pub(super) key_files: Vec<KeyFile>,
    /// Values the redaction replaced: file index, match index, new value.
    pub(super) changes: Vec<(usize, usize, String)>,
}

/// A file whose snippets looked like key material, and its key-block scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyFile {
    /// Its index in the scan, in scan order (a whole scan replays by it).
    pub(super) index: usize,
    /// Its row path.
    pub(super) path: String,
    /// The digest of the bytes the scan read (`None`: no read).
    pub(super) read: Option<super::sources::Digest>,
}

impl Redaction {
    /// Bytes the record adds to a stored scan.
    pub(super) fn weight(&self) -> usize {
        self.output_root.as_os_str().len()
            + self
                .key_files
                .iter()
                .map(|key| key.path.len() + 40)
                .sum::<usize>()
            + self
                .changes
                .iter()
                .map(|(_, _, value)| value.len() + 16)
                .sum::<usize>()
    }
}

/// A matched file must pass the read policy before a page reads it or shows
/// its rows. A file gone since the scan (an atomic save's temporary file,
/// renamed away) is no denial: nothing at its path is read, and the page that
/// shows its rows leaves them out ([`verify_shown`]).
pub(super) fn validate_matched(
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    source: &std::path::Path,
) -> Result<(), ToolError> {
    matched_gone(paths, source).map(drop)
}

/// Checks a matched file against the read policy: `Ok(true)` when the check
/// found nothing at its path (the scan matched a file there, so it was
/// removed or renamed since), `Ok(false)` when the policy admits it.
fn matched_gone(
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    source: &std::path::Path,
) -> Result<bool, ToolError> {
    #[cfg(test)]
    race(&BEFORE_VALIDATE, source);
    match paths.validate_read(source) {
        Ok(_) => Ok(false),
        Err(error)
            if error.code == octocode_runtime_base::policy::PolicyErrorCode::NotFound
                && !paths.names_several_entries(source) =>
        {
            Ok(true)
        }
        Err(error) => Err(ToolError::new(
            error.shared_code(),
            "Search encountered a path denied by the active path policy",
        )),
    }
}

/// Counts `source` among the files whose matched rows the page could not
/// serve (`why`): one gone since the scan, as the search counts a file gone
/// before it read it, or one changed during the call that no read reached
/// some matched lines of. The row states it as a file it could not read
/// (`fileErrors`, `partialReasons: filesSkipped`).
fn note_left_out(
    stats: &mut octocode_engine::types::TextSearchStats,
    source: &std::path::Path,
    why: impl FnOnce() -> String,
) {
    stats.error_count = Some(stats.error_count.unwrap_or(0).saturating_add(1));
    if stats.first_error.is_none() {
        stats.first_error = Some(format!("{}: {}", source.display(), why()));
    }
}

/// Takes a file gone since the scan out of the totals the scan counted: the
/// row's counts are of the files it can still show.
fn uncount(
    stats: &mut octocode_engine::types::TextSearchStats,
    gone: octocode_engine::types::FileCounts,
) {
    let less = |count: &mut Option<u32>, by: u32| {
        if let Some(count) = count.as_mut() {
            *count = count.saturating_sub(by);
        }
    };
    less(&mut stats.match_count, gone.occurrences);
    less(&mut stats.matched_lines, gone.lines);
    less(&mut stats.files_matched, 1);
}

/// Leaves `left_out` (indexes into `file.matches`) off the page: the rows
/// `rows` showed, without them.
fn leave_out(
    file: &mut octocode_engine::types::TextSearchFile,
    rows: std::ops::Range<usize>,
    left_out: &[usize],
) -> std::ops::Range<usize> {
    if left_out.is_empty() {
        return rows;
    }
    let before = left_out.iter().filter(|&&at| at < rows.start).count();
    let within = left_out.iter().filter(|&&at| rows.contains(&at)).count();
    let mut at = 0;
    file.matches.retain(|_| {
        let kept = !left_out.contains(&at);
        at += 1;
        kept
    });
    rows.start - before..rows.end - before - within
}

/// Whether every value of `file` was checked against its source, so the
/// candidate's state can be judged; a withheld value never is.
pub(super) fn verify_candidate(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    (redacted, withheld): (&mut Redacted, &Withheld),
) -> Result<bool, ToolError> {
    scope.cancel.check().map_err(ToolError::cancelled)?;
    let source = scope.output_root.join(&file.path);
    validate_matched(scope.paths, &source)?;
    let materialized = materialize_noting(scope, file, 0..file.matches.len())?;
    // A row withheld while its text was rebuilt shows what the redaction
    // made of it: a replaced span is reported.
    for &index in &materialized.replaced {
        let matched = &file.matches[index];
        redacted.insert((file.path.clone(), matched.line, matched.column));
    }
    let held = !materialized.is_empty() || shows_withheld(withheld, file, &file.matches);
    let before = file
        .matches
        .iter()
        .map(|matched| matched.value.clone())
        .collect::<Vec<_>>();
    let rows = 0..file.matches.len();
    let hidden = HiddenRows::default();
    let verification = guard_source_secrets(
        file,
        &source,
        (scope.expected_digest)(&source),
        rows,
        &ValueCheck {
            security: scope.security,
            shape: RowShape::of(scope.query),
            cancel: scope.cancel,
            judged: Some(scope.judged),
            hidden: &hidden,
        },
    )
    .map_err(ToolError::cancelled)?;
    for (matched, before) in file.matches.iter_mut().zip(before) {
        if matched.value != before {
            matched.deferred = None;
            redacted.insert((file.path.clone(), matched.line, matched.column));
        }
    }
    note_cut_whole(scope.security, &source, (&verification, &hidden), held);
    redacted.extend(hidden.into_redacted(&file.path));
    match verification {
        Verification::Verified => Ok(!held),
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
/// parsed from the secret check's read. A row no earlier read of its
/// changed file reached shows its line as that read holds it. Rows a fresh
/// scan's file can no longer give text (the file is gone since the scan, or
/// no read of it reached their line) are left off the page, and the file is
/// counted in `parsed.stats` as one the page could not read
/// ([`note_left_out`]); a stored scan restarts. Returns whether some shown values were withheld or
/// redacted because their source changed during the call or could not be
/// re-read.
pub(super) fn verify_shown(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    layout: &Layout,
    (redacted, withheld): (&mut Redacted, &Withheld),
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
    if layout.list {
        for (index, _) in &layout.shown {
            let source = output_root.join(&parsed.files[*index].path);
            if let super::sources::Expected::Stored(stored) = expected_digest(&source)
                && (stored.is_none() || stored != super::sources::digest_file(&source).ok())
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
        let source = output_root.join(&file.path);
        // Nothing is read of a file the policy does not admit now. One gone
        // since the scan has nothing to read or show: a stored scan
        // restarts, and a fresh one leaves its rows out.
        if matched_gone(scope.paths, &source)? {
            if let super::sources::Expected::Stored(_) = expected_digest(&source) {
                return Err(changed_since_scan(query));
            }
            let end = rows.end.min(file.matches.len());
            file.matches.drain(rows.start.min(end)..end);
            note_left_out(&mut parsed.stats, &source, || {
                "gone since the search matched it".to_owned()
            });
            uncount(&mut parsed.stats, file.counted);
            unverified = true;
            continue;
        }
        let hidden = HiddenRows::default();
        let check = ValueCheck {
            security,
            shape: RowShape::of(query),
            cancel,
            judged: Some(scope.judged),
            hidden: &hidden,
        };
        let held = materialize_noting(scope, file, rows.clone())?;
        for &index in &held.replaced {
            let matched = &file.matches[index];
            redacted.insert((file.path.clone(), matched.line, matched.column));
        }
        cancel.check().map_err(ToolError::cancelled)?;
        #[cfg(any(test, feature = "test-support"))]
        before_check(&source);
        // The page's first files are outlined for enclosing names: read
        // each once, for its secret check, its outline, and the rows no
        // earlier read of a changed file reached.
        let len = file.matches.len();
        let shows_rows = rows.start.min(len) < rows.end.min(len);
        let parsed_by_cut = shows_rows.then(|| cut.remove(index)).flatten();
        let bytes = (shows_rows && (parsed_by_cut.is_none() || !held.unreached.is_empty()))
            .then(|| {
                super::sources::read_entry(
                    &source,
                    octocode_runtime_base::security::SourceLimits::REAL.whole_source,
                )
            })
            .flatten();
        // Bytes past what this scanner judges whole are judged as the file's
        // read streams them, not from the buffer.
        let whole = |bytes: &&Vec<u8>| bytes.len() <= security.limits().whole_source;
        // A row no earlier read reached shows its line as the check's read
        // holds it, which the check judges; a row that read does not reach
        // either has no text to show and is left out.
        let unreached = match bytes.as_ref().filter(whole) {
            Some(bytes) => lines::rebuild_from_read(file, &held.unreached, bytes),
            None => held.unreached.clone(),
        };
        let rows = &leave_out(file, rows.clone(), &unreached);
        if !unreached.is_empty() {
            let unreached = unreached.len();
            note_left_out(&mut parsed.stats, &source, || {
                format!(
                    "changed while the page read it; {unreached} matched line(s) no read reached were left out"
                )
            });
        }
        // The check rewrites only the shown rows, so only they are compared.
        let end = rows.end.min(file.matches.len());
        let start = rows.start.min(end);
        let rows_withheld =
            !held.is_empty() || shows_withheld(withheld, file, &file.matches[start..end]);
        unverified |= rows_withheld;
        let before = file.matches[start..end]
            .iter()
            .map(|matched| matched.value.clone())
            .collect::<Vec<_>>();
        let expected = expected_digest(&source);
        let verification = match bytes.as_ref().filter(whole) {
            Some(bytes) => guard_read(
                file,
                &source,
                || Ok(bytes.as_slice()),
                expected,
                rows.clone(),
                &check,
            ),
            None => guard_source_secrets(file, &source, expected, rows.clone(), &check),
        };
        let verification = verification.map_err(ToolError::cancelled)?;
        note_cut_whole(security, &source, (&verification, &hidden), rows_withheld);
        match verification {
            Verification::Verified => {}
            Verification::Unverified => unverified = true,
            Verification::Changed => return Err(changed_since_scan(query)),
        }
        match (parsed_by_cut, bytes) {
            (Some(outline), _) => {
                outlines.insert(source, outline);
            }
            (None, Some(bytes)) if start < end => to_outline.push((source, bytes)),
            (None, _) => {}
        }
        for (matched, before) in file.matches[start..end].iter().zip(before) {
            if matched.value != before {
                redacted.insert((file.path.clone(), matched.line, matched.column));
            }
        }
        redacted.extend(hidden.into_redacted(&file.path));
    }
    outlines.extend(parallel_outlines(to_outline, scope));
    Ok(unverified)
}

/// Tests: one write per registered source, made after a page rebuilt the
/// source's values and before its secret check reads it, as a writer racing
/// the call makes it.
#[cfg(any(test, feature = "test-support"))]
pub type CheckRace = (std::path::PathBuf, Box<dyn FnOnce() + Send>);

/// Tests: the writes [`before_check`] makes, by canonical source path.
#[cfg(any(test, feature = "test-support"))]
pub static BEFORE_CHECK: std::sync::Mutex<Vec<CheckRace>> = std::sync::Mutex::new(Vec::new());

/// Tests: the writes made before a page first checks a matched file against
/// the read policy, by canonical source path.
#[cfg(test)]
pub(super) static BEFORE_VALIDATE: std::sync::Mutex<Vec<CheckRace>> =
    std::sync::Mutex::new(Vec::new());

/// Tests: makes the write registered for `source` in [`BEFORE_CHECK`], once.
#[cfg(any(test, feature = "test-support"))]
fn before_check(source: &std::path::Path) {
    race(&BEFORE_CHECK, source);
}

/// Tests: makes the write registered for `source` in `races`, once.
#[cfg(any(test, feature = "test-support"))]
fn race(races: &std::sync::Mutex<Vec<CheckRace>>, source: &std::path::Path) {
    let due = {
        let mut races = races
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        races
            .iter()
            .position(|(path, _)| path == source)
            .map(|at| races.remove(at).1)
    };
    if let Some(write) = due {
        write();
    }
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
    let workers = octocode_tools_shared::workers::workers(files.len(), None);
    let mut files = files;
    octocode_tools_shared::workers::each_parallel(&mut files, workers, |(source, bytes)| {
        // A file whose parse panicked keeps its rows, without enclosing names.
        let outline = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::enclosing::outline_of(bytes, source, security)
        }))
        .unwrap_or(None);
        (std::mem::take(source), outline)
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
        let Some(bytes) = super::sources::read_entry(
            &source,
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        ) else {
            continue;
        };
        if let super::sources::Expected::Stored(stored) = (scope.expected_digest)(&source)
            && stored != Some(octocode_engine::digest::sha256_digest(&bytes))
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
