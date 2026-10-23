//! The one listing pipeline `tree` and `files` run: walk (or reuse the walk
//! a first page stored), bind the page cut to a snapshot, cut and render the
//! page, and probe an empty listing. An operation contributes only its
//! query: the walk it asks the engine for ([`Listing`]).
use super::cut::page_ranges;
use super::probe::{FilterProbe, broaden_empty, lead_to_skipped, probe_listing};
use super::walked::Walked;
use super::{
    StructureResult, Walking, listing_prefix, memo, restart_if_stale, rows, validated_root, walked,
};
use crate::response::result::ToolError;
use crate::tools::display_name;
use crate::tools::empty_walk::Walk;
use serde_json::{Value, json};

use crate::contracts::listing_roles::structure_search::FIELDS as LISTING_FIELDS;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_contract_types::StructureSearchQueryFilesSort as Sort;
use octocode_engine::types::FileSystemQueryOptions;

mod walk;

#[cfg(test)]
pub(super) use walk::listing_walks;
use walk::walk_listing;

/// A listing operation's query: the fields both operations share
/// ([`listing_fields!`]) and what the operation decides itself.
pub(super) trait Listing: Clone + serde::Serialize + serde::de::DeserializeOwned {
    /// Page layout tag folded into the snapshot.
    const LAYOUT: &'static str;

    fn path(&self) -> &str;
    /// Which paths the request's own walk takes.
    fn walk(&self) -> Walk;
    /// This query with `relaxed` lifting skips, walking from its first
    /// entry: the resume cursors of the request's own walk do not apply.
    fn relaxed(&self, relaxed: Walk) -> Self;
    /// The caller's page size; `None` pages by the response budget.
    fn page_size(&self) -> Option<usize>;
    fn page(&self) -> usize;
    fn snapshot(&self) -> Option<&str>;
    /// Listing rows (walk or sort order) earlier windows listed.
    fn scan_offset(&self) -> usize;
    /// Walk entries (walk order) an earlier, time-cut walk inspected.
    fn walk_offset(&self) -> u32;
    /// Rows the walk lists: `maxEntries`, at most the walk ceiling.
    fn requested(&self) -> usize;
    /// Line counts and modified times in each entry (`extras: ["details"]`).
    fn details(&self) -> bool;
    fn show_hidden(&self) -> bool;
    /// The default prune applies (`defaultExcludes`).
    fn default_excludes(&self) -> bool;
    /// The caller's `include` and `exclude` globs, as written.
    fn globs(&self) -> (&[String], &[String]);

    /// A name or metadata filter (contract `x-listing`) narrows the
    /// listing, so what it skipped may hold the entries asked for.
    fn filtered(&self) -> bool {
        let row = serde_json::to_value(self).unwrap_or_default();
        LISTING_FIELDS
            .iter()
            .any(|(field, role)| role.selects() && crate::response::output::has_value(&row, field))
    }
    /// The order rows are listed in.
    fn sort(&self) -> Sort;
    /// The walk stops at the first match past `maxEntries`: in walk order
    /// the first matches are final once found.
    fn early_exit(&self) -> bool;
    /// Whether a filtered listing could list a path the policy withheld.
    fn filter_probe(&self, root: &std::path::Path) -> Result<FilterProbe, ToolError>;
    /// What decides the walk a listing runs and what it enters, whatever
    /// the window it cuts (the offsets, the rows and their layout): the
    /// walk a cursor continues. That is the root, the operation and every
    /// field the contract gives a listing role (`x-listing`), as the
    /// request spells them.
    fn walk_identity(&self, root: &std::path::Path) -> Value {
        let row = serde_json::to_value(self).unwrap_or_default();
        let fields = LISTING_FIELDS
            .iter()
            .filter_map(|(field, _)| Some(((*field).to_owned(), row.get(*field)?.clone())))
            .collect::<serde_json::Map<_, _>>();
        json!([root.to_string_lossy(), row["operation"], fields])
    }
    /// The engine walk of this query's filters over `root`; the pipeline
    /// sets its bounds and cursor.
    fn engine_options(&self, root: &std::path::Path) -> FileSystemQueryOptions;
    /// This query with every default a broadened listing must keep (the
    /// depth it walked).
    fn broadening(&self) -> Self;
    /// A root this operation cannot list fails the call.
    fn check_root(&self, root: &std::path::Path, paths: &PathPolicy) -> Result<(), ToolError>;
}

