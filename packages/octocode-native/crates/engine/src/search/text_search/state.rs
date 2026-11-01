//! Search-wide accumulation: per-file records, the shared counters walk workers write, and their
//! snapshot.

use crate::num::saturating_u32;
use crate::search::text_parser::FileEntry;
use crate::text::extension_of;
use crate::types::{BinaryExtensionCount, SearchedSource, TextSearchMatch};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::SystemTime;

/// Per-file collected state from a single `Searcher` pass.
pub(super) struct FileRec {
    pub(super) path: String,
    pub(super) entry: FileEntry,
    /// Number of matching (or, with `-v`, non-matching) lines reported.
    pub(super) matched_lines: u32,
    /// Number of individual matches (submatches) across all matched lines.
    pub(super) submatches: u32,
    /// only-matching spans (empty unless the output is spans).
    pub(super) om_matches: Vec<TextSearchMatch>,
    /// Metadata timestamp captured for non-path sort keys.
    pub(super) sort_time: Option<SystemTime>,
    /// `relevance` only: summed [`relevance::line_weight`](crate::search::relevance::line_weight) of matched lines.
    pub(super) line_weight: u32,
    /// `relevance` only: a test, generated, or vendored path below the root.
    pub(super) demoted: bool,
    /// `relevance` only: a generated file, by its path below the root or its
    /// header; it ranks after every hand-written file.
    pub(super) generated: bool,
    /// `relevance` identifier search only: a matched line declares the name
    /// (see [`identifier_search`](super::mode::identifier_search)).
    pub(super) declares: bool,
    /// `relevance` word alternation only: how many of its words matched
    /// (see [`alternation_words`](super::mode::alternation_words)).
    pub(super) covered: u32,
    /// The bytes the values came from (see [`SourceValues::Hashed`](crate::types::SourceValues::Hashed)).
    pub(super) source: Option<SearchedSource>,
    /// A returned line was not UTF-8: the values are decoded text.
    pub(super) not_utf8: bool,
    /// Per searched term: its matches or matched lines (see
    /// [`crate::types::TextSearchFile::term_counts`]).
    pub(super) term_counts: Vec<u32>,
}

/// Everything one search accumulated. Totals (`files_matched`, `submatches`,
/// `matched_lines`) count every kept file, including files the collection cap
/// later drops from `recs`.
pub(super) struct CollectResult {
    pub(super) recs: Vec<FileRec>,
    pub(super) files_searched: u64,
    pub(super) bytes_searched: u64,
    pub(super) files_matched: u64,
    pub(super) submatches: u64,
    pub(super) matched_lines: u64,
    /// A per-file only-matching span cap was hit (`maxOnlyMatchingPerFile`).
    pub(super) span_capped: bool,
    /// The PCRE2 wall-clock deadline was hit (`pcre2Deadline`).
    pub(super) timed_out: bool,
    /// At least one file exceeded the byte ceiling and was skipped (`maxFileSize`).
    pub(super) size_skipped: bool,
    /// At least one file was quit as binary (`binaryQuit`); coverage is partial.
    pub(super) binary_quit: bool,
    /// Paths of binary-quit files (bounded) and their total count.
    pub(super) binary_files: Vec<String>,
    pub(super) binary_file_count: u32,
    pub(super) skipped_binary_count: u32,
    pub(super) file_terms_dropped: u32,
    pub(super) depth_cut_dirs: u32,
    /// Skipped binary files per lowercased extension, most files first.
    pub(super) skipped_binary_extensions: Vec<BinaryExtensionCount>,
    /// Root-relative directories the prune list skipped, sorted.
    pub(super) pruned_dirs: Vec<String>,
    /// The caller cancelled the search before the walk finished (`cancelled`).
    pub(super) cancelled: bool,
    pub(super) error_count: u32,
    pub(super) first_error: Option<String>,
    pub(super) pattern_error_count: u32,
    pub(super) test_code_excluded: crate::types::TestCodeExcluded,
}

