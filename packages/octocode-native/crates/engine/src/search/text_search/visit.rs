//! One walk entry: admission, the ceiling, the search, and the record kept.

use super::TextSearchPathFilter;
use super::collect::{WalkControl, WorkerRecs};
use super::file_search::{FileOutcome, FileSearcher, hashed_source, read_prefix};
use super::file_terms::FileTerms;
use super::mode::ranks_by_relevance;
use super::state::{CollectState, FileRec};
use super::walk_rule::{EntryRule, Skipped, capture_sort_time};
use crate::search::relevance;
use crate::search::walk::Prune;
use crate::types::TextSearchOptions;
use aho_corasick::AhoCorasick;
use grep_matcher::Matcher;
use ignore::WalkState;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

/// Walk-wide settings and shared state every worker reads while visiting.
pub(super) struct WalkScope<'a> {
    pub(super) opts: &'a TextSearchOptions,
    /// Directories never walked (`exclude_dir`), recorded when skipped.
    pub(super) prune: Prune,
    /// Ignored and dot entries left out, reported when skipped.
    pub(super) rule: Option<EntryRule>,
    pub(super) state: &'a CollectState,
    pub(super) control: WalkControl<'a>,
    pub(super) max_file_bytes: u64,
    /// Files-without-match: keep unmatched files instead of matched ones.
    pub(super) keep_unmatched: bool,
    pub(super) identifier: bool,
    pub(super) file_terms: Option<FileTerms>,
    pub(super) attributes: Option<relevance::GitAttributes>,
    pub(super) words: Option<AhoCorasick>,
}

