//! One walk entry: admission, the ceiling, the search, and the record kept.

use super::TextSearchPathFilter;
use super::collect::WorkerRecs;
use super::file_search::{FileOutcome, FileSearcher};
use super::file_terms::FileTerms;
use super::state::{CollectState, FileRec};
use super::walk_rule::{EntryRule, Selection, Skipped, capture_sort_time};
use crate::search::walk::Probe;
use crate::types::TextSearchOptions;
use grep_matcher::Matcher;
use ignore::WalkState;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl<'a> Probe<'a> {
    /// The entry the `ignore` walker yielded.
    pub(super) fn of(dent: &'a ignore::DirEntry) -> Self {
        Probe {
            path: dent.path(),
            depth: dent.depth(),
            file_type: dent.file_type(),
        }
    }
}

/// Walk-wide settings and shared state every worker reads while visiting.
pub(super) struct WalkScope<'a> {
    pub(super) opts: &'a TextSearchOptions,
    /// Ignored and dot entries left out, reported when skipped.
    pub(super) rule: Option<Arc<EntryRule>>,
    /// What the walker drops inside itself (globs, `language`).
    pub(super) selection: Selection,
    pub(super) state: &'a CollectState,
    /// The caller's cancellation, recorded in `state` whenever it is seen:
    /// a cancel first seen inside a file (its read, its matched lines) cuts
    /// the result as surely as one seen between walk entries, so the result
    /// says it was cancelled wherever it was seen.
    pub(super) cancelled: Box<dyn Fn() -> bool + Sync + 'a>,
    pub(super) max_file_bytes: u64,
    /// Files-without-match: keep unmatched files instead of matched ones.
    pub(super) keep_unmatched: bool,
    pub(super) file_terms: Option<FileTerms>,
    /// Which files the search's judge need not judge.
    pub(super) judge_gate: super::judge_gate::JudgeGate,
    /// A cancelled walk lets the files it handed out finish and commit
    /// instead of stopping them mid-file (a stream: what it read before the
    /// cut is the same on every run).
    pub(super) drains: bool,
    /// How many workers open files at once; none: all of them.
    pub(super) open_gate: Option<super::open_gate::OpenGate>,
}

