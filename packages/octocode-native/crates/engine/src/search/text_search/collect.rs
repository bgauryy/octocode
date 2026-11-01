//! The parallel walk: per-worker retention of records and the walk driver.

use super::file_search::{FileSearcher, build_searcher};
use super::file_terms::FileTerms;
use super::mode::{
    MatchWork, Mode, alternation_words, identifier_search, lists_match_density, match_work,
    only_matching, ranks_by_relevance,
};
use super::result::preserves_traversal_order;
use super::state::{CollectState, FileRec};
use super::visit::WalkScope;
use super::walk_rule::build_walk_builder;
use super::{
    DEFAULT_MAX_LINE_SEARCH_FILE_BYTES, DEFAULT_MAX_SEARCH_FILE_BYTES, TextSearchPathFilter,
};
use crate::error::Result;
use crate::search::relevance;
use crate::search::walk::Prune;
use crate::types::{PatternSyntax, SearchContext, SourceValues, TextSearchOptions};
use grep_matcher::Matcher;
use grep_searcher::BinaryDetection;
use ignore::WalkState;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Per-worker collection buffer. Workers push into their own `Vec` and merge
/// into the shared state on drop, after `build_parallel().run()` finishes,
/// instead of taking a global mutex per matched file. With `eager` set (the
/// PCRE2 path) every file is merged at once so the driver's snapshot sees it.
pub(super) struct WorkerRecs<'a> {
    pub(super) local: Vec<FileRec>,
    pub(super) state: &'a CollectState,
    eager: bool,
}

impl WorkerRecs<'_> {
    pub(super) fn push(&mut self, rec: FileRec) {
        self.state.record_kept(rec.submatches, rec.matched_lines);
        if self.eager {
            {
                let mut shared = self
                    .state
                    .recs
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                shared.push(rec);
            }
        } else {
            self.local.push(rec);
        }
    }
}

impl Drop for WorkerRecs<'_> {
    fn drop(&mut self) {
        if self.local.is_empty() {
            return;
        }
        {
            let mut shared = self
                .state
                .recs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            shared.append(&mut self.local);
        }
    }
}

/// How a walk is bounded and where finished files go.
#[derive(Clone, Copy)]
pub(super) struct WalkControl<'a> {
    /// Wall-clock ceiling (PCRE2 only); `None` for linear engines.
    pub(super) deadline: Option<Instant>,
    /// Caller cancellation, polled before every walk entry.
    pub(super) cancelled: &'a (dyn Fn() -> bool + Sync),
    /// Merge each finished file into the shared state immediately.
    pub(super) eager: bool,
}

/// Run a parallel ignore walk + per-file search for a concrete matcher type,
/// writing into `state`. The walk stops at the next entry once `cancelled()`
/// returns true, `state.stop` is raised, or `deadline` passes.
pub(super) fn collect<M: Matcher + Sync>(
    opts: &TextSearchOptions,
    (matcher, terms): (&M, &[M]),
    mode: Mode,
    path_filter: Arc<dyn TextSearchPathFilter>,
    state: &CollectState,
    control: WalkControl<'_>,
) -> Result<()> {
    let WalkControl {
        deadline, eager, ..
    } = control;
    let only_matching = only_matching(opts);
    // only-matching emits bare spans; only-matching ignores context too.
    let context = if mode == Mode::Normal && !only_matching {
        opts.context
    } else {
        SearchContext::default()
    };

    let max_file_bytes =
        opts.max_file_bytes
            .map(u64::from)
            .unwrap_or(if opts.multiline.unwrap_or(false) {
                DEFAULT_MAX_SEARCH_FILE_BYTES
            } else {
                DEFAULT_MAX_LINE_SEARCH_FILE_BYTES
            });

    // Traversal ordering is only stable if a single worker drains the walk in a
    // fixed order; the default parallel walk merges worker buffers arbitrarily.
    let (mut walk_builder, rule) = build_walk_builder(opts)?;
    // Otherwise the walk takes this call's share of the process CPU budget
    // (`walk_builder`, `search::cpu`).
    if preserves_traversal_order(opts) {
        walk_builder.threads(1);
    }

    let scope = WalkScope {
        opts,
        prune: Prune::new(opts.exclude_dir.iter().flatten()),
        rule,
        state,
        control,
        max_file_bytes,
        keep_unmatched: mode == Mode::FilesWithoutMatch,
        identifier: identifier_search(opts),
        file_terms: FileTerms::new(opts),
        attributes: ranks_by_relevance(opts)
            .then(|| relevance::GitAttributes::new(Path::new(&opts.path)))
            .flatten(),
        words: ranks_by_relevance(opts)
            .then(|| alternation_words(opts))
            .flatten(),
    };
    let scope = &scope;
    let worker = || {
        let path_filter = Arc::clone(&path_filter);
        let mut worker_recs = WorkerRecs {
            local: Vec::new(),
            state,
            eager,
        };
        let mut files = FileSearcher {
            opts,
            matcher,
            searcher: build_searcher(opts, context, BinaryDetection::quit(b'\x00')),
            prefix_searcher: None,
            context,
            // `mode`/`only_matching` are invariant for the whole search.
            work: MatchWork {
                deferred_line: opts.values == SourceValues::Deferred
                    && opts.syntax != PatternSyntax::Pcre2
                    && context.is_empty()
                    && !only_matching
                    && !opts.multiline.unwrap_or(false),
                weigh_lines: ranks_by_relevance(opts)
                    && lists_match_density(mode)
                    && !opts.invert_match.unwrap_or(false),
                ..match_work(mode, only_matching)
            },
            deadline,
            stop: &state.stop,
            keep_head: ranks_by_relevance(opts),
            copy: scope.file_terms.is_some(),
            words: scope.words.as_ref(),
            terms,
        };

        move |dent| scope.visit(dent, &*path_filter, &mut files, &mut worker_recs)
    };
    // A file root is one entry: the parallel walker would spawn its pool and
    // poll in idle 1 ms sleeps to search it, so it is searched on this thread.
    if Path::new(&opts.path).is_file() {
        let mut visit = worker();
        for dent in walk_builder.build() {
            if matches!(visit(dent), WalkState::Quit) {
                break;
            }
        }
    } else {
        walk_builder.build_parallel().run(|| Box::new(worker()));
    }
    Ok(())
}
