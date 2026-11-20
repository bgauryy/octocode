//! Search-wide accumulation: per-file records, the shared counters walk workers write, and their
//! snapshot.

mod held;
pub(super) use held::{Charge, Held, context_weight, match_weight, raw_weight, record_weight};

use crate::num::saturating_u32;
use crate::search::text_parser::FileEntry;
use crate::text::extension_of;
use crate::types::{BinaryExtensionCount, GrownFile, SearchedSource, TextSearchMatch};
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
    /// Masked spans of the text the file was searched as.
    pub(super) masked: u32,
    /// only-matching spans (empty unless the output is spans).
    pub(super) om_matches: Vec<TextSearchMatch>,
    /// Metadata timestamp captured for non-path sort keys.
    pub(super) sort_time: Option<SystemTime>,
    /// The bytes the values came from (see [`SourceValues::Hashed`](crate::types::SourceValues::Hashed)).
    pub(super) source: Option<SearchedSource>,
    /// A returned line was not UTF-8: the values are decoded text.
    pub(super) not_utf8: bool,
    /// The file starts with a UTF-8 byte-order mark the searcher dropped.
    pub(super) utf8_bom: bool,
    /// Per searched term: its matches or matched lines (see
    /// [`crate::types::TextSearchFile::term_counts`]).
    pub(super) term_counts: Vec<u32>,
}

/// Everything one search accumulated: every matched file is kept in `recs`.
#[derive(Default)]
pub(super) struct CollectResult {
    pub(super) recs: Vec<FileRec>,
    pub(super) files_searched: u64,
    pub(super) masked_spans: u64,
    pub(super) bytes_searched: u64,
    pub(super) files_matched: u64,
    pub(super) submatches: u64,
    pub(super) matched_lines: u64,
    /// At least one file exceeded the byte ceiling and was skipped (`fileTooLarge`).
    pub(super) size_skipped: bool,
    /// At least one file was quit as binary (`binaryQuit`); coverage is partial.
    pub(super) binary_quit: bool,
    /// Paths of binary-quit files (bounded) and their total count.
    pub(super) binary_files: Vec<String>,
    pub(super) binary_file_count: u32,
    pub(super) skipped_binary_count: u32,
    pub(super) file_terms_dropped: u32,
    pub(super) depth_cut_dirs: u32,
    /// Files the line scope left out unread.
    pub(super) scope_left_out: u32,
    /// Skipped binary files per lowercased extension, most files first.
    pub(super) skipped_binary_extensions: Vec<BinaryExtensionCount>,
    /// The caller cancelled the search before the walk finished (`cancelled`).
    pub(super) cancelled: bool,
    pub(super) error_count: u32,
    pub(super) first_error: Option<String>,
    pub(super) pattern_error_count: u32,
    pub(super) changed_error_count: u32,
    /// Kept files that grew while they were searched, in path order.
    pub(super) grown_files: Vec<GrownFile>,
    pub(super) test_code_excluded: crate::types::TestCodeExcluded,
}

/// Shared accumulation state for one search. Walk workers write finished files
/// and counters here, and `stop` makes every worker quit at its next check.
pub(super) struct CollectState {
    pub(super) recs: Mutex<Vec<FileRec>>,
    pub(super) files_searched: AtomicU64,
    pub(super) masked_spans: AtomicU64,
    pub(super) bytes_searched: AtomicU64,
    pub(super) files_matched: AtomicU64,
    pub(super) submatches: AtomicU64,
    pub(super) matched_lines: AtomicU64,
    pub(super) size_skipped: AtomicBool,
    binary_quit: AtomicBool,
    pub(super) binary_files: Mutex<Vec<String>>,
    pub(super) binary_file_count: AtomicU64,
    pub(super) skipped_binary_count: AtomicU32,
    pub(super) file_terms_dropped: AtomicU32,
    pub(super) depth_cut_dirs: AtomicU32,
    pub(super) scope_left_out: AtomicU32,
    pub(super) skipped_binary_extensions: Mutex<HashMap<String, (u32, BTreeSet<String>)>>,
    pub(super) cancelled: AtomicBool,
    /// Set when the walk must end now (cancellation, or a PCRE2 window's end).
    pub(super) stop: AtomicBool,
    pub(super) error_count: AtomicU64,
    pub(super) first_error: Mutex<Option<String>>,
    /// Errors raised by the matcher on a readable file (see
    /// [`TextSearchStats::pattern_error_count`](crate::types::TextSearchStats::pattern_error_count)).
    pub(super) pattern_error_count: AtomicU64,
    /// Errors of files that kept changing while they were searched (see
    /// [`TextSearchStats::changed_error_count`](crate::types::TextSearchStats::changed_error_count)).
    pub(super) changed_error_count: AtomicU64,
    /// Kept files that grew while they were searched ([`Self::record_grown`]).
    grown_files: Mutex<Vec<GrownFile>>,
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
            masked_spans: AtomicU64::new(0),
            bytes_searched: AtomicU64::new(0),
            files_matched: AtomicU64::new(0),
            submatches: AtomicU64::new(0),
            matched_lines: AtomicU64::new(0),
            size_skipped: AtomicBool::new(false),
            binary_quit: AtomicBool::new(false),
            binary_files: Mutex::new(Vec::new()),
            binary_file_count: AtomicU64::new(0),
            skipped_binary_count: AtomicU32::new(0),
            file_terms_dropped: AtomicU32::new(0),
            depth_cut_dirs: AtomicU32::new(0),
            scope_left_out: AtomicU32::new(0),
            skipped_binary_extensions: Mutex::new(HashMap::new()),
            cancelled: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            error_count: AtomicU64::new(0),
            first_error: Mutex::new(None),
            pattern_error_count: AtomicU64::new(0),
            changed_error_count: AtomicU64::new(0),
            grown_files: Mutex::new(Vec::new()),
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

