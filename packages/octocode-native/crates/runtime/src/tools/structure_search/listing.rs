//! The one listing pipeline `tree` and `files` run: walk (or reuse the walk
//! a first page stored), bind the page cut to a snapshot, cut and render the
//! page, and probe an empty listing. An operation contributes only its
//! query: the walk it asks the engine for ([`Listing`]).
use super::probe::{FilterProbe, broaden_empty, deepen_empty, lead_to_skipped, probe_listing};
use super::walked::{PageCut, Walked, push_warnings};
use super::{
    Walking, listing_prefix, memo, restart_if_stale, rows, validated_root, walked, windows,
};
use crate::response::result::ToolError;
use crate::tools::empty_walk::Walk;
use serde_json::{Value, json};

use crate::contracts::listing_roles::structure_search;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_contract_types::{StructureSearchSort, StructureSearchView};
use octocode_engine::types::FileSystemQueryOptions;

mod walk;

#[cfg(test)]
pub(super) use walk::listing_walks;
use walk::walk_listing;

/// A listing operation's query: the fields both operations share
/// ([`listing_fields!`]) and what the operation decides itself.
pub(super) trait Listing:
    Clone
    + serde::Serialize
    + serde::de::DeserializeOwned
    + Into<octocode_contract_types::StructureSearchQuery>
{
    /// Page layout tag folded into the snapshot.
    const LAYOUT: &'static str;

    fn path(&self) -> &str;
    fn view(&self) -> StructureSearchView;
    /// Which paths the request's own walk takes.
    fn walk(&self) -> Walk;
    /// This query with `relaxed` lifting skips, walking from its first
    /// entry: the resume cursors of the request's own walk do not apply.
    fn relaxed(&self, relaxed: Walk) -> Self;
    /// This query moved by `to`: a continuation's position.
    fn moved(&self, to: super::cut::Move) -> Self;
    /// Private page position within the saved listing.
    fn page(&self) -> usize;
    fn snapshot(&self) -> Option<&str>;
    /// Listing rows (walk or sort order) earlier windows listed.
    fn scan_offset(&self) -> usize;
    /// Walk entries (walk order) an earlier, time-cut walk inspected.
    fn walk_offset(&self) -> u32;
    /// Rows one window lists after the earlier windows' (`scanOffset`):
    /// the internal walk window.
    fn requested(&self) -> usize {
        super::max_entries()
    }
    /// Rows per page (`limit`); `None` cuts pages by the budget alone.
    fn page_rows(&self) -> Option<usize>;
    fn show_hidden(&self) -> bool;
    /// The default prune applies (`defaultExcludes`).
    fn default_excludes(&self) -> bool;
    /// The caller's `include` and `exclude` globs, as written.
    fn globs(&self) -> (&[String], &[String]);

    /// A name or metadata filter (contract `x-listing`) narrows the
    /// listing, so what it skipped may hold the entries asked for.
    fn filtered(&self) -> bool {
        let row = serde_json::to_value(self).unwrap_or_default();
        structure_search::FIELDS
            .iter()
            .any(|(field, role)| role.selects() && crate::response::output::has_value(&row, field))
    }
    /// The order rows are listed in.
    fn sort(&self) -> StructureSearchSort;
    /// The walk stops at the first match past `limit`: in walk order
    /// the first matches are final once found.
    fn early_exit(&self) -> bool;
    /// Whether a filtered listing could list a path the policy withheld.
    fn filter_probe(&self, root: &std::path::Path) -> Result<FilterProbe, ToolError>;
    /// What decides the walk a listing runs and what it enters, whatever
    /// the window it cuts (the offsets, the rows and their layout): the
    /// walk a cursor continues. That is the root, the operation and every
    /// field the contract gives a listing role (`x-listing`), as the
    /// request spells them, a field at its contract default as its omission
    /// ([`crate::response::result::canonical_row`]).
    fn walk_identity(&self, root: &std::path::Path) -> Value {
        let mut row = serde_json::to_value(self).unwrap_or_default();
        crate::response::result::canonical_row(crate::id::ToolId::StructureSearch, &mut row);
        let fields = structure_search::FIELDS
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
    /// Which side of the test-code line the listing keeps; `None` both.
    fn test_code(&self) -> Option<octocode_contract_types::TestCode> {
        None
    }
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
            fn view(&self) -> octocode_contract_types::StructureSearchView {
                self.view
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
            fn moved(&self, to: super::cut::Move) -> Self {
                use super::cut::Move;
                let first = std::num::NonZeroU64::MIN;
                let token = |snapshot: String| {
                    snapshot
                        .parse()
                        .expect("a listing snapshot is a sha256 hex digest")
                };
                match to {
                    Move::Page { page, snapshot } => Self {
                        page,
                        snapshot: Some(token(snapshot)),
                        ..self.clone()
                    },
                    // A sorted listing's next window ranks the whole scope
                    // again from its first entry.
                    Move::Window { scan_offset, snapshot } => Self {
                        page: first,
                        snapshot: snapshot.map(token),
                        scan_offset,
                        walk_offset: self
                            .walk_offset
                            .filter(|_| super::files::is_traversal(&super::listing::Listing::sort(self))),
                        ..self.clone()
                    },
                    Move::Resume { walk_offset, scan_offset, snapshot } => Self {
                        page: first,
                        snapshot: snapshot.map(token),
                        scan_offset,
                        walk_offset: Some(walk_offset),
                        ..self.clone()
                    },
                }
            }
            fn page(&self) -> usize {
                octocode_engine::num::saturating_usize(self.page.get()).max(1)
            }
            fn snapshot(&self) -> Option<&str> {
                self.snapshot.as_deref().map(String::as_str)
            }
            fn scan_offset(&self) -> usize {
                self.scan_offset
                    .map_or(0, |offset| octocode_engine::num::saturating_usize(offset.get()))
            }
            fn walk_offset(&self) -> u32 {
                self.walk_offset
                    .map_or(0, |offset| octocode_engine::num::saturating_u32(offset.get()))
            }
            fn page_rows(&self) -> Option<usize> {
                self.limit.map(|limit| octocode_engine::num::saturating_usize(limit.get()))
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
) -> Result<Value, ToolError> {
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
        q.view(),
        q.sort(),
        requested,
        q.page_rows(),
        q.scan_offset(),
        q.walk_offset(),
    ]);
    let policy = paths.identity();
    let page = q.page();
    let (walk, stored) = walked(page, q.snapshot(), &policy, || {
        windows::ranked_window(q, (root, security), &policy, requested).map_or_else(
            || {
                walk_listing(
                    q,
                    root,
                    paths,
                    security,
                    cancel,
                    requested,
                    Walking::Listing(budget),
                )
            },
            Ok,
        )
    })?;
    if let Some(before) = walk.meta.pinned_before.as_deref()
        && let Some(restart) = restart_if_stale(q, windows::pinned(q), before)
    {
        return Ok(restart);
    }
    // The page cut and the row layout are part of the snapshot: a
    // continuation under a different response window restarts instead of
    // skipping entries.
    let budget = crate::response::page_windows::page_budget(response_window);
    let mut bound = vec![
        identity,
        json!(budget),
        json!(Q::LAYOUT),
        json!(walk.rows_digest()),
    ];
    // A later page of a listing past one window binds the windows before it too.
    bound.extend(
        walk.meta
            .pins_through
            .as_ref()
            .map(|through| json!(through)),
    );
    let snapshot = crate::digest::json_sha256(&bound);
    if let Some(restart) = restart_if_stale(q, q.snapshot().filter(|_| page > 1), &snapshot) {
        return Ok(restart);
    }
    let prefix = listing_prefix(paths, root);
    let deferred_detail = q.view() == StructureSearchView::Details
        && super::files::sort_key(&q.sort())
            != Some(octocode_contract_types::StructureSearchSortAnyOrderBy::LineCount);
    let pages = walk.pages(PageCut {
        prefix: prefix.clone(),
        view: q.view(),
        deferred_detail,
        budget,
        limit: q.page_rows(),
    });
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
    deepen_empty(&mut out, q, root, |deeper, entries, walking| {
        walk_listing(deeper, root, paths, security, cancel, entries, walking)
    })?;
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
    let shown_rows = rows::page_entries(&walk.rows[shown.clone()], q.view(), detail, cancel)?;
    let files = rows::dir_groups(&shown_rows, prefix);
    let mut out = json!({
        "path": prefix,
        "files": files,
        "snapshot": snapshot,
    });
    // Every page states its block; the response stage keeps it where it
    // routes (`response::rows::walks_on`).
    // A window `next.nextPage` continues is not the listing's end: its
    // rows are what this window loaded, not the listing's total.
    let continues = cut.walk_lead().is_some();
    let has_more = cut.has_more() || continues;
    // A window of a listing walked whole counts the listing's rows, exactly;
    // its page count is known only once the later windows are cut, so it
    // states none.
    let ranking = cut.limit_cut && cut.whole_walk();
    let facts = if ranking {
        crate::response::pages::PageFacts::open(page, None, has_more).with_items(cut.matched())
    } else {
        crate::response::pages::PageFacts::sized(page, cut.total_pages, total, has_more)
    }
    .out_of_range(cut.out_of_range());
    out["pagination"] = facts.to_value();
    if ranking {
        out["pagination"]["countScope"] = json!("complete");
    } else if continues {
        out["pagination"]["countScope"] = json!("loaded");
    }
    // A walk cut by time proved nothing about what it did not scan, and a
    // resumed walk-order window past every entry of its filters is no empty
    // listing; a sorted window that ended its walk ranked the whole scope.
    let whole_scope = q.walk_offset() == 0 || !super::files::is_traversal(&q.sort());
    if total == 0 && walk.meta.time_cut.is_none() && whole_scope && walk.meta.available == 0 {
        out["status"] = json!("empty");
    }
    let mut warnings = Vec::new();
    cut.finish(&mut out, q, &mut warnings);
    // The walk's summary, what it skipped, its retries and its read
    // describe the whole listing: page 1 states them once, and a sorted walk
    // states them when it ends (its paused windows carry them there).
    if page == 1 && walk.meta.time_window != super::cut::TimeWindow::Paused {
        walk.meta.disclose(&mut out, root)?;
        lead_to_skipped(&mut out, q, &walk.meta.uncovered, q.walk(), q.filtered());
        if let Some(read) = rows::read_lead(&walk.rows[shown], root) {
            out["calls"]["read"] = read;
        }
    }
    let (include, exclude) = q.globs();
    let query_notes = crate::policy::include::parent_glob_warning(include, exclude)
        .into_iter()
        .collect();
    push_warnings(&mut out, query_notes, warnings);
    Ok(out)
}