/// The [`Listing`] accessors of the fields both generated queries declare
/// under one name, followed by the operation's own items.
macro_rules! listing_fields {
    ($query:ty { $($own:tt)* }) => {
        impl super::listing::Listing for $query {
            fn path(&self) -> &str {
                self.path.as_str()
            }
            fn walk(&self) -> crate::tools::empty_walk::Walk {
                crate::tools::empty_walk::Walk {
                    ignored: self.include_ignored == Some(true),
                    hidden: self.include_hidden == Some(true),
                    default_excludes: crate::policy::prune::DefaultsFlag::defaults(
                        &self.default_excludes,
                    ),
                }
            }
            fn relaxed(&self, relaxed: crate::tools::empty_walk::Walk) -> Self {
                let mut query = self.clone();
                query.include_ignored = Some(relaxed.ignored);
                query.include_hidden = Some(relaxed.hidden);
                query.default_excludes = Some(octocode_contract_types::DefaultExcludes(
                    relaxed.default_excludes,
                ));
                query.scan_offset = None;
                query.walk_offset = None;
                query
            }
            fn page_size(&self) -> Option<usize> {
                crate::response::paging::page_size(
                    crate::id::ToolId::StructureSearch,
                    None,
                    self.page_size.map(|size| size.get()),
                )
            }
            fn page(&self) -> usize {
                crate::num::usize_of(self.page.get()).max(1)
            }
            fn snapshot(&self) -> Option<&str> {
                self.snapshot.as_deref().map(String::as_str)
            }
            fn scan_offset(&self) -> usize {
                self.scan_offset
                    .map_or(0, |offset| crate::num::usize_of(offset.get()))
            }
            fn walk_offset(&self) -> u32 {
                self.walk_offset
                    .map_or(0, |offset| crate::num::u32_of(offset.get()))
            }
            fn requested(&self) -> usize {
                let ceiling = super::max_walk() as usize;
                self.max_entries
                    .map_or(ceiling, |limit| crate::num::usize_of(limit.get()))
                    .min(ceiling)
            }
            fn details(&self) -> bool {
                use octocode_contract_types::ExtraKind;
                self.extras.contains(&ExtraKind::Details)
            }
            fn show_hidden(&self) -> bool {
                self.include_hidden == Some(true)
            }
            fn default_excludes(&self) -> bool {
                crate::policy::prune::DefaultsFlag::defaults(&self.default_excludes)
            }
            fn globs(&self) -> (&[String], &[String]) {
                (&self.include, &self.exclude)
            }
            $($own)*
        }
    };
}
pub(super) use listing_fields;

