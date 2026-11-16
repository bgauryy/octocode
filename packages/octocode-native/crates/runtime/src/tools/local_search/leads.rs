//! The follow-up calls a search page offers (read, references, re-runs,
//! listings), ranked; what the page could not cover is in `coverage`, the
//! empty page's probes in `probes`.

use super::layout::*;
use super::local_search_query;
use super::types::*;
use super::{binaries::*, handoff::*, probes::*};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::{Continuation, ToolError};
use crate::tools::empty_walk::Cause;
use crate::tools::leads::skipped::under;
use octocode_contract_types::{LocalSearchQuery, SkippedPathCause, SkippedPaths};
use octocode_engine::types::TextSearchOptions;
use serde_json::{Value, json};

mod reads;
mod skipped;

/// The `next` name of the listing of binaries a walk skipped.
pub(super) const BINARY_LISTING: &str = "binarySkipped";

/// What a page's leads disclosed of the paths its walk skipped.
pub(super) struct Disclosed {
    /// Skipped paths hold a match, or may: a retry over them reaches it.
    pub(super) skipped_match: bool,
    pub(super) skipped_paths: Option<SkippedPaths>,
    /// The check of an empty page's skipped paths found nothing in them.
    pub(super) checked: Option<Checked>,
}

/// A page's result, for its leads.
pub(super) struct Found<'a> {
    pub(super) query: &'a LocalSearchQuery,
    pub(super) paths: &'a PathPolicy,
    pub(super) root: &'a std::path::Path,
    pub(super) output_root: &'a std::path::Path,
    pub(super) files: &'a [SearchFile],
    pub(super) layout: &'a Layout,
    pub(super) definition: Option<&'a Definition>,
    pub(super) symbol: Option<&'a str>,
    pub(super) scanned: &'a octocode_engine::types::TextSearchStats,
    /// What the walk left out: policy-withheld, default-excluded and hidden
    /// entries.
    pub(super) skipped: &'a crate::policy::discovery::WalkSkips,
    /// The walk reached every path. A binary cut or an unreadable file
    /// leaves file content unread, not paths unwalked, so it does not
    /// clear this.
    pub(super) walk_complete: bool,
    pub(super) empty: bool,
    /// What an empty scan's walk searched, when its record can serve.
    pub(super) searched: Option<&'a [std::path::PathBuf]>,
    /// The scanner's limits, which say what a read of a source can serve.
    pub(super) limits: crate::security::SourceLimits,
}

