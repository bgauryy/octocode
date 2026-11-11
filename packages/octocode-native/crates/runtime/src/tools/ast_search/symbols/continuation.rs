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
use crate::policy::{path::PathPolicy, prune::DefaultsFlag};
use crate::tools::ast_search::memo::ScanMemo;
use octocode_contract_types::AstSearchQuerySymbols;
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
    crate::digest::json_sha256(&json!([
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
        crate::security::SourceLimits::REAL.whole_source,
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
            set.snapshot = crate::digest::json_sha256(&json!([set.snapshot, before]));
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

    /// Keep `set` for the pages after the one being served, when `q` lets a
    /// result be kept: a filtering scope's walk asks its verdicts again on
    /// every page anyway, for the key.
    pub(super) fn remember(&self, q: &AstSearchQuerySymbols, set: &Arc<SymbolSet>) {
        if crate::tools::test_code::Keep::of(q.test_code).is_some() {
            return;
        }
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
            weight(set),
        );
    }
}

/// About the heap `set` holds, what the one process budget weighs a kept result
/// by: every row, and what stands beside the rows, measured.
fn weight(set: &SymbolSet) -> usize {
    let beside_rows = octocode_engine::json_size::bytes_or(
        &(
            &set.references,
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
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::tools::ast_search::symbols::{outline::outline_file, select::select};
    use crate::tools::test_support::{Fixture, simple_policy};

    /// The weight tracks the heap the rows hold, at least what they serialize to and
    /// within what a row table costs, as the store's byte budget expects, with and
    /// without the provenance fields.
    #[test]
    fn a_stored_set_weighs_about_its_rows() {
        let root = Fixture::new();
        let source = (0..200).fold(String::new(), |source, index| {
            source
                + &format!("export function declared{index}(value: number) {{ return value; }}\n")
        });
        let file = root
            .write("a.ts", source)
            .canonicalize()
            .expect("canonical");
        let (paths, _) = simple_policy(&root.0);
        let row_paths = crate::tools::ast_search::RowPaths::new(&paths, &file, true);
        for provenance in [false, true] {
            let q: AstSearchQuerySymbols = serde_json::from_value(
                json!({"operation":"symbols","path":file,"includeProvenance":provenance}),
            )
            .expect("query");
            let outline = outline_file(&q, (&file, "a.ts"), None, &NeverCancel)
                .expect("outline")
                .unwrap_or_else(|row| panic!("terminal row {row}"));
            let set = select(&q, &file, outline, row_paths, &NeverCancel).expect("set");
            let rows = serde_json::to_vec(&set.declarations).expect("rows").len();
            let weight = weight(&set);
            assert!(
                rows <= weight && weight <= rows * 16,
                "{weight} bytes for {rows} of rows (provenance: {provenance})"
            );
        }
    }
}
