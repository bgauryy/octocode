//! The symbol set of an outline kept for the continuation pages of its
//! result. Only a continuation reads it: it carries the snapshot its first
//! page returned, so a fresh request never meets stored state, and a page
//! after the first pages the stored set instead of parsing again.
//!
//! A set is served only while the continuation's inputs match the ones it
//! was built from: the query fields and path policy that shape the outline
//! and the stamps of every file it read, plus for a directory the walk
//! (re-walked per page: names, ignore and prune facts, no reads). An edit, or
//! a file added, removed or ignored, leaves the set unserved and the page
//! outlines afresh, where a snapshot that no longer matches answers
//! `staleSnapshot`.
use super::ast_search_query_symbols;
use super::directory::DirectoryWalk;
use super::select::SymbolSet;
use crate::ast_search::memo::ScanMemo;
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_runtime_base::policy::{path::PathPolicy, prune::DefaultsFlag};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::SystemTime};

static SETS: ScanMemo<SymbolSet> = ScanMemo::new();

/// What an outline's stored set is keyed and watched by.
pub(super) struct Continuation {
    /// The outline inputs the snapshot does not digest, and the walk.
    key: String,
    /// Every file the set read or walked, stamped when the set is stored.
    sources: Vec<PathBuf>,
    /// Each source's stamp as the walk read it, before any read.
    stamps: Vec<Option<octocode_engine::stamp::SourceStamp>>,
    root: PathBuf,
    /// Before the read: no file written after it is trusted as read.
    started: SystemTime,
}

/// The digest of the outline inputs the snapshot does not digest, with what
/// the outline's walk found.
fn key_of(
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    root: &std::path::Path,
    walked: &Value,
) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        paths.identity(),
        root.to_string_lossy(),
        q.parser,
        ast_search_query_symbols::language_globs(q),
        q.symbol_filter,
        ast_search_query_symbols::kinds(q),
        ast_search_query_symbols::exclude(q),
        ast_search_query_symbols::include(q),
        ast_search_query_symbols::scan_offset(q),
        q.test_code,
        q.default_excludes.defaults(),
        q.include_ignored,
        q.include_hidden,
        q.include_provenance,
        octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        walked,
    ]))
}

impl Continuation {
    /// The continuation of a directory outline over `walk`.
    pub(super) fn of(
        q: &AstSearchQuerySymbols,
        paths: &PathPolicy,
        root: &std::path::Path,
        walk: &DirectoryWalk,
        started: SystemTime,
    ) -> Self {
        let listing = &walk.files;
        let walked = json!([
            listing
                .entries
                .iter()
                .map(|entry| &entry.relative_path)
                .collect::<Vec<_>>(),
            &listing.ignored,
            &listing.pruned_dirs,
            listing.was_capped,
            walk.walk_errors,
        ]);
        Self {
            key: key_of(q, paths, root, &walked),
            sources: listing
                .entries
                .iter()
                .map(|entry| root.join(&entry.relative_path))
                .collect(),
            stamps: listing.entries.iter().map(|entry| entry.stamp).collect(),
            root: root.to_path_buf(),
            started,
        }
    }

    /// The continuation of the outline of one file, watched by its own stamp.
    pub(super) fn of_file(
        q: &AstSearchQuerySymbols,
        paths: &PathPolicy,
        file: &std::path::Path,
        started: SystemTime,
    ) -> Self {
        Self {
            key: key_of(q, paths, file, &Value::Null),
            sources: vec![file.to_path_buf()],
            stamps: Vec::new(),
            root: file.to_path_buf(),
            started,
        }
    }

    /// What the first `count` files of the walk are now
    /// ([`octocode_engine::digest::ListedFiles`]).
    fn listed(&self, count: usize) -> String {
        let mut listed = octocode_engine::digest::ListedFiles::default();
        for (source, stamp) in self.sources.iter().zip(&self.stamps).take(count) {
            listed.add(source, *stamp);
        }
        listed.hex()
    }

    /// What the files before `q`'s window are now; `None` for a first window.
    pub(super) fn listed_before(&self, q: &AstSearchQuerySymbols) -> Option<String> {
        let offset = ast_search_query_symbols::scan_offset(q) as usize;
        (offset > 0).then(|| self.listed(offset))
    }

    /// Bind `set` to the windows around it: its pages answer for the files
    /// `before` its window, and its resume pins the files through its cut.
    pub(super) fn bind(&self, set: &mut SymbolSet, before: Option<&str>) {
        if let Some(before) = before {
            set.snapshot =
                octocode_runtime_base::digest::json_sha256(&json!([set.snapshot, before]));
        }
        set.scan_pin = set.scan_resume.map(|resume| self.listed(resume as usize));
    }

    /// The set the first page stored for the snapshot this continuation
    /// page carries, while nothing it read changed.
    pub(super) fn stored(&self, q: &AstSearchQuerySymbols) -> Option<Arc<SymbolSet>> {
        if ast_search_query_symbols::page(q) <= 1 {
            return None;
        }
        SETS.get(ast_search_query_symbols::snapshot(q)?, &self.key)
    }

