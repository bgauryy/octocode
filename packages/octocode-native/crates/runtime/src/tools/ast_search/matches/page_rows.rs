//! The rows of a file whose matches span match pages, kept so its later match
//! pages cut from them instead of parsing it again. They are a pure function of
//! what the key names: the snapshot digests the rule, the scope and every
//! scanned byte, and the scan key the row shape. They are served only to a page
//! whose stored scan just proved every source unchanged, and live in memory
//! only, unredacted as a stored scan's matches are: each page copies its rows
//! out and passes them through the same key-block check and output gate.
use super::rows::MatchRow;
use crate::tools::page_memo::PageMemo;
use std::sync::Arc;

static ROWS: PageMemo<Arc<Vec<MatchRow>>> = PageMemo::new();

/// Where one page finds and leaves its files' rows.
#[derive(Clone, Copy)]
pub(super) struct PageRows<'a> {
    pub(super) snapshot: &'a str,
    pub(super) key: &'a str,
    /// The page's stored scan just passed its freshness proof. Only such a page
    /// reads rows; a fresh scan's page rebuilds from bytes it hashes.
    pub(super) proved: bool,
}

impl PageRows<'_> {
    pub(super) fn id(&self, path: &str) -> String {
        format!("{}\0{path}", self.snapshot)
    }

    /// The rows kept for the file with row path `path`.
    pub(super) fn get(&self, path: &str) -> Option<Arc<Vec<MatchRow>>> {
        if !self.proved {
            return None;
        }
        ROWS.get(&self.id(path), self.key, Arc::clone)
    }

    /// Keep `rows`, rebuilt for a page of the file with row path `path`.
    pub(super) fn keep(&self, path: &str, rows: Vec<MatchRow>) {
        let bytes = weight(&rows);
        ROWS.put(self.id(path), self.key.to_owned(), bytes, Arc::new(rows));
    }
}

/// About how many bytes `rows` hold: each row's text, its JSON's structure, and
/// once the descriptors a file's rows share.
fn weight(rows: &[MatchRow]) -> usize {
    let descriptors = rows
        .iter()
        .find_map(|row| row.source_descriptors.as_ref())
        .map_or(0, |descriptors| descriptors.owned_bytes());
    let text = rows
        .iter()
        .map(|row| 256 + super::row_chars(row))
        .sum::<usize>();
    text + descriptors
}

/// Every file a page parsed again, for the tests that count parses.
#[cfg(test)]
static REBUILT: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) fn rebuilt(path: &std::path::Path) {
    REBUILT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path.to_owned());
}

/// How many times a page parsed `path` again.
#[cfg(test)]
pub(super) fn rebuilds(path: &std::path::Path) -> usize {
    REBUILT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|rebuilt| rebuilt.as_path() == path)
        .count()
}
