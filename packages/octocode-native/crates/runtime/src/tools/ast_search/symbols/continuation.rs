//! The symbol set of a directory outline kept for the continuation pages of
//! its result. Only a continuation reads it: it carries the snapshot its
//! first page returned, so a fresh request never meets stored state, and a
//! page after the first pages the stored set instead of parsing the scanned
//! files again.
//!
//! A set is served only while the continuation's inputs match the ones it
//! was built from: the query fields and path policy that shape the outline,
//! the walk of the directory (re-walked per page: names, ignore and prune
//! facts, no reads) and the stamps of every walked file. An edit, or a file
//! added, removed or ignored, leaves the set unserved and the page outlines
//! afresh, where a snapshot that no longer matches answers `staleSnapshot`.
use super::ast_search_query_symbols;
use super::directory::DirectoryWalk;
use super::select::SymbolSet;
use crate::policy::{path::PathPolicy, prune::DefaultsFlag};
use crate::tools::ast_search::memo::ScanMemo;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::SystemTime};

static SETS: ScanMemo<SymbolSet> = ScanMemo::new();

/// What a directory outline's stored set is keyed and watched by.
pub(super) struct Continuation {
    /// The outline inputs the snapshot does not digest, and the walk.
    key: String,
    /// Every walked file, stamped when the set is stored.
    sources: Vec<PathBuf>,
    root: PathBuf,
    /// Before the walk: no file written after it is trusted as read.
    started: SystemTime,
}

impl Continuation {
    pub(super) fn of(
        q: &AstSearchQuerySymbols,
        paths: &PathPolicy,
        root: &std::path::Path,
        walk: &DirectoryWalk,
        started: SystemTime,
    ) -> Self {
        let listing = &walk.files;
        let key = crate::digest::json_sha256(&json!([
            paths.identity(),
            root.to_string_lossy(),
            q.parser,
            ast_search_query_symbols::language_globs(q),
            q.symbol_filter,
            ast_search_query_symbols::kinds(q),
            ast_search_query_symbols::exclude(q),
            ast_search_query_symbols::scan_offset(q),
            q.test_code,
            q.default_excludes.defaults(),
            q.include_ignored,
            q.include_hidden,
            q.include_provenance,
            crate::tools::ast_search::source::MAX_PARSE_SOURCE_BYTES,
            listing
                .entries
                .iter()
                .map(|entry| &entry.relative_path)
                .collect::<Vec<_>>(),
            &listing.ignored,
            &listing.pruned_dirs,
            listing.was_capped,
            walk.walk_errors,
        ]));
        Self {
            key,
            sources: listing
                .entries
                .iter()
                .map(|entry| root.join(&entry.relative_path))
                .collect(),
            root: root.to_path_buf(),
            started,
        }
    }

    /// The set the first page stored for the snapshot this continuation
    /// page carries, while nothing it read changed.
    pub(super) fn stored(&self, q: &AstSearchQuerySymbols) -> Option<Arc<SymbolSet>> {
        if ast_search_query_symbols::page(q) <= 1 {
            return None;
        }
        SETS.get(ast_search_query_symbols::snapshot(q)?, &self.key)
    }

    /// Keep `set` for the pages after the one being served.
    pub(super) fn remember(&self, set: &Arc<SymbolSet>) {
        let bytes = serde_json::to_vec(set.as_ref()).map_or(usize::MAX, |bytes| bytes.len());
        SETS.put(
            (set.snapshot.clone(), self.key.clone()),
            crate::tools::page_memo::Watch {
                root: &self.root,
                sources: &self.sources,
                rules: &[],
                markers: &[],
            },
            self.started,
            Arc::clone(set),
            bytes,
        );
    }
}