impl WalkScope<'_> {
    /// Count each ignore file the walk could not read as an error of the
    /// search: its rules did not apply. Rule files load as the walk meets
    /// them, so call this once the walk is done.
    pub(super) fn record_unreadable_ignore_files(&self) {
        let Some(rule) = &self.rule else {
            return;
        };
        for file in rule.unreadable_ignore_files() {
            self.state.record_error(format!(
                "{}: ignore file unreadable ({:?}); its rules were not applied",
                file.path.display(),
                file.kind
            ));
        }
    }

    /// Search one walk entry and keep its record; returns how the walk goes on.
    pub(super) fn visit<M: Matcher>(
        &self,
        dent: std::result::Result<ignore::DirEntry, ignore::Error>,
        path_filter: &dyn TextSearchPathFilter,
        files: &mut FileSearcher<'_, M>,
        worker_recs: &mut WorkerRecs<'_>,
    ) -> WalkState {
        match self.admit(dent, path_filter) {
            Ok(dent) => {
                self.search_entry(self.state, dent.path(), files, worker_recs);
                WalkState::Continue
            }
            Err(walk_state) => walk_state,
        }
    }

    /// Search one admitted file and keep its record, counting it in `state`.
    pub(super) fn search_entry<M: Matcher>(
        &self,
        state: &CollectState,
        path: &Path,
        files: &mut FileSearcher<'_, M>,
        worker_recs: &mut WorkerRecs<'_>,
    ) {
        let opts = self.opts;
        // A test-code scope leaves out, unread, every file whose location
        // (path or mount) puts it on the other side; a file that may mix
        // both is searched and its hits are kept by line.
        if let Some(scope) = &opts.test_code {
            let test = (scope.unread_test_path)(path);
            let searched = if scope.keep_test {
                test || (scope.may_hold)(path)
            } else {
                !test
            };
            if !searched {
                state.test_files_excluded.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // A line scope leaves out, unread, every file it cannot hold a kept
        // line of.
        if opts
            .line_scope
            .as_ref()
            .is_some_and(|scope| !(scope.holds)(path))
        {
            state.scope_left_out.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let Some((file, meta, outcome)) = self.search_settled(state, files, path) else {
            return;
        };
        let file_len = meta.len();
        if self.left_out_whole(path, &outcome) {
            state.test_files_excluded.fetch_add(1, Ordering::Relaxed);
            return;
        }
        state
            .test_lines_excluded
            .fetch_add(u64::from(outcome.test_lines_excluded), Ordering::Relaxed);
        state.files_searched.fetch_add(1, Ordering::Relaxed);
        state.bytes_searched.fetch_add(file_len, Ordering::Relaxed);
        if outcome.opaque {
            // Binary from its leading bytes: outside a text search, so
            // any bytes that happened to match are not hits.
            state.record_skipped_binary(path);
            return;
        } else if outcome.binary {
            // Text after the NUL was not searched: coverage is partial.
            state.record_binary(path);
        }

        let has_match = outcome.matched_lines > 0;
        // A binary file quit before any match is *unknown*, not "without
        // match": a files-without view never lists it.
        if outcome.binary && !has_match && self.keep_unmatched {
            return;
        }
        if has_match == self.keep_unmatched {
            // Normal/files-only/count modes keep matched files;
            // files-without-match keeps the rest.
            return;
        }

        let copied = outcome.copied;
        if self
            .file_terms
            .as_ref()
            .is_some_and(|terms| !terms.pass(copied, &file))
        {
            state.file_terms_dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // Every kept file, a stopped walk's too, carries the digest of the
        // bytes its values were made from.
        let source = opts
            .values
            .hashed()
            .then(|| files.searched_source(&file, &meta, outcome.served))
            .flatten();
        drop(file);
        let sort_time = capture_sort_time(opts, path);
        let path = path.to_string_lossy().into_owned();
        // The rows stay held with the record, and the record itself too.
        let mut charge = outcome.charge;
        if !charge.add(super::state::record_weight(&path, &outcome.term_counts)) {
            files.stop.store(true, Ordering::Relaxed);
        }
        charge.keep();
        worker_recs.push(FileRec {
            path,
            entry: outcome.entry,
            matched_lines: outcome.matched_lines,
            submatches: outcome.submatches,
            om_matches: outcome.om_matches,
            sort_time,
            source,
            not_utf8: outcome.not_utf8,
            utf8_bom: outcome.utf8_bom,
            term_counts: outcome.term_counts,
        });
    }

    /// Whether the test-code scope leaves out the searched file `path`
    /// whole by its spans ([`crate::types::wholly_left_out`]): it is then
    /// left out as a file, as one left unread by its location is, never
    /// listed and never counted by its lines. The spans are those its hits
    /// read; a file without hits has them read only when a files-without
    /// view would list it.
    fn left_out_whole(&self, path: &Path, outcome: &FileOutcome) -> bool {
        let Some(scope) = &self.opts.test_code else {
            return false;
        };
        let spans = match &outcome.test_spans {
            Some(spans) => std::sync::Arc::clone(spans),
            None if self.keep_unmatched && outcome.matched_lines == 0 && (scope.may_hold)(path) => {
                if (scope.test_path)(path) {
                    std::sync::Arc::new(vec![crate::types::WHOLE_FILE])
                } else {
                    (scope.spans)(path)
                }
            }
            None => return false,
        };
        crate::types::wholly_left_out(scope.keep_test, &spans)
    }

    /// Whether the walk must end before its next entry: stopped, or the
    /// caller cancelled (recorded, and every worker stops unless the walk
    /// [`drains`](Self::drains)), keeping the files already finished.
    pub(super) fn halted(&self) -> bool {
        if self.state.stopped() {
            return true;
        }
        if (self.cancelled)() {
            if !self.drains {
                self.state.stop.store(true, Ordering::Relaxed);
            }
            return true;
        }
        false
    }

    /// The entry when it is a file to search, or the walk state to return
    /// for it (stop, cancellation, walk error, filter).
    pub(super) fn admit(
        &self,
        dent: std::result::Result<ignore::DirEntry, ignore::Error>,
        path_filter: &dyn TextSearchPathFilter,
    ) -> std::result::Result<ignore::DirEntry, WalkState> {
        let state = self.state;
        if self.halted() {
            return Err(WalkState::Quit);
        }
        let dent = match dent {
            Ok(d) => d,
            Err(error) => {
                state.record_error(error.to_string());
                return Err(WalkState::Continue);
            }
        };
        let probe = Probe::of(&dent);
        let is_dir = probe.is_dir();
        if left_out(self.rule.as_deref(), path_filter, probe, is_dir) {
            return Err(if is_dir {
                WalkState::Skip
            } else {
                WalkState::Continue
            });
        }
        self.note_depth_cut(probe, is_dir);
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            if dent.path_is_symlink() {
                path_filter.skipped_symlink(dent.path());
            }
            return Err(WalkState::Continue);
        }
        Ok(dent)
    }

    /// A directory at the depth limit is yielded but never entered (the
    /// walk counts root files as `max_depth` 0): one that holds entries is a
    /// part of the tree left unsearched.
    pub(super) fn note_depth_cut(&self, entry: Probe<'_>, is_dir: bool) {
        if is_dir
            && self.opts.max_depth.map(|max| max as usize + 1) == Some(entry.depth)
            && std::fs::read_dir(entry.path).is_ok_and(|mut entries| entries.next().is_some())
        {
            self.state.depth_cut_dirs.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Whether a file of `len` bytes is above the byte ceiling (recorded as
    /// a size skip).
    fn over_ceiling(&self, state: &CollectState, len: u64) -> bool {
        if len > self.max_file_bytes {
            state.size_skipped.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// The entry's opened file and metadata, or `None` when it is above the
    /// byte ceiling or fails to open (recorded as an error).
    fn open_within_ceiling(
        &self,
        state: &CollectState,
        path: &Path,
    ) -> Option<(std::fs::File, std::fs::Metadata)> {
        // Keep successful files, but report incomplete coverage when
        // opening or matching fails. A skipped file proves no absence.
        let permit = self
            .open_gate
            .as_ref()
            .map(super::open_gate::OpenGate::enter);
        let opened = crate::text::open_regular(path);
        drop(permit);
        let (file, meta) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                state.record_error(format!("{}: {error}", path.display()));
                return None;
            }
        };
        // Skip files above the byte ceiling before reading them, so a
        // pathological multi-GB file cannot force a giant buffer + lossy
        // copy: the open read nothing, and its `fstat` is the size of what
        // would be read. Surfaced as `fileTooLarge`; not counted in
        // `files_searched`.
        if self.over_ceiling(state, meta.len()) {
            return None;
        }
        Some((file, meta))
    }

    /// The file at `path`, opened and searched, with hits from one read of
    /// it proved one version's ([`crate::text::Settling`]): a search a
    /// rewrite in place tore may have joined the old version's start to the
    /// new one's rest, so a matched file whose stamp moved while it was
    /// searched is opened and searched again, until one search of it settles
    /// or the walk halts. A file that only grew while it was searched (a live
    /// log) keeps the hits of a search whose bytes are the start of one
    /// version of it ([`Self::proved_prefix`]), as of their length. A search
    /// that failed while a write moved its file is made again the same way:
    /// the failure may be the write's. One that never settles is counted as
    /// a file that kept changing while it was read, and none of its hits is
    /// kept. `None` when it is not searched.
    fn search_settled<M: Matcher>(
        &self,
        state: &CollectState,
        files: &mut FileSearcher<'_, M>,
        path: &Path,
    ) -> Option<(std::fs::File, std::fs::Metadata, FileOutcome)> {
        use crate::text::{Proved, Settled};
        let mut settling = crate::text::Settling::default();
        let (stop, cancelled) = (files.stop, files.cancelled);
        let live = || {
            !self.halted() && !stop.load(Ordering::Relaxed) && !cancelled.is_some_and(|gone| gone())
        };
        loop {
            let (file, meta) = self.open_within_ceiling(state, path)?;
            let outcome = match files.search(&file, path) {
                Ok(outcome) => outcome,
                // A stopped search ends the file it waited or worked on: no
                // error of the file's own.
                Err(error)
                    if (error.kind() == std::io::ErrorKind::Interrupted
                        || super::file_search::is_stopped_read(&error))
                        && files.halted() =>
                {
                    return None;
                }
                Err(error) => {
                    match settling.settled(&file, &meta) {
                        Settled::Stable(_) => self.record_failure(state, path, &error),
                        Settled::Again if live() => continue,
                        Settled::Again | Settled::Unsettled => self.record_changed(state, path),
                    }
                    return None;
                }
            };
            // A file no hit matched shows no text of it.
            if outcome.matched_lines == 0 {
                return Some((file, meta, outcome));
            }
            // A judged search keeps the hits of a proved prefix only by the
            // judge's verdict on those bytes, held ([`Self::proved_prefix`]),
            // which are admitted until it is made. Without that judgment,
            // the file is searched again.
            let judged = self.opts.judge.is_some();
            let held_judge = self
                .opts
                .judge
                .as_ref()
                .and_then(|judge| judge.held.as_ref());
            let mut admitted = None;
            let taken = || {
                if outcome.binary || (judged && held_judge.is_none()) {
                    return None;
                }
                let (len, digest) = files.taken(&file)?;
                if let Some(held_judge) = held_judge {
                    let bytes = usize::try_from(len)
                        .ok()
                        .filter(|_| len <= held_judge.limit)?;
                    admitted = Some(files.admit(bytes.saturating_mul(3)).ok()?);
                }
                Some(crate::text::Taken::Hashed {
                    len,
                    digest,
                    keep: held_judge.is_some(),
                })
            };
            match settling.settled_read(&file, &meta, taken, &live) {
                Proved::Whole(_) => return Some((file, meta, outcome)),
                Proved::Prefix(prefix) => {
                    let kept =
                        Self::proved_prefix(files, path, outcome, (held_judge, &prefix.bytes));
                    drop(admitted);
                    return match kept {
                        Ok(outcome) => {
                            state.record_grown(path, prefix.len);
                            Some((file, meta, outcome))
                        }
                        Err(error) => {
                            self.record_failure(state, path, &error);
                            None
                        }
                    };
                }
                Proved::Again if live() => {}
                Proved::Again | Proved::Unsettled => {
                    self.record_changed(state, path);
                    return None;
                }
            }
        }
    }

    /// The outcome kept of a file whose search read `prefix` of its own
    /// bytes, proved the start of one version of it
    /// ([`crate::text::Proved::Prefix`]): by its judge's verdict on those
    /// bytes held (`held`; none for a search without a judge, whose `prefix`
    /// is not kept), the search of the text it serves of them, else of the
    /// bytes themselves. The search's own outcome stands when it is of those
    /// same bytes, as they are.
    fn proved_prefix<M: Matcher>(
        files: &mut FileSearcher<'_, M>,
        path: &Path,
        outcome: FileOutcome,
        (held, prefix): (Option<&crate::types::JudgeHeld>, &[u8]),
    ) -> std::io::Result<FileOutcome> {
        let served = held.and_then(|held| (held.serve)(path, prefix));
        if served.is_none() && !outcome.served {
            return Ok(outcome);
        }
        drop(outcome);
        let utf16 = crate::text::has_utf16_bom(prefix);
        let mut kept = match &served {
            Some(served) => files.search_proved(served.into(), path, (utf16, true))?,
            None => files.search_proved(
                super::file_search::Searched::own(prefix),
                path,
                (utf16, false),
            )?,
        };
        kept.utf8_bom = kept.matched_lines > 0 && !utf16 && crate::text::bom_len(prefix) > 0;
        Ok(kept)
    }

    /// Count the failed search of `path` as an error of the search, and as
    /// a pattern error when the matcher raised it.
    fn record_failure(&self, state: &CollectState, path: &Path, error: &std::io::Error) {
        // The file opened; std never reports a read failure as `Other`,
        // which carries the matcher's own errors (the searcher's and this
        // sink's `io::Error::other`), unless it is a read of a file that kept
        // changing.
        if crate::text::is_changed_while_read(error) {
            state.changed_error_count.fetch_add(1, Ordering::Relaxed);
        } else if error.kind() == std::io::ErrorKind::Other {
            state.pattern_error_count.fetch_add(1, Ordering::Relaxed);
        }
        state.record_error(format!("{}: {error}", self.shown(path).display()));
    }

    /// Count `path` as a file the search could not read because it kept
    /// changing while it was read.
    fn record_changed(&self, state: &CollectState, path: &Path) {
        state.changed_error_count.fetch_add(1, Ordering::Relaxed);
        state.record_error(format!(
            "{}: {}",
            self.shown(path).display(),
            crate::text::changed_while_read()
        ));
    }

    /// `path` as a file's error names it: relative to the search's root, or
    /// the file's name when it is the root.
    fn shown<'p>(&self, path: &'p Path) -> &'p Path {
        path.strip_prefix(&self.opts.path)
            .ok()
            .filter(|relative| !relative.as_os_str().is_empty())
            .or_else(|| path.file_name().map(Path::new))
            .unwrap_or(path)
    }
}

#[cfg(test)]
mod tests;

/// Whether the walk leaves `entry` out on its own: an ignore or dot rule
/// ([`EntryRule`]) or the path filter. The entry is reported to the filter
/// first, so what the rules leave out is counted, not lost.
pub(super) fn left_out(
    rule: Option<&EntryRule>,
    path_filter: &dyn TextSearchPathFilter,
    entry: Probe<'_>,
    is_dir: bool,
) -> bool {
    match rule.and_then(|rule| rule.skips(None, entry, is_dir)) {
        Some(skipped) => {
            report_left_out(path_filter, skipped, entry.path, is_dir);
            true
        }
        None => !path_filter.allows_entry(entry.path, entry.file_type),
    }
}

/// Report to the path filter an entry the rules left out.
pub(super) fn report_left_out(
    path_filter: &dyn TextSearchPathFilter,
    skipped: Skipped,
    path: &Path,
    is_dir: bool,
) {
    match skipped {
        Skipped::Ignored => path_filter.skipped_ignored(path, is_dir),
        Skipped::Hidden => path_filter.skipped_hidden(path, is_dir),
    }
}