impl Found<'_> {
    /// Leads in the tool's own rank: a read of the top hit, the symbol's
    /// lspSearch lead, then the skipped binaries
    /// and the literal search. Returns what it disclosed of the skipped
    /// paths.
    pub(super) fn add_leads(
        &self,
        stats: &mut SearchStats,
        next: &mut Option<Value>,
        hints: &mut Vec<String>,
        warnings: &mut Vec<String>,
        search_options: &TextSearchOptions,
        cancel: &impl CancellationCheck,
    ) -> Result<Disclosed, ToolError> {
        let query = self.query;
        let first_disclosure = self.first_page();
        let mut add = |name: String, lead: Value| {
            if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
                map.insert(name, lead);
            }
        };
        if let Some(read) = self.read_lead() {
            add("read".into(), read);
        }
        warnings.extend(self.windowed_hits());
        if let (Some(symbol), Some(definition)) = (self.symbol, self.definition)
            && let Some(row) = crate::tools::leads::lsp::verify_query(
                &definition.source,
                symbol,
                u64::from(definition.line),
                crate::tools::leads::lsp::Verify::for_kind(&definition.kind),
                local_search_query::test_code(query),
            )
        {
            add(
                crate::tools::leads::lsp::lead_name(&row),
                Continuation::related(ToolId::LspSearch, row).build(),
            );
        }
        // Binaries were never text to search: every row states their count
        // in `stats` (an empty row's absence sentence restates it). Only
        // binary-cut files too many to name get a listing: their text past
        // the NUL is unread.
        if let Some(disclosure) = self.skipped_binaries() {
            stats.binary_skipped = self.leading_binaries();
            if let Some(listing) = disclosure.listing {
                add(BINARY_LISTING.into(), listing);
            }
        }
        // Directories the depth limit left unentered are coverage the walk
        // never had: disclosed once, with the same search at every depth.
        if first_disclosure
            && let Some(dirs) = self.scanned.depth_cut_dirs.filter(|count| *count > 0)
        {
            warnings.push(format!(
                "{dirs} dir(s) at maxDepth were not entered; files below them were not searched."
            ));
            add("expandDepth".into(), unlimited_depth_lead(query)?);
        }
        // What the walk left out is disclosed once, on the first page, in
        // `skippedPaths` (the security policy's withheld entries, which
        // nothing lifts, among them). The search over the paths a flag lifts
        // is offered only where it applies: an empty page that proves which
        // of them hold a match, in `disclose_unsearched`. A page with hits
        // already answers; its skipped paths stay counted, not re-offered.
        // Thousands of hits page for dozens of calls: one count call ranks
        // the files first, so the next read or narrowing is chosen, not paged.
        // A page that shows no hit rows (a `countAll` tally) pages nothing.
        if self.first_page()
            && !list_view(local_search_query::view(query))
            && !self.files.is_empty()
            && self
                .scanned
                .match_count
                .is_some_and(|total| total >= LARGE_RESULT_MATCHES)
        {
            add("viewFileCount".into(), count_lead(query)?);
        }
        // Files whose match the `fileHas`/`fileLacks` terms removed: the
        // empty page is the filter's, not the pattern's.
        if self.empty
            && let Some(dropped) = self.scanned.file_terms_dropped.filter(|count| *count > 0)
        {
            warnings.push(format!(
                "{dropped} file(s) matched matchString but fileHas/fileLacks removed them."
            ));
            add("broadenSearch".into(), unfiltered_terms_lead(query)?);
        }
        let unsearched = self.empty
            && self.walk_complete
            && first_disclosure
            && !self.root.is_file()
            && !names_nothing(self.skipped);
        let tagged = unsearched
            .then(|| probe_unsearched(query, self.paths, search_options, self.searched, cancel));
        let unsearched = match tagged {
            Some(tagged) => disclose_unsearched(
                query,
                search_options,
                tagged,
                Skipped {
                    walk: self.skipped,
                    causes: self.skipped_causes(),
                },
                next,
                hints,
            )?,
            None => Unsearched::default(),
        };
        let skipped_paths = if first_disclosure {
            self.skipped_paths(unsearched.observed)?
        } else {
            None
        };
        Ok(Disclosed {
            skipped_match: unsearched.may_hold_matches,
            skipped_paths,
            checked: unsearched.checked,
        })
    }

    pub(super) fn first_page(&self) -> bool {
        local_search_query::page(self.query).max(1) == 1
    }
}

/// The same search one folder up from `root` (the searched file or
/// directory), from page 1, when `root` lies below the top of its tree
/// ([`PathPolicy::tree_root`]: the workspace, a clone's checkout): the scope
/// widens by one level, never to the whole tree at once. `None` at the top
/// itself or outside every tree.
pub(super) fn parent_folder_lead(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    root: &std::path::Path,
) -> Result<Option<Value>, ToolError> {
    let parent = root.parent().filter(|parent| {
        paths
            .tree_root(root)
            .is_some_and(|top| top.as_path() != root && parent.starts_with(&top))
    });
    match parent {
        Some(parent) => restart_lead(query, json!({"path": paths.shown(parent)})).map(Some),
        None => Ok(None),
    }
}

/// The same search without its `maxDepth`, from page 1.
fn unlimited_depth_lead(query: &LocalSearchQuery) -> Result<Value, ToolError> {
    restart_lead(query, json!({"maxDepth": null}))
}

/// The same search without its `fileHas`/`fileLacks` terms, from page 1.
fn unfiltered_terms_lead(query: &LocalSearchQuery) -> Result<Value, ToolError> {
    restart_lead(query, json!({"fileHas": null, "fileLacks": null}))
}

/// Matches at which a line view leads to its per-file counts.
pub(super) const LARGE_RESULT_MATCHES: u32 = 1_000;

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
            "classification": null,
        }),
    )
}

/// The query as a fresh page-1 search (none of its
/// [`crate::contracts::state_fields`]) with `changes`
/// applied, as the localSearch call that runs it.
pub(super) fn restart_lead(query: &LocalSearchQuery, changes: Value) -> Result<Value, ToolError> {
    let mut changes = changes;
    for field in crate::contracts::state_fields(crate::id::ToolId::LocalSearch) {
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
            .filter(|glob| keep_globs && !crate::policy::include::reaches_above(glob))
            .cloned()
            .collect::<Vec<_>>()
    };
    crate::tools::leads::files_listing::scope_listing(
        local_search_query::path(query),
        (
            matchable(local_search_query::include(query)),
            matchable(local_search_query::exclude(query)),
        ),
        local_search_query::include_ignored(query) == Some(true),
        !local_search_query::default_excludes(query).defaults(),
    )
}