/// Shared accumulation state for one search. Walk workers write finished files
/// and counters here; the PCRE2 driver can read a consistent snapshot of the
/// files finished so far while a worker is still stuck in a match, and raise
/// `stop` so every worker quits at its next check.
pub(super) struct CollectState {
    pub(super) recs: Mutex<Vec<FileRec>>,
    pub(super) files_searched: AtomicU64,
    pub(super) bytes_searched: AtomicU64,
    pub(super) files_matched: AtomicU64,
    pub(super) submatches: AtomicU64,
    pub(super) matched_lines: AtomicU64,
    pub(super) span_capped: AtomicBool,
    pub(super) timed_out: AtomicBool,
    pub(super) size_skipped: AtomicBool,
    binary_quit: AtomicBool,
    pub(super) binary_files: Mutex<Vec<String>>,
    pub(super) binary_file_count: AtomicU64,
    pub(super) skipped_binary_count: AtomicU32,
    pub(super) file_terms_dropped: AtomicU32,
    pub(super) depth_cut_dirs: AtomicU32,
    pub(super) skipped_binary_extensions: Mutex<HashMap<String, (u32, BTreeSet<String>)>>,
    pub(super) pruned_dirs: Mutex<Vec<String>>,
    pub(super) cancelled: AtomicBool,
    /// Set when the walk must end now (deadline, cancellation, driver timeout).
    pub(super) stop: AtomicBool,
    pub(super) error_count: AtomicU64,
    pub(super) first_error: Mutex<Option<String>>,
    /// Errors raised by the matcher on a readable file (see
    /// [`TextSearchStats::pattern_error_count`](crate::types::TextSearchStats::pattern_error_count)).
    pub(super) pattern_error_count: AtomicU64,
    /// Files a test-code scope left out by path, and matched lines it left
    /// out inside searched files.
    pub(super) test_files_excluded: AtomicU32,
    pub(super) test_lines_excluded: AtomicU64,
}

impl CollectState {
    pub(super) fn new() -> Self {
        Self {
            recs: Mutex::new(Vec::new()),
            files_searched: AtomicU64::new(0),
            bytes_searched: AtomicU64::new(0),
            files_matched: AtomicU64::new(0),
            submatches: AtomicU64::new(0),
            matched_lines: AtomicU64::new(0),
            span_capped: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
            size_skipped: AtomicBool::new(false),
            binary_quit: AtomicBool::new(false),
            binary_files: Mutex::new(Vec::new()),
            binary_file_count: AtomicU64::new(0),
            skipped_binary_count: AtomicU32::new(0),
            file_terms_dropped: AtomicU32::new(0),
            depth_cut_dirs: AtomicU32::new(0),
            skipped_binary_extensions: Mutex::new(HashMap::new()),
            pruned_dirs: Mutex::new(Vec::new()),
            cancelled: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            error_count: AtomicU64::new(0),
            first_error: Mutex::new(None),
            pattern_error_count: AtomicU64::new(0),
            test_files_excluded: AtomicU32::new(0),
            test_lines_excluded: AtomicU64::new(0),
        }
    }