/// Execute one listing query.
pub(super) fn execute_listing<Q: Listing>(
    q: &Q,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
    budget: std::time::Duration,
) -> StructureResult {
    let validated = validated_root(q.path(), paths, cancel)?;
    let root = validated.canonical.as_path();
    q.check_root(root, paths)?;
    let requested = q.requested();
    // Snapshot fingerprint over the query shape, the page cut, and the
    // ordered result set, so a continuation cursor (page>1) is rejected
    // with `staleSnapshot` when the corpus or query drifted between pages,
    // or a different response window would cut other pages. Effective
    // (not raw) values: the nextPage continuation injects these defaults,
    // so the digest matches what the follow-up request carries.
    let identity = json!([
        q.walk_identity(root),
        q.sort(),
        q.details(),
        requested,
        q.scan_offset(),
        q.walk_offset(),
        q.page_size(),
    ]);
    let policy = paths.identity();
    let page = q.page();
    let (walk, stored) = walked(page, q.snapshot(), &policy, || {
        walk_listing(
            q,
            root,
            paths,
            security,
            cancel,
            requested,
            Walking::Listing(budget),
        )
    })?;
    // The page cut and the row layout are part of the snapshot: a
    // continuation under a different response window restarts instead of
    // skipping entries.
    let budget = crate::response::page_windows::page_budget(response_window, &identity);
    let snapshot =
        crate::digest::json_sha256(&json!([identity, budget, Q::LAYOUT, walk.rows_digest()]));
    if let Some(restart) = restart_if_stale(q, page, q.snapshot(), &snapshot) {
        return Ok(restart);
    }
    let prefix = listing_prefix(paths, root);
    let deferred_detail = q.details() && q.sort() != Sort::Lines;
    let pages = page_ranges(
        &rows::costs(&walk.rows, &prefix, deferred_detail),
        q.page_size(),
        budget,
    );
    if !stored && pages.len() > 1 {
        memo::put(snapshot.clone(), policy, paths, root, &walk);
    }
    let mut out = listing_page(
        q,
        &walk,
        &pages,
        snapshot,
        (root, &prefix),
        requested,
        (deferred_detail.then_some(paths), cancel),
    )?;
    let probe_walk = |relaxed: &Q, entries: usize, walking: Walking<'_>| {
        walk_listing(relaxed, root, paths, security, cancel, entries, walking)
    };
    probe_listing(&mut out, q, (root, paths), &walk.meta.uncovered, probe_walk)?;
    broaden_empty(&mut out, &q.broadening(), root, probe_walk)?;
    Ok(out)
}

/// The response for this page of a walk. `detail` reads this page's files'
/// line counts and times under that path policy.
fn listing_page<Q: Listing>(
    q: &Q,
    walk: &Walked,
    pages: &[std::ops::Range<usize>],
    snapshot: String,
    (root, prefix): (&std::path::Path, &str),
    requested: usize,
    (detail, cancel): (Option<&PathPolicy>, &dyn CancellationCheck),
) -> Result<Value, ToolError> {
    let page = q.page();
    let total = walk.rows.len();
    let cut = walk.cut(
        page,
        pages.len(),
        requested,
        q.early_exit(),
        (q.scan_offset(), q.walk_offset()),
        snapshot.clone(),
    );
    let shown = pages.get(page - 1).cloned().unwrap_or(total..total);
    let shown_rows = walk.rows[shown.clone()]
        .iter()
        .map(|row| Ok((row.dir.clone(), row.page_entry(detail, cancel)?)))
        .collect::<Result<Vec<_>, ToolError>>()?;
    let files = rows::dir_groups(&shown_rows, prefix);
    let mut out = json!({
        "path": display_name(root),
        "files": files,
        "snapshot": snapshot,
    });
    // Every page states its block; the response stage keeps it where it
    // routes (`response::rows::walks_on`).
    let mut facts =
        crate::response::pages::PageFacts::sized(page, cut.total_pages, total, cut.has_more())
            .out_of_range(cut.out_of_range());
    if let Some(size) = q.page_size() {
        facts = facts.with_page_size(size);
    }
    out["pagination"] = facts.to_value();
    // A walk cut by time proved nothing about what it did not scan, and a
    // resumed window past every entry of its filters is no empty listing.
    if total == 0 && walk.meta.time_cut.is_none() && q.walk_offset() == 0 {
        out["status"] = json!("empty");
    }
    let mut warnings = Vec::new();
    cut.finish(&mut out, q, &mut warnings);
    // The walk's summary, what it skipped, its retries and its read
    // describe the whole listing: page 1 states them once.
    if page == 1 {
        walk.meta.disclose(&mut out, root, q.details())?;
        lead_to_skipped(&mut out, q, &walk.meta.uncovered, q.walk(), q.filtered());
        if let Some(read) = rows::read_lead(&walk.rows[shown], root) {
            out["next"]["read"] = read;
        }
    }
    let (include, exclude) = q.globs();
    let query_notes = crate::policy::include::parent_glob_warning(include, exclude)
        .into_iter()
        .collect();
    walk.meta.warn(&mut out, query_notes, warnings);
    Ok(out)
}
