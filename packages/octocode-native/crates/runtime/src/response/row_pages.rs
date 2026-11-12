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
    drop_empty_declared_siblings, drop_filled_elsewhere, empty_declared_siblings,
    filled_collections, lean_declared_skeleton,
};
use octocode_engine::digest::JsonFingerprint;
use octocode_io::cancellation::Cancellation;
use plan::{PageBudget, PreparedRows, RowPlan};
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use shares::{PartShare, keep_row_next, row_next_chars, row_part_chars};
use skeleton::{
    continues_chars, enclosing_element, lean_later_context, put_walks, take_enclosing_walks,
};
use slicer::{Open, Slicer};

use super::page_model::{ResponsePageOptions, ResponsePagination};

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

/// One page of the [`RowSource`] of `structured`, with no room kept for the
/// page's `responsePagination` block (the plan tests measure rows alone).
#[cfg(test)]
pub(super) fn paginate_rows(
    structured: Map<String, Value>,
    options: &ResponsePageOptions,
    volatile: Vec<Option<Map<String, Value>>>,
    tool: crate::id::ToolId,
) -> (Map<String, Value>, ResponsePagination) {
    let envelope = octocode_engine::digest::fingerprint_json(&structured).expect("envelope");
    let cancelled = Cancellation::new();
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
    .page(options, &cancelled)
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
/// The envelope members that note the whole answer, stated by its first page.
const ENVELOPE_NOTICES: [&str; 2] = ["warnings", "hints"];
const RESPONSE_PAGINATION: &str = "responsePagination";

pub(super) struct RowSource {
    /// The envelope without its rows.
    skeleton: Map<String, Value>,
    plan: RowPlan,
    /// Each row's per-call provider facts, regained by its first part after
    /// the cut: no page edge or page measure depends on them.
    volatile: Vec<Option<Map<String, Value>>>,
    snapshot: String,
    /// The call that reads on past the last page: the rows' walks, stated
    /// as the envelope's one continuation ([`crate::response::continuations::hoist`]).
    onward: Option<Value>,
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
        cancelled: &Cancellation,
    ) -> Result<Self, super::pager::ResponseError> {
        let budget = options.response_chars.unwrap_or(1).max(1);
        let rows = match structured.remove(ROWS_KEY) {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        };
        volatile.resize(rows.len(), None);
        let onward = structured
            .remove(RESPONSE_PAGINATION)
            .and_then(|mut block| block.get_mut("next").map(Value::take));
        let overhead =
            octocode_engine::json_size::utf16_units(&structured).unwrap_or(0) + ROWS_KEY.len() + 6;
        let total = envelope.utf16_units;
        let snapshot = super::cursor::rows_snapshot(budget, &envelope.digest);
        let template = |more_pages: bool| ResponsePagination {
            #[cfg(test)]
            total_pages: 1,
            has_more: more_pages || onward.is_some(),
            #[cfg(test)]
            returned_chars: total,
            snapshot: snapshot.clone(),
            next_offset: more_pages.then_some(total),
            next: onward.clone().filter(|_| !more_pages),
        };
        // Room for the pagination block.
        let prepared = PreparedRows::new(rows, tool);
        let row_budget = |reserve: usize| budget.saturating_sub(overhead + reserve).max(1);
        let finished = block(&template(false));
        let widest = block(&template(true));
        // A response whose rows, added up, fit one page beside the block a
        // finished page carries is one page of that block. Past it some row
        // splits, so ordinary fragments reserve the continued-page block.
        // Only the final query row can spend the finished envelope allowance.
        let one_page = total + finished <= budget && prepared.size() <= row_budget(finished);
        let block_chars = if one_page { finished } else { widest };
        let plan = prepared.plan_with_terminal(
            PageBudget {
                rows: row_budget(block_chars),
                terminal: Some(budget.saturating_sub(overhead + finished).max(1)),
                envelope: overhead + block_chars,
            },
            cancelled,
        )?;
        Ok(Self {
            skeleton: structured,
            plan,
            volatile,
            snapshot,
            onward,
        })
    }

    /// The page `options` asks for (its cursor's offset), or a restart page
    /// when its snapshot changed or it is past the last page.
    pub(super) fn page(
        &self,
        options: &ResponsePageOptions,
        cancelled: &Cancellation,
    ) -> Result<(Map<String, Value>, ResponsePagination), super::pager::ResponseError> {
        let Self { plan, .. } = self;
        let mut structured = self.skeleton.clone();
        let requested = options.response_offset.unwrap_or(0);
        let changed = options.response_snapshot.as_deref() != Some(&self.snapshot);
        if requested > 0 && (changed || requested >= plan.total_pages) {
            return Err(super::pager::ResponseError::InvalidPage);
        }
        let selected = plan.build_page(requested, &self.volatile, cancelled)?;
        // The answer's notices (an input repair, a call-wide warning) are
        // stated once, by its first page; a later page carries its rows.
        if requested > 0 {
            for notice in ENVELOPE_NOTICES {
                structured.shift_remove(notice);
            }
        }
        structured.insert(ROWS_KEY.into(), Value::Array(selected));
        let more_pages = requested + 1 < plan.total_pages;
        // A page holding a value wider than the budget alone (one fragment
        // could not be divided further) is stated where it is served
        // (`pager::oversized`).
        let pagination = ResponsePagination {
            #[cfg(test)]
            total_pages: plan.total_pages,
            has_more: more_pages || self.onward.is_some(),
            #[cfg(test)]
            returned_chars: octocode_engine::json_size::utf16_units(&structured).unwrap_or(0)
                - super::pager::volatile_chars(&structured),
            snapshot: self.snapshot.clone(),
            next_offset: more_pages.then_some(requested + 1),
            next: self.onward.clone().filter(|_| !more_pages),
        };
        Ok((structured, pagination))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lazy_page_tests;

impl From<evidence::DependencyError> for super::pager::ResponseError {
    fn from(error: evidence::DependencyError) -> Self {
        match error {
            evidence::DependencyError::Cancelled => Self::Cancelled,
            error => Self::InvalidEvidence(error.to_string()),
        }
    }
}
