//! The parallel walk: per-worker retention of records and the walk driver.

use super::file_search::{FileSearcher, build_searcher};
use super::file_terms::FileTerms;
use super::mode::{
    MatchWork, Mode, alternation_words, identifier_search, lists_match_density, match_work,
    only_matching, ranks_by_relevance,
};
use super::result::preserves_traversal_order;
use super::state::{CollectState, FileRec, Held};
use super::visit::WalkScope;
use super::walk_rule::build_walk_builder;
use super::{
    DEFAULT_MAX_LINE_SEARCH_FILE_BYTES, DEFAULT_MAX_SEARCH_FILE_BYTES, TextSearchPathFilter,
};
use crate::error::Result;
use crate::search::relevance;
use crate::types::{PatternSyntax, SearchContext, SourceValues, TextSearchOptions};
use grep_matcher::Matcher;
use grep_searcher::BinaryDetection;
use ignore::WalkState;
use std::path::Path;
use std::sync::Arc;

/// Per-worker collection buffer. Workers push into their own `Vec` and merge
/// into the shared state on drop, after `build_parallel().run()` finishes,
/// instead of taking a global mutex per matched file.
pub(super) struct WorkerRecs<'a> {
    pub(super) local: Vec<FileRec>,
    pub(super) state: &'a CollectState,
}

impl WorkerRecs<'_> {
    pub(super) fn push(&mut self, rec: FileRec) {
        self.state.record_kept(rec.submatches, rec.matched_lines);
        self.local.push(rec);
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

/// How a walk is stopped.
#[derive(Clone, Copy)]
pub(super) struct WalkControl<'a> {
    /// Caller cancellation, polled before every walk entry.
    pub(super) cancelled: &'a (dyn Fn() -> bool + Sync),
}

/// The largest file a search of `opts` opens.
pub(super) fn byte_ceiling(opts: &TextSearchOptions) -> u64 {
    opts.max_file_bytes
        .map(u64::from)
        .unwrap_or(if opts.multiline.unwrap_or(false) {
            DEFAULT_MAX_SEARCH_FILE_BYTES
        } else {
            DEFAULT_MAX_LINE_SEARCH_FILE_BYTES
        })
}

/// Run a parallel ignore walk + per-file search for a concrete matcher type,
/// writing into `state`. The walk stops at the next entry once `cancelled()`
/// returns true or `state.stop` is raised, as it is once what it keeps
/// passes `held`.
pub(super) fn collect<M: Matcher + Sync>(
    opts: &TextSearchOptions,
    (matcher, terms): (&M, &[M]),
    mode: Mode,
    path_filter: Arc<dyn TextSearchPathFilter>,
    (state, held): (&CollectState, &Arc<Held>),
    control: WalkControl<'_>,
) -> Result<()> {
    let (walk_builder, scope) = walk_scope(opts, mode, state, control)?;
    let scope = &scope;
    let worker = || {
        let path_filter = Arc::clone(&path_filter);
        let mut worker_recs = WorkerRecs {
            local: Vec::new(),
            state,
        };
        let mut files = file_searcher(scope, mode, (matcher, terms), (&state.stop, held));
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
    scope.record_unreadable_ignore_files();
    Ok(())
}

/// The walker a search of `opts` runs and what every worker reads while it
/// visits, with walk-wide records in `state`.
pub(super) fn walk_scope<'a>(
    opts: &'a TextSearchOptions,
    mode: Mode,
    state: &'a CollectState,
    control: WalkControl<'a>,
) -> Result<(ignore::WalkBuilder, WalkScope<'a>)> {
    // Traversal ordering is only stable if a single worker drains the walk in a
    // fixed order; the default parallel walk merges worker buffers arbitrarily.
    let (mut walk_builder, rule) = build_walk_builder(opts)?;
    // Otherwise the walk takes this call's share of the process CPU budget
    // (`walk_builder`, `cpu`).
    if preserves_traversal_order(opts) {
        walk_builder.threads(1);
    }
    let scope = WalkScope {
        opts,
        rule,
        state,
        control,
        max_file_bytes: byte_ceiling(opts),
        keep_unmatched: mode == Mode::FilesWithoutMatch,
        identifier: identifier_search(opts),
        file_terms: FileTerms::new(opts),
        attributes: ranks_by_relevance(opts)
            .then(|| relevance::GitAttributes::new(Path::new(&opts.path)))
            .flatten(),
        words: ranks_by_relevance(opts)
            .then(|| alternation_words(opts))
            .flatten(),
        judge_gate: super::judge_gate::JudgeGate::of(
            opts,
            &super::rust_spelling(opts, &opts.pattern),
        ),
    };
    Ok((walk_builder, scope))
}

/// One worker's searchers for the files `scope` admits, stopped by `stop`,
/// which they raise once what they keep passes `held`.
pub(super) fn file_searcher<'a, M: Matcher>(
    scope: &'a WalkScope<'_>,
    mode: Mode,
    (matcher, terms): (&'a M, &'a [M]),
    (stop, held): (&'a std::sync::atomic::AtomicBool, &'a Arc<Held>),
) -> FileSearcher<'a, M> {
    let opts = scope.opts;
    let only_matching = only_matching(opts);
    // only-matching emits bare spans; only-matching ignores context too.
    let context = if mode == Mode::Normal && !only_matching {
        opts.context
    } else {
        SearchContext::default()
    };
    FileSearcher {
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
            row_window: opts.row_window,
            ..match_work(mode, only_matching)
        },
        stop,
        held,
        keep_head: ranks_by_relevance(opts),
        copy: scope.file_terms.is_some(),
        words: scope.words.as_ref(),
        terms,
        gate: &scope.judge_gate,
    }
}
