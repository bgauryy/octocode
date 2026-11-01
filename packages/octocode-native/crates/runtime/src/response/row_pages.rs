//! Row-aware response pages: every page is a complete envelope of whole
//! rows, or of parts of a row too large for one page. Oversized rows are
//! planned into fragments by size; only the requested page is built.

mod arena;
mod collections;
mod evidence;
mod plan;
mod shares;
mod skeleton;
mod slicer;
use arena::{Fragment, FragmentArena, Planned, mark_continuations};
use collections::{
    collection_pointer, delivered_collections, drop_delivered_elsewhere,
    drop_empty_declared_siblings, empty_declared_siblings, lean_declared_skeleton,
};
use octocode_engine::digest::JsonFingerprint;
use plan::{Later, PreparedRows, RowPlan};
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use shares::{PartShare, keep_row_next, row_next_chars, row_part_chars};
use skeleton::{continues_chars, enclosing_element, lean_elements, volatile_reserve};
use slicer::{Open, Slicer};
use tokio_util::sync::CancellationToken;

use super::page_model::{ResponsePageOptions, ResponsePagination};
use super::result::Remaining;

#[cfg(test)]
thread_local! {
    /// Row fragments built by the current thread's pager (test sensor).
    static MATERIALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn note_materialized() {
    #[cfg(test)]
    MATERIALIZED.with(|count| count.set(count.get() + 1));
}

/// A planned fragment in output order: its row position, its `rowPart`
/// (when the row is split), and the page it lands on.
struct Placed {
    fragment: Fragment,
    row: usize,
    part: usize,
    of: usize,
    page: usize,
}

/// The incomplete envelope warning a page with rows left over leads with:
/// the query indexes later pages serve first and those they continue.
fn incomplete_banner(later: &Later) -> Value {
    let list = |queries: &[usize]| {
        let noun = if queries.len() == 1 {
            "query"
        } else {
            "queries"
        };
        let queries = queries.iter().map(usize::to_string).collect::<Vec<_>>();
        format!("{noun} {}", queries.join(", "))
    };
    let mut banner = String::from("incomplete: later response pages");
    if !later.unserved.is_empty() {
        banner.push_str(" serve ");
        banner.push_str(&list(&later.unserved));
    }
    if !later.continued.is_empty() {
        banner.push_str(if later.unserved.is_empty() {
            " continue "
        } else {
            " and continue "
        });
        banner.push_str(&list(&later.continued));
    }
    Value::String(banner)
}

/// The most chars the incomplete banner adds to `skeleton`, an envelope of
/// rows with query indexes `queries`: a banner naming every query in both
/// lists (wider than any page's, where each row is in one at most), plus
/// its `warnings` key when the envelope has none.
fn incomplete_banner_chars(skeleton: &Map<String, Value>, queries: &[usize]) -> usize {
    let widest = Later {
        unserved: queries.to_vec(),
        continued: queries.to_vec(),
    };
    let banner = octocode_engine::json_size::utf16_units(&incomplete_banner(&widest)).unwrap_or(0);
    match skeleton.get("warnings").and_then(Value::as_array) {
        Some(warnings) if !warnings.is_empty() => banner + 1,
        Some(_) => banner,
        // `"warnings":[…],`
        None => banner + "\"warnings\":[],".len(),
    }
}

/// One page of the [`RowSource`] of `structured`, with no room kept for the
/// page's `responsePagination` block (the plan tests measure rows alone).
#[cfg(test)]
pub(super) fn paginate_rows(
    structured: Map<String, Value>,
    options: &ResponsePageOptions,
    volatile: Vec<Option<Map<String, Value>>>,
    remaining: &[Vec<Remaining>],
    tool: crate::id::ToolId,
) -> (Map<String, Value>, ResponsePagination) {
    let envelope = octocode_engine::digest::fingerprint_json(&structured).expect("envelope");
    let cancelled = CancellationToken::new();
    RowSource::new(
        structured,
        &envelope,
        options,
        volatile,
        tool,
        &|_| 0,
        &cancelled,
    )
    .expect("valid source")
    .page(options, remaining, &|_| 0, &cancelled)
    .expect("valid page")
}

/// Row-aware pages: the plan that packs whole rows (or fragments of
/// oversized rows) of one envelope into complete envelopes, built once and
/// cut into the zero-based page a cursor addresses. A saved response
/// keeps it, so a later page neither copies nor re-plans the envelope.
///
/// Pages cut at row boundaries, in row order: a row that fits a page is
/// never split, and only a row larger than a page is split into parts
/// (`rowPart`), its first part filling the room its page has left. A split
/// row's page continuations ride its last part ([`PartShare`]). Pages are
/// assigned from fragment sizes; a page materializes only its own
/// fragments, and the page count is the number of calls a reader makes.
///
/// The budget is the whole rendered page, so the plan keeps room for the
/// `responsePagination` block the caller attaches afterwards: `block` is that
/// block's size in chars for a given pagination (0 when none is attached).
/// The room is the block of a page with more behind it, at the widest page
/// numbers. The final query row may finish its remaining array tail in the
/// smaller finished envelope; earlier rows retain the ordinary allowance.
/// The envelope key of every tool's result rows.
const ROWS_KEY: &str = "results";

pub(super) struct RowSource {
    /// The envelope without its rows.
    skeleton: Map<String, Value>,
    plan: RowPlan,
    /// Each row's per-call provider facts, regained by its first part.
    volatile: Vec<Option<Map<String, Value>>>,
    budget: usize,
    snapshot: String,
}

impl RowSource {
    /// Plan `structured`, whose compact rendering is `envelope` (one
    /// streaming pass names its length and content snapshot, so the
    /// rendering is never built), at the `responseChars` of `options`.
    pub(super) fn new(
        mut structured: Map<String, Value>,
        envelope: &JsonFingerprint,
        options: &ResponsePageOptions,
        mut volatile: Vec<Option<Map<String, Value>>>,
        tool: crate::id::ToolId,
        block: &dyn Fn(&ResponsePagination) -> usize,
        cancelled: &CancellationToken,
    ) -> Result<Self, super::pager::ResponseError> {
        let budget = options.response_chars.unwrap_or(1).max(1);
        let rows = match structured.remove(ROWS_KEY) {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        };
        volatile.resize(rows.len(), None);
        let overhead =
            octocode_engine::json_size::utf16_units(&structured).unwrap_or(0) + ROWS_KEY.len() + 6;
        let total = envelope.utf16_units;
        let snapshot = super::cursor::rows_snapshot(budget, &envelope.digest);
        let template = |has_more: bool| ResponsePagination {
            page: total.max(1),
            total_pages: total.max(1),
            has_more,
            returned_chars: total,
            snapshot: snapshot.clone(),
            next_offset: has_more.then_some(total),
            oversized: None,
            next: None,
        };
        // Room for the envelope warning a page with rows left over leads with,
        // and for the pagination block.
        let prepared = PreparedRows::new(rows, &volatile, tool);
        let banner = incomplete_banner_chars(&structured, prepared.queries());
        let row_budget = |reserve: usize| budget.saturating_sub(overhead + banner + reserve).max(1);
        let finished = block(&template(false));
        let widest = block(&template(true));
        // A response whose rows, added up, fit one page beside the block a
        // finished page carries is one page of that block. Past it some row
        // splits, so ordinary fragments reserve the continued-page block.
        // Only the final query row can spend the finished envelope allowance.
        let one_page = total + finished <= budget && prepared.size() <= row_budget(finished);
        let plan = prepared.plan_with_terminal(
            row_budget(if one_page { finished } else { widest }),
            Some(budget.saturating_sub(overhead + finished).max(1)),
            cancelled,
        )?;
        Ok(Self {
            skeleton: structured,
            plan,
            volatile,
            budget,
            snapshot,
        })
    }

    /// The page `options` asks for (its cursor's offset), or a restart page
    /// when its snapshot changed or it is past the last page.
    pub(super) fn page(
        &self,
        options: &ResponsePageOptions,
        remaining: &[Vec<Remaining>],
        block: &dyn Fn(&ResponsePagination) -> usize,
        cancelled: &CancellationToken,
    ) -> Result<(Map<String, Value>, ResponsePagination), super::pager::ResponseError> {
        let Self { plan, .. } = self;
        let mut structured = self.skeleton.clone();
        let requested = options.response_offset.unwrap_or(0);
        let changed = options.response_snapshot.as_deref() != Some(&self.snapshot);
        if requested > 0 && (changed || requested >= plan.total_pages) {
            return Err(super::pager::ResponseError::InvalidPage);
        }
        let selected = plan.build_page(requested, &self.volatile, remaining, cancelled)?;
        let later = plan.later(requested);
        structured.insert(ROWS_KEY.into(), Value::Array(selected));
        if !later.is_empty() {
            lead_with_incomplete_banner(&mut structured, &later);
        }
        let has_more = requested + 1 < plan.total_pages;
        let char_length = octocode_engine::json_size::utf16_units(&structured).unwrap_or(0);
        let mut pagination = ResponsePagination {
            page: requested + 1,
            total_pages: plan.total_pages,
            has_more,
            returned_chars: char_length,
            snapshot: self.snapshot.clone(),
            next_offset: has_more.then_some(requested + 1),
            oversized: None,
            next: None,
        };
        // Oversized: the page with its own block still passes the budget, because
        // one fragment could not be divided further.
        pagination.oversized =
            (pagination.returned_chars + block(&pagination) > self.budget).then_some(true);
        Ok((structured, pagination))
    }
}

/// Lead the envelope with the warning naming the queries `later` pages
/// hold, so a reader meets it before the rows.
fn lead_with_incomplete_banner(structured: &mut Map<String, Value>, later: &Later) {
    let banner = incomplete_banner(later);
    let mut warnings = match structured.shift_remove("warnings") {
        Some(Value::Array(warnings)) => warnings,
        _ => Vec::new(),
    };
    warnings.push(banner);
    structured.shift_insert(0, "warnings".into(), Value::Array(warnings));
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lazy_page_tests;

#[cfg(test)]
mod banner_tests;

impl From<evidence::DependencyError> for super::pager::ResponseError {
    fn from(error: evidence::DependencyError) -> Self {
        match error {
            evidence::DependencyError::Cancelled => Self::Cancelled,
            error => Self::InvalidEvidence(error.to_string()),
        }
    }
}