    /// The set a first page of the same outline kept, while nothing it read
    /// changed: a repeat of the request, or a row of the same batch, reads it
    /// instead of outlining again ([`Self::first_or_outline`]).
    fn first(&self) -> Option<Arc<SymbolSet>> {
        SETS.get(&self.first_key(), &self.key)
    }

    /// The store key of a first page's set: the outline's inputs, with the
    /// walk, stand in for the snapshot no first page carries yet.
    fn first_key(&self) -> String {
        format!("first:{}", self.key)
    }

    /// A first page's set: the one kept for these inputs, else `outline`'s,
    /// kept for the requests after it. Concurrent requests for one outline
    /// (the rows of one batch) outline it once ([`ScanMemo::turn`]); a
    /// request stopped while it waits outlines for itself.
    pub(super) fn first_or_outline<E>(
        &self,
        q: &AstSearchQuerySymbols,
        stop: &dyn Fn() -> bool,
        outline: impl FnOnce() -> Result<Arc<SymbolSet>, E>,
    ) -> Result<Arc<SymbolSet>, E> {
        use octocode_tools_shared::page_memo::Turn;
        let key = self.first_key();
        loop {
            if let Some(set) = self.first() {
                return Ok(set);
            }
            let leading = match SETS.turn(&key, stop) {
                Turn::Landed => continue,
                Turn::Lead(leading) => Some(leading),
                Turn::Stopped => None,
            };
            let set = outline()?;
            self.keep(q, key, &set);
            drop(leading);
            return Ok(set);
        }
    }

    /// What proves a page of this outline current to a request that asked
    /// for the same page while it was built ([`crate::ast_search::shared_rows`]):
    /// the root, every file the walk listed, every directory it admitted
    /// (`dirs`) with their ignore files, the ignore rules outside the walk
    /// and the repository markers above it, as a kept scan is watched.
    /// `None` when one of them was written within the settle window before
    /// the outline started, or is gone, or an ignore file cannot be watched.
    pub(super) fn proof(
        &self,
        paths: &PathPolicy,
        dirs: &[PathBuf],
    ) -> Option<octocode_engine::stamp::Proof> {
        use octocode_tools_shared::page_memo::{
            Watch, directory_ignore_inputs_with, repository_markers,
        };
        let global = paths.global_ignore();
        let discovery = paths.discovery_walk();
        let mut sources = self.sources.clone();
        sources.extend(dirs.iter().cloned());
        let mut rules =
            octocode_engine::search::walk::external_ignore_inputs_with(&self.root, global);
        for dir in std::iter::once(&self.root).chain(dirs) {
            let inputs = directory_ignore_inputs_with(dir, &self.root, global, |ignore| {
                discovery.permits_entry(ignore, None)
            });
            if inputs.untracked {
                return None;
            }
            sources.extend(inputs.files);
            rules.extend(inputs.external);
        }
        Watch {
            root: &self.root,
            sources: &sources,
            rules: &rules,
            markers: &repository_markers(&self.root),
        }
        .capture(octocode_engine::stamp::Link::Follow, Some(self.started))
    }

    /// Keep `set` for the pages after the one being served, when `q` lets a
    /// result be kept: a filtering scope's walk asks its verdicts again on
    /// every page anyway, for the key.
    pub(super) fn remember(&self, q: &AstSearchQuerySymbols, set: &Arc<SymbolSet>) {
        self.keep(q, set.snapshot.clone(), set);
    }

    /// Keep `set` under `snapshot`, watched by every file it read.
    fn keep(&self, q: &AstSearchQuerySymbols, snapshot: String, set: &Arc<SymbolSet>) {
        if octocode_tools_shared::test_code::Keep::of(q.test_code).is_some() {
            return;
        }
        SETS.put(
            (snapshot, self.key.clone()),
            octocode_tools_shared::page_memo::Watch {
                root: &self.root,
                sources: &self.sources,
                rules: &[],
                markers: &[],
            },
            self.started,
            Arc::clone(set),
            weight(set),
        );
    }
}

/// About the heap `set` holds, what the one process budget weighs a kept result
/// by: every row, and what stands beside the rows, measured.
fn weight(set: &SymbolSet) -> usize {
    let beside_rows = octocode_engine::json_size::bytes_or(
        &(
            &set.text_search,
            &set.source_descriptors,
            &set.diagnostics,
            &set.skipped_paths,
            &set.suppressed,
            &set.kinds_left_out,
        ),
        usize::MAX,
    );
    set.declarations
        .iter()
        .map(|row| std::mem::size_of::<Value>() + super::rows::heap_bytes(row))
        .sum::<usize>()
        .saturating_add(set.sizes.len() * std::mem::size_of::<usize>())
        .saturating_add(set.name_columns.len() * std::mem::size_of::<u32>())
        .saturating_add(beside_rows)
}

#[cfg(test)]
mod tests;
