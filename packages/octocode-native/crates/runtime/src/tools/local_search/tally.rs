//! localSearch `unique: "countAll"`: each matched value once over the whole
//! scan, with its occurrences and the files holding it, most frequent first.
//! The scan counts values per file as `unique: "count"` does; every file's
//! values are then checked against their source and redacted as a shown
//! row's are ([`verify_candidate`]), and only those values are tallied: a
//! redacted value counts as its placeholder, an unverified one as the
//! unverified placeholder (the row then says so). The tally is the page:
//! no per-file rows and no file pages; a long tally is split by response
//! pagination, never cut.
use super::executor::Shaped;
use super::layout::Layout;
use super::local_search_query;
use super::page_view::PageView;
use super::verify::{Outlines, PageScope, Redacted, Redaction, Withheld, verify_candidate};
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{SearchContext, TextSearchFile, TextSearchResult};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// One tallied value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UniqueValue {
    pub value: String,
    /// Its occurrences over the scan.
    pub count: u32,
    /// The files holding it.
    pub files: u32,
}

/// A scan's tally and what checking its values found.
pub(super) struct Tally {
    pub(super) values: Vec<UniqueValue>,
    /// Tallied values redaction changed (`redactedMatches`).
    pub(super) redacted: usize,
}

/// Whether `query` asks for the tally over every file.
///
/// Waits on core: `unique: "countAll"` is not a contract value yet, so a
/// query cannot ask for it and this is `false` outside tests. Once core
/// declares it, this reads `unique(query) == Unique::CountAll` and the scan
/// option maps it to `SpanValues::Counted`, as `count`.
pub(super) fn counts_all(query: &LocalSearchQuery) -> bool {
    local_search_query::unique(query) == local_search_query::Unique::Count && forced()
}

#[cfg(not(test))]
fn forced() -> bool {
    false
}

#[cfg(test)]
thread_local! {
    static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn forced() -> bool {
    FORCED.with(std::cell::Cell::get)
}

/// Until the guard drops, a `unique: "count"` query on this thread runs as
/// `countAll` would (the option core has not declared yet).
#[cfg(test)]
pub(super) fn test_count_all() -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCED.with(|forced| forced.set(false));
        }
    }
    FORCED.with(|forced| forced.set(true));
    Reset
}

/// The tally page of a fresh scan whose values `redact_scan` redacted: every
/// file's values are checked against their source, then tallied. Nothing is
/// stored for later pages: the one page is the whole answer.
pub(super) fn shape(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut TextSearchResult,
    (mut redacted, withheld): (Redacted, Withheld),
    redaction: Redaction,
) -> Result<Shaped, ToolError> {
    if local_search_query::has_classification(scope.query) {
        return Err(ToolError::new(
            ErrorCode::InvalidInput,
            "unique countAll tallies every hit; classification selects hits per file: use unique count with classification for the kept hits' values.",
        ));
    }
    let mut unverified = false;
    for file in &mut parsed.files {
        unverified |= !verify_candidate(scope, file, (&mut redacted, &withheld))?;
    }
    let tally = tally(&parsed.files, &redacted);
    let layout = layout(scope.query, parsed.files.len());
    let whole = TextSearchResult {
        files: std::mem::take(&mut parsed.files),
        stats: parsed.stats.clone(),
    };
    let (view, _) = PageView::dense(whole, layout);
    Ok(Shaped {
        packed: None,
        view,
        redacted,
        redaction,
        unverified,
        outlines: Outlines::new(),
        selection: None,
        tally: Some(tally),
        notes: Vec::new(),
    })
}

/// Each value of `files` once: its occurrences (a per-file count row adds
/// its `count`) and the files holding it, most frequent first, then in
/// more files, then by value. Values that redaction made equal tally as one.
fn tally(files: &[TextSearchFile], redacted: &Redacted) -> Tally {
    let mut at: HashMap<&str, usize> = HashMap::new();
    let mut values: Vec<UniqueValue> = Vec::new();
    let mut changed: HashSet<usize> = HashSet::new();
    for file in files {
        let mut seen: HashSet<usize> = HashSet::new();
        for hit in &file.matches {
            let index = *at.entry(hit.value.as_str()).or_insert_with(|| {
                values.push(UniqueValue {
                    value: hit.value.clone(),
                    count: 0,
                    files: 0,
                });
                values.len() - 1
            });
            let value = &mut values[index];
            value.count = value.count.saturating_add(hit.count.unwrap_or(1));
            if seen.insert(index) {
                value.files = value.files.saturating_add(1);
            }
            if redacted.contains(&(file.path.clone(), hit.line, hit.column)) {
                changed.insert(index);
            }
        }
    }
    values.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(b.files.cmp(&a.files))
            .then_with(|| a.value.cmp(&b.value))
    });
    Tally {
        values,
        redacted: changed.len(),
    }
}

/// The one page of a tally over `files` matched files: no rows shown, no
/// further page.
fn layout(query: &LocalSearchQuery, files: usize) -> Layout {
    Layout {
        list: false,
        shown: Vec::new(),
        total_pages: 1,
        total_items: saturating_u32(files),
        offset: 0,
        length: 0,
        matches_per: 1,
        row_unit: "matches",
        out_of_range: files > 0 && local_search_query::page(query).max(1) > 1,
        context: SearchContext::default(),
        streamed: true,
        merge_context: None,
        pages: None,
    }
}
