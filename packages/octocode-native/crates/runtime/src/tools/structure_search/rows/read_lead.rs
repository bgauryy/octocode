//! The read a listing's first page leads to.
use super::{EntryKind, Row};
use crate::response::result::Continuation;
use octocode_contract_types::{
    LocalFetchQuery, LocalFetchQueryContent0, LocalFetchQueryContent0View, LocalFetchQuerySymbols,
    LocalFetchQuerySymbolsView,
};
use serde_json::Value;

/// A related read on a listing's first page (`rows`): the best-ranked file
/// ([`read_rank`](crate::tools::root_read::read_rank), the ranking
/// ghStructure shares; any outline-able file ranks last), first among equal
/// ranks in listing order: of the small, hand-written files, else of the
/// files of at most [`READ_LEAD_FALLBACK_MAX_BYTES`]. Source and docs are
/// outlined (`view:"symbols"`).
pub(crate) fn read_lead(rows: &[Row], root: &std::path::Path) -> Option<Value> {
    let best = |preferred: bool| {
        rows.iter()
            .enumerate()
            .filter(|(_, row)| {
                row.kind == EntryKind::File
                    && if preferred {
                        row.size <= READ_LEAD_MAX_BYTES && !generated_or_fixture(&row.path)
                    } else {
                        row.size <= READ_LEAD_FALLBACK_MAX_BYTES
                    }
            })
            .filter_map(|(index, row)| {
                let depth = row.path.matches('/').count();
                let rank =
                    crate::tools::root_read::read_rank(&row.path, depth, &[]).or_else(|| {
                        crate::tools::outlines(&row.path)
                            .then_some(crate::tools::root_read::RANK_ANY)
                    })?;
                Some(((rank, depth, index), row))
            })
            .min_by_key(|(key, _)| *key)
            .map(|(_, row)| row)
    };
    let row = best(true).or_else(|| best(false))?;
    let path = if row.path.is_empty() || root.is_file() {
        root.to_path_buf()
    } else {
        root.join(&row.path)
    };
    let path = path.to_string_lossy();
    let query: LocalFetchQuery = if crate::tools::outlines(&row.path) {
        LocalFetchQuerySymbols {
            limit: None,
            main_goal: None,
            path: path.as_ref().try_into().ok()?,
            read_progress: None,
            reasoning: None,
            snapshot: None,
            view: LocalFetchQuerySymbolsView::Symbols,
        }
        .into()
    } else {
        LocalFetchQueryContent0 {
            classification: None,
            limit: None,
            main_goal: None,
            path: path.as_ref().try_into().ok()?,
            read_progress: None,
            reasoning: None,
            selection: None,
            snapshot: None,
            view: LocalFetchQueryContent0View::Content,
        }
        .into()
    };
    Some(Continuation::related_query(query).build())
}

/// Largest file a listing's read lead points at by preference: a lead
/// over a multi-megabyte generated file would page for many calls.
pub(super) const READ_LEAD_MAX_BYTES: i64 = 64 * 1024;

/// Largest file a listing's read lead points at at all: past it, the
/// outline itself would page for many calls, so no lead is offered.
const READ_LEAD_FALLBACK_MAX_BYTES: i64 = 1024 * 1024;

/// A generated, minified, declaration, or lock file, or one under a test
/// or fixture directory: a poor first read of an unfamiliar tree.
pub(super) fn generated_or_fixture(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let generated = name.contains(".generated.")
        || name.contains("_generated")
        || name.contains(".min.")
        || name.ends_with(".d.ts")
        || name.ends_with(".lock");
    let fixture_dir = path.split('/').rev().skip(1).any(|dir| {
        matches!(
            dir,
            "test" | "tests" | "fixtures" | "__fixtures__" | "__snapshots__"
        )
    });
    generated || fixture_dir
}