    /// Count a file that is binary from its leading bytes under its
    /// extension (lowercased, as structureSearch matches extensions).
    pub(super) fn record_skipped_binary(&self, path: &Path) {
        self.skipped_binary_count.fetch_add(1, Ordering::Relaxed);
        let extension = extension_of(&path.to_string_lossy(), true, "");
        let name = extension
            .is_empty()
            .then(|| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .flatten();
        {
            let mut groups = self
                .skipped_binary_extensions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let (count, names) = groups.entry(extension).or_default();
            *count = count.saturating_add(1);
            names.extend(name);
        }
    }

    /// Remember a file searched only up to its first NUL byte. The snapshot
    /// sorts the paths, so the list does not depend on which walk worker
    /// finished first.
    pub(super) fn record_binary(&self, path: &Path) {
        self.binary_quit.store(true, Ordering::Relaxed);
        self.binary_file_count.fetch_add(1, Ordering::Relaxed);
        // Every binary-quit file is named: each is a coverage gap.
        {
            let mut files = self
                .binary_files
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            files.push(path.to_string_lossy().into_owned());
        }
    }

    /// Remember a directory the prune list skipped, root-relative.
    pub(super) fn record_pruned(&self, relative: String) {
        {
            let mut dirs = self
                .pruned_dirs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            dirs.push(relative);
        }
    }

    pub(super) fn record_error(&self, message: String) {
        // `u64` cannot overflow; the snapshot saturates to `u32`, so a count
        // stuck at u32::MAX still reports incomplete coverage.
        self.error_count.fetch_add(1, Ordering::Relaxed);
        let mut detail = self
            .first_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if detail.is_none() {
            *detail = Some(message.chars().take(512).collect());
        }
    }

    /// Count one kept file in the search-wide totals. Totals are `u64`, so they
    /// cannot overflow from per-file `u32` counts; output saturates to `u32`.
    pub(super) fn record_kept(&self, submatches: u32, matched_lines: u32) {
        self.files_matched.fetch_add(1, Ordering::Relaxed);
        self.submatches
            .fetch_add(u64::from(submatches), Ordering::Relaxed);
        self.matched_lines
            .fetch_add(u64::from(matched_lines), Ordering::Relaxed);
    }

    pub(super) fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Take the files finished so far plus every counter. Files a worker
    /// finishes after the snapshot are not included.
    pub(super) fn snapshot(&self) -> CollectResult {
        let recs = std::mem::take(
            &mut *self
                .recs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let first_error = self
            .first_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        CollectResult {
            recs,
            files_searched: self.files_searched.load(Ordering::Relaxed),
            bytes_searched: self.bytes_searched.load(Ordering::Relaxed),
            files_matched: self.files_matched.load(Ordering::Relaxed),
            submatches: self.submatches.load(Ordering::Relaxed),
            matched_lines: self.matched_lines.load(Ordering::Relaxed),
            span_capped: self.span_capped.load(Ordering::Relaxed),
            timed_out: self.timed_out.load(Ordering::Relaxed),
            size_skipped: self.size_skipped.load(Ordering::Relaxed),
            binary_quit: self.binary_quit.load(Ordering::Relaxed),
            binary_files: {
                let mut files = self
                    .binary_files
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                files.sort();
                files
            },
            binary_file_count: saturating_u32(self.binary_file_count.load(Ordering::Relaxed)),
            skipped_binary_count: self.skipped_binary_count.load(Ordering::Relaxed),
            file_terms_dropped: self.file_terms_dropped.load(Ordering::Relaxed),
            depth_cut_dirs: self.depth_cut_dirs.load(Ordering::Relaxed),
            pruned_dirs: {
                let mut dirs = self
                    .pruned_dirs
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                dirs.sort();
                dirs
            },
            skipped_binary_extensions: {
                let groups = self
                    .skipped_binary_extensions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut groups = groups
                    .iter()
                    .map(|(extension, (count, names))| BinaryExtensionCount {
                        extension: extension.clone(),
                        count: *count,
                        names: names.iter().cloned().collect(),
                    })
                    .collect::<Vec<_>>();
                groups.sort_by(|a, b| {
                    b.count
                        .cmp(&a.count)
                        .then_with(|| a.extension.cmp(&b.extension))
                });
                groups
            },
            cancelled: self.cancelled.load(Ordering::Relaxed),
            error_count: saturating_u32(self.error_count.load(Ordering::Relaxed)),
            first_error,
            pattern_error_count: saturating_u32(self.pattern_error_count.load(Ordering::Relaxed)),
            test_code_excluded: crate::types::TestCodeExcluded {
                files: self.test_files_excluded.load(Ordering::Relaxed),
                lines: saturating_u32(self.test_lines_excluded.load(Ordering::Relaxed)),
            },
        }
    }
}