    /// Remember a kept file that grew while it was searched, its hits of
    /// its first `bytes` (one version's start). The snapshot sorts them.
    pub(super) fn record_grown(&self, path: &Path, bytes: u64) {
        self.grown_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(GrownFile {
                path: path.to_string_lossy().into_owned(),
                bytes,
            });
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
            masked_spans: self.masked_spans.load(Ordering::Relaxed),
            bytes_searched: self.bytes_searched.load(Ordering::Relaxed),
            files_matched: self.files_matched.load(Ordering::Relaxed),
            submatches: self.submatches.load(Ordering::Relaxed),
            matched_lines: self.matched_lines.load(Ordering::Relaxed),
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
            scope_left_out: self.scope_left_out.load(Ordering::Relaxed),
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
                sort_extension_groups(&mut groups);
                groups
            },
            cancelled: self.cancelled.load(Ordering::Relaxed),
            error_count: saturating_u32(self.error_count.load(Ordering::Relaxed)),
            first_error,
            pattern_error_count: saturating_u32(self.pattern_error_count.load(Ordering::Relaxed)),
            changed_error_count: saturating_u32(self.changed_error_count.load(Ordering::Relaxed)),
            grown_files: {
                let mut files = self
                    .grown_files
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                files.sort_by(|a, b| a.path.cmp(&b.path));
                files
            },
            test_code_excluded: crate::types::TestCodeExcluded {
                files: self.test_files_excluded.load(Ordering::Relaxed),
                lines: saturating_u32(self.test_lines_excluded.load(Ordering::Relaxed)),
            },
        }
    }
}

impl CollectResult {
    /// What `self` and then `later` accumulated, as one search that read
    /// `self`'s files first: counts add, flags join, lists keep their order
    /// (binary-quit paths are sorted once, by [`Self::settled`]), and the
    /// first error stays the earlier one.
    pub(super) fn merge(mut self, later: Self) -> Self {
        self.recs.extend(later.recs);
        self.files_searched += later.files_searched;
        self.masked_spans += later.masked_spans;
        self.bytes_searched += later.bytes_searched;
        self.files_matched += later.files_matched;
        self.submatches += later.submatches;
        self.matched_lines += later.matched_lines;
        self.size_skipped |= later.size_skipped;
        self.binary_quit |= later.binary_quit;
        self.binary_files.extend(later.binary_files);
        self.binary_file_count = self
            .binary_file_count
            .saturating_add(later.binary_file_count);
        self.skipped_binary_count = self
            .skipped_binary_count
            .saturating_add(later.skipped_binary_count);
        self.file_terms_dropped = self
            .file_terms_dropped
            .saturating_add(later.file_terms_dropped);
        self.depth_cut_dirs = self.depth_cut_dirs.saturating_add(later.depth_cut_dirs);
        self.scope_left_out = self.scope_left_out.saturating_add(later.scope_left_out);
        for group in later.skipped_binary_extensions {
            match self
                .skipped_binary_extensions
                .iter_mut()
                .find(|known| known.extension == group.extension)
            {
                Some(known) => {
                    known.count = known.count.saturating_add(group.count);
                    let names = std::mem::take(&mut known.names)
                        .into_iter()
                        .chain(group.names)
                        .collect::<BTreeSet<_>>();
                    known.names = names.into_iter().collect();
                }
                None => self.skipped_binary_extensions.push(group),
            }
        }
        sort_extension_groups(&mut self.skipped_binary_extensions);
        self.cancelled |= later.cancelled;
        self.error_count = self.error_count.saturating_add(later.error_count);
        self.first_error = self.first_error.or(later.first_error);
        self.pattern_error_count = self
            .pattern_error_count
            .saturating_add(later.pattern_error_count);
        self.changed_error_count = self
            .changed_error_count
            .saturating_add(later.changed_error_count);
        self.grown_files.extend(later.grown_files);
        self.test_code_excluded.files = self
            .test_code_excluded
            .files
            .saturating_add(later.test_code_excluded.files);
        self.test_code_excluded.lines = self
            .test_code_excluded
            .lines
            .saturating_add(later.test_code_excluded.lines);
        self
    }

    /// The merged result as one snapshot reads it: binary-quit and grown
    /// paths sorted.
    pub(super) fn settled(mut self) -> Self {
        self.binary_files.sort();
        self.grown_files.sort_by(|a, b| a.path.cmp(&b.path));
        self
    }

    /// Heap bytes of the kept files: what holding them costs, as a search
    /// charges them to its [`Held`] bound.
    pub(super) fn weight(&self) -> usize {
        self.recs
            .iter()
            .map(|rec| {
                record_weight(&rec.path, &rec.term_counts)
                    + rec.entry.raw_matches.iter().map(raw_weight).sum::<usize>()
                    + rec
                        .entry
                        .contexts
                        .values()
                        .map(|line| context_weight(line))
                        .sum::<usize>()
                    + rec.entry.prepared.iter().map(match_weight).sum::<usize>()
                    + rec.om_matches.iter().map(match_weight).sum::<usize>()
            })
            .sum::<usize>()
            + self.binary_files.iter().map(String::len).sum::<usize>()
            + self.first_error.as_ref().map_or(0, String::len)
    }
}

/// Extension groups most files first, then by extension: one order however
/// the counts were gathered.
fn sort_extension_groups(groups: &mut [BinaryExtensionCount]) {
    groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });
}
