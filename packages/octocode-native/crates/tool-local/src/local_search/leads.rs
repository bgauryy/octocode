//! What a search page says beside its rows: the skipped-path ledger and
//! window reads and the calls an empty or refused page offers; what the page could not cover
//! is in `coverage`, the empty page's recovery in `probes`.

use super::local_search_query;
use super::probes::*;
use super::types::*;
use octocode_contract_types::{LocalSearchQuery, SkippedPaths};
use octocode_runtime_base::policy::prune::DefaultsFlag;
use octocode_runtime_base::response::result::{Continuation, ToolError};
use octocode_tools_shared::empty_walk::Cause;
use serde_json::{Value, json};

mod reads;
mod skipped;
pub(super) use reads::add_window_reads;

/// What a page's leads disclosed of the paths its walk skipped.
pub(super) struct Disclosed {
    /// Skipped paths hold a match, or may: a retry over them reaches it.
    pub(super) skipped_match: bool,
    pub(super) skipped_paths: Option<SkippedPaths>,
}

/// A page's result, for what it says beside its rows.
pub(super) struct Found<'a> {
    pub(super) query: &'a LocalSearchQuery,
    pub(super) root: &'a std::path::Path,
    pub(super) scanned: &'a octocode_engine::types::TextSearchStats,
    /// What the walk left out: policy-withheld, default-excluded and hidden
    /// entries.
    pub(super) skipped: &'a octocode_runtime_base::policy::discovery::WalkSkips,
    /// The walk reached every path. A binary cut or an unreadable file
    /// leaves file content unread, not paths unwalked, so it does not
    /// clear this.
    pub(super) walk_complete: bool,
    pub(super) empty: bool,
}

impl Found<'_> {
    /// What a page says beside its rows. A page with hits states only what
    /// limits trust in them (`warnings`, `skippedPaths`): what to read or
    /// ask next is written from its rows' own paths and lines. An empty
    /// page offers `recovery` the causes it knows, in order. Returns what
    /// it disclosed of the skipped paths.
    pub(super) fn add_leads(
        &self,
        stats: &mut SearchStats,
        recovery: &mut Recovery,
        warnings: &mut Vec<String>,
    ) -> Result<Disclosed, ToolError> {
        let query = self.query;
        let first_disclosure = self.first_page();
        // Binaries were never text to search. Their count qualifies an
        // answer that is empty or partial; beside hits it changes nothing.
        stats.binary_skipped = self.leading_binaries();
        // Directories the depth limit left unentered are coverage the walk
        // never had: disclosed once.
        if first_disclosure
            && let Some(dirs) = self.scanned.depth_cut_dirs.filter(|count| *count > 0)
        {
            warnings.push(format!(
                "{dirs} dir(s) at maxDepth were not entered; files below them were not searched."
            ));
        }
        // Files whose match the `fileHas`/`fileLacks` terms removed: the
        // empty page is the filter's, not the pattern's.
        if self.empty
            && let Some(dropped) = self.scanned.file_terms_dropped.filter(|count| *count > 0)
        {
            recovery.offer(
                format!(
                    "{dropped} file(s) matched matchString but fileHas/fileLacks removed them; the related[] call searches without those terms."
                ),
                Some(("broadenSearch", unfiltered_terms_lead(query)?)),
            );
        }
        // What the walk left out is disclosed once, on the first page, in
        // `skippedPaths` (the security policy's withheld entries, which
        // nothing lifts, among them).
        let skipped_paths = if first_disclosure {
            self.skipped_paths()?
        } else {
            None
        };
        let unsearched = if self.discloses_skipped() {
            skipped_recovery(query, self.skipped, &self.skipped_causes())
        } else {
            None
        };
        let skipped_match = unsearched.is_some();
        if let Some((hint, call)) = unsearched {
            recovery.offer(hint, Some(call));
        }
        Ok(Disclosed {
            skipped_match,
            skipped_paths,
        })
    }

    pub(super) fn first_page(&self) -> bool {
        local_search_query::page(self.query).max(1) == 1
    }
}

/// The same search without its `fileHas`/`fileLacks` terms, from page 1.
fn unfiltered_terms_lead(query: &LocalSearchQuery) -> Result<Value, ToolError> {
    restart_lead(query, json!({"fileHas": null, "fileLacks": null}))
}

/// The same search as per-file match counts, from page 1.
pub(super) fn count_lead(query: &LocalSearchQuery) -> Result<Value, ToolError> {
    // Fields that shape match rows (or are valid only under matchOnly)
    // have no meaning in a count view.
    restart_lead(
        query,
        json!({
            "view": "countMatches",
            "contextLines": null,
            "unique": null,
        }),
    )
}

/// The query as a fresh page-1 search (none of its
/// [`octocode_runtime_base::contracts::state_fields`]) with `changes`
/// applied, as the localSearch call that runs it.
pub(super) fn restart_lead(query: &LocalSearchQuery, changes: Value) -> Result<Value, ToolError> {
    let mut changes = changes;
    for field in octocode_runtime_base::contracts::state_fields(
        octocode_runtime_base::id::ToolId::LocalSearch,
    ) {
        changes[field.as_str()] = Value::Null;
    }
    let restarted = local_search_query::edited(query, &changes)?;
    Ok(Continuation::related_query(restarted).build())
}

/// A structureSearch `files` listing of the searched scope in the query's
/// walk scope (`includeIgnored`, `defaultExcludes`; a `files` listing always
/// shows dot entries). With `keep_globs`, the query's own include/exclude
/// globs: what they match there (a glob that reaches above `path` is left
/// out: it never matches, and the row's warning names it). Without, when
/// those globs matched no file, the listing shows the names that exist.
pub(super) fn scope_listing(query: &LocalSearchQuery, keep_globs: bool) -> Option<Value> {
    let matchable = |globs: &[String]| {
        globs
            .iter()
            .filter(|glob| {
                keep_globs && !octocode_runtime_base::policy::include::reaches_above(glob)
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    octocode_tools_shared::leads::files_listing::scope_listing(
        local_search_query::path(query),
        (
            matchable(local_search_query::include(query)),
            matchable(local_search_query::exclude(query)),
        ),
        local_search_query::include_ignored(query) == Some(true),
        !local_search_query::default_excludes(query).defaults(),
    )
}