impl WalkScope<'_> {
    /// Search one walk entry and keep its record; returns how the walk goes on.
    pub(super) fn visit<M: Matcher>(
        &self,
        dent: std::result::Result<ignore::DirEntry, ignore::Error>,
        path_filter: &dyn TextSearchPathFilter,
        files: &mut FileSearcher<'_, M>,
        worker_recs: &mut WorkerRecs<'_>,
    ) -> WalkState {
        let (opts, state) = (self.opts, self.state);
        let dent = match self.admit(dent, path_filter) {
            Ok(dent) => dent,
            Err(walk_state) => return walk_state,
        };
        let path: &Path = dent.path();
        let Some((file, meta)) = self.open_within_ceiling(&dent) else {
            return WalkState::Continue;
        };
        let file_len = meta.len();
        let Some(outcome) = self.search_file(files, &file, path) else {
            return WalkState::Continue;
        };
        let relative = path
            .strip_prefix(&opts.path)
            .unwrap_or(path)
            .to_string_lossy();
        let generated = ranks_by_relevance(opts)
            && outcome.matched_lines > 0
            && (relevance::is_generated_path(&relative)
                || self
                    .attributes
                    .as_ref()
                    .is_some_and(|marks| marks.marks_generated(path))
                || outcome.generated_header.unwrap_or_else(|| {
                    read_prefix(&file, relevance::GENERATED_HEADER_BYTES)
                        .is_ok_and(|prefix| relevance::has_generated_header(&prefix))
                }));
        if outcome.span_cap_reached {
            state.span_capped.store(true, Ordering::Relaxed);
        }
        if outcome.deadline_hit {
            state.timed_out.store(true, Ordering::Relaxed);
        }
        state.files_searched.fetch_add(1, Ordering::Relaxed);
        state.bytes_searched.fetch_add(file_len, Ordering::Relaxed);
        if outcome.opaque {
            // Binary from its leading bytes: outside a text search, so
            // any bytes that happened to match are not hits.
            state.record_skipped_binary(path);
            return WalkState::Continue;
        } else if outcome.binary {
            // Text after the NUL was not searched: coverage is partial.
            state.record_binary(path);
        }

        let has_match = outcome.matched_lines > 0;
        // A binary file quit before any match is *unknown*, not "without
        // match": a files-without view never lists it.
        if outcome.binary && !has_match && self.keep_unmatched {
            return WalkState::Continue;
        }
        if has_match == self.keep_unmatched {
            // Normal/files-only/count modes keep matched files;
            // files-without-match keeps the rest.
            return WalkState::Continue;
        }

        let copied = outcome.copied;
        if self
            .file_terms
            .as_ref()
            .is_some_and(|terms| !terms.pass(copied, &file))
        {
            state.file_terms_dropped.fetch_add(1, Ordering::Relaxed);
            return WalkState::Continue;
        }
        let demoted =
            generated || (ranks_by_relevance(opts) && relevance::is_demoted_path(&relative));
        // A stopped walk keeps what it found without reading further.
        let source = (opts.values.hashed() && !state.stopped())
            .then(|| hashed_source(&file, &meta))
            .flatten();
        drop(file);
        worker_recs.push(FileRec {
            path: dent.path().to_string_lossy().into_owned(),
            entry: outcome.entry,
            matched_lines: outcome.matched_lines,
            submatches: outcome.submatches,
            om_matches: outcome.om_matches,
            sort_time: capture_sort_time(opts, &dent),
            line_weight: outcome.line_weight,
            covered: outcome.covered,
            demoted,
            generated,
            declares: outcome.declares && self.identifier,
            source,
        });
        WalkState::Continue
    }

    /// The entry when it is a file to search, or the walk state to return
    /// for it (stop, cancellation, deadline, walk error, filter, prune).
    fn admit(
        &self,
        dent: std::result::Result<ignore::DirEntry, ignore::Error>,
        path_filter: &dyn TextSearchPathFilter,
    ) -> std::result::Result<ignore::DirEntry, WalkState> {
        let (opts, state) = (self.opts, self.state);
        // Between entries: end the walk on stop, cancellation, or the
        // deadline, keeping the files already finished.
        if state.stopped() {
            return Err(WalkState::Quit);
        }
        if (self.control.cancelled)() {
            state.cancelled.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Err(WalkState::Quit);
        }
        if let Some(deadline) = self.control.deadline
            && Instant::now() >= deadline
        {
            state.timed_out.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Err(WalkState::Quit);
        }
        let dent = match dent {
            Ok(d) => d,
            Err(error) => {
                state.record_error(error.to_string());
                return Err(WalkState::Continue);
            }
        };
        let is_dir = dent.file_type().is_some_and(|kind| kind.is_dir());
        let skip = || {
            if is_dir {
                WalkState::Skip
            } else {
                WalkState::Continue
            }
        };
        match self
            .rule
            .as_ref()
            .and_then(|rule| rule.skips(&dent, is_dir))
        {
            Some(Skipped::Ignored) => {
                path_filter.skipped_ignored(dent.path(), is_dir);
                return Err(skip());
            }
            Some(Skipped::Hidden) => {
                path_filter.skipped_hidden(dent.path(), is_dir);
                return Err(skip());
            }
            None => {}
        }
        if !path_filter.allows_entry(dent.path(), dent.file_type()) {
            return Err(skip());
        }
        if self.prune.prunes_entry(&dent) {
            let relative = dent.path().strip_prefix(&opts.path).unwrap_or(dent.path());
            state.record_pruned(crate::text::slash_path(relative));
            return Err(WalkState::Skip);
        }
        // A directory at the depth limit is yielded but never entered (the
        // walk counts root files as `max_depth` 0): one that holds entries
        // is a part of the tree left unsearched.
        if is_dir
            && opts.max_depth.map(|max| max as usize + 1) == Some(dent.depth())
            && std::fs::read_dir(dent.path()).is_ok_and(|mut entries| entries.next().is_some())
        {
            state.depth_cut_dirs.fetch_add(1, Ordering::Relaxed);
        }
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            if dent.path_is_symlink() {
                path_filter.skipped_symlink(dent.path());
            }
            return Err(WalkState::Continue);
        }
        Ok(dent)
    }

    /// Whether a file of `len` bytes is above the byte ceiling (recorded as
    /// a size skip).
    fn over_ceiling(&self, len: u64) -> bool {
        if len > self.max_file_bytes {
            self.state.size_skipped.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// The entry's opened file and metadata, or `None` when it is above the
    /// byte ceiling or fails to open (recorded as an error).
    fn open_within_ceiling(
        &self,
        dent: &ignore::DirEntry,
    ) -> Option<(std::fs::File, std::fs::Metadata)> {
        // Skip files above the byte ceiling before opening them, so a
        // pathological multi-GB file cannot force a giant buffer + lossy
        // copy. Surfaced as `maxFileSize`; not counted in `files_searched`.
        if dent
            .metadata()
            .ok()
            .is_some_and(|meta| self.over_ceiling(meta.len()))
        {
            return None;
        }
        let path = dent.path();
        // Keep successful files, but report incomplete coverage when
        // opening or matching fails. A skipped file proves no absence.
        let (file, meta) = match crate::text::source::open_regular(path) {
            Ok(opened) => opened,
            Err(error) => {
                self.state
                    .record_error(format!("{}: {error}", path.display()));
                return None;
            }
        };
        if self.over_ceiling(meta.len()) {
            return None;
        }
        Some((file, meta))
    }

    /// The file's search outcome, or `None` when the search failed (recorded
    /// as an error, and as a pattern error when the matcher raised it).
    fn search_file<M: Matcher>(
        &self,
        files: &mut FileSearcher<'_, M>,
        file: &std::fs::File,
        path: &Path,
    ) -> Option<FileOutcome> {
        match files.search(file) {
            Ok(outcome) => Some(outcome),
            Err(error) => {
                // The file opened; std never reports a read failure as
                // `Other`, which carries the matcher's own errors (the
                // searcher's and this sink's `io::Error::other`).
                if error.kind() == std::io::ErrorKind::Other {
                    self.state
                        .pattern_error_count
                        .fetch_add(1, Ordering::Relaxed);
                }
                // Root-relative, or the file's name when it is the root.
                let shown = path
                    .strip_prefix(&self.opts.path)
                    .ok()
                    .filter(|relative| !relative.as_os_str().is_empty())
                    .or_else(|| path.file_name().map(Path::new))
                    .unwrap_or(path);
                self.state
                    .record_error(format!("{}: {error}", shown.display()));
                None
            }
        }
    }
}
