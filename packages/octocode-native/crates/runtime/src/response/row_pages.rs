//! Row-aware response pages: every page is a complete envelope of whole
//! rows, or of parts of a row too large for one page. Oversized rows are
//! planned into fragments by size; only the requested page is built.

mod arena;
mod plan;
mod shares;
mod skeleton;
mod slicer;
use arena::{Fragment, FragmentArena, Planned, mark_continuations};
use octocode_engine::digest::JsonFingerprint;
use plan::{PreparedRows, RowPlan};
use serde_json::{Map, Value, json};
pub(crate) use shares::fair_shares;
use shares::{PartShare, keep_row_next, row_next_chars, row_part_chars};
use skeleton::{
    collection_pointer, continues_chars, drop_emptied_lists, empty_secondary_lists,
    enclosing_element, lean_elements, lean_skeleton, take_largest_array, volatile_reserve,
};
use slicer::{Open, Slicer};

use super::page_model::{ResponsePageOptions, ResponsePagination, restart_pagination};
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

/// The `incomplete — …` envelope warning a page with `later` of `rows`
/// rows left over leads with.
fn incomplete_banner(later: usize, rows: usize) -> Value {
    Value::String(format!(
        "incomplete — {later} of {rows} rows continue on later response pages"
    ))
}

/// The most chars the incomplete banner adds to `skeleton`, an envelope of
/// `rows` rows: the widest count, plus its `warnings` key when the envelope
/// has none.
fn incomplete_banner_chars(skeleton: &Map<String, Value>, rows: usize) -> usize {
    let banner =
        octocode_engine::json_size::utf16_units(&incomplete_banner(rows, rows)).unwrap_or(0);
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
    tool: Option<crate::id::ToolId>,
) -> (Map<String, Value>, ResponsePagination) {
    let envelope = octocode_engine::digest::fingerprint_json(&structured).expect("envelope");
    RowSource::new(structured, &envelope, options, volatile, tool, &|_| 0).page(
        options,
        remaining,
        &|_| 0,
    )
}

/// Row-aware pages: the plan that packs whole rows (or fragments of
/// oversized rows) of one envelope into complete envelopes, built once and
/// cut into the zero-based page a cursor addresses. A saved response
/// keeps it, so a later page neither copies nor re-plans the envelope.
///
/// The first page gives every row a fair share of the budget, so each row
/// shows its head (with its drill-downs and handoffs) before any row
/// continues; the rest of each row follows in row order, and a split row's
/// page continuations ride its last part ([`PartShare`]). Pages are assigned
/// from fragment sizes; a page materializes only its own fragments.
///
/// The budget is the whole rendered page, so the plan keeps room for the
/// `responsePagination` block the caller attaches afterwards: `block` is that
/// block's size in chars for a given pagination (0 when none is attached).
/// The room is the block of a page with more behind it, at the widest page
/// numbers, so every page of one plan has the same row budget; a response
/// that fits one page keeps only the block a finished page carries.
pub(super) struct RowSource {
    /// The envelope without its rows.
    skeleton: Map<String, Value>,
    rows_key: &'static str,
    plan: RowPlan,
    /// Each row's per-call provider facts, regained by its first part.
    volatile: Vec<Option<Map<String, Value>>>,
    budget: usize,
    /// UTF-16 length of the whole envelope.
    total: usize,
    snapshot: String,
}

impl RowSource {
    /// Plan `structured`, whose compact rendering is `envelope` (one
    /// streaming pass names its length and content snapshot, so the
    /// rendering is never built), at the `responseLength` of `options`.
    pub(super) fn new(
        mut structured: Map<String, Value>,
        envelope: &JsonFingerprint,
        options: &ResponsePageOptions,
        mut volatile: Vec<Option<Map<String, Value>>>,
        tool: Option<crate::id::ToolId>,
        block: &dyn Fn(&ResponsePagination) -> usize,
    ) -> Self {
        let budget = options.response_length.unwrap_or(1).max(1);
        let rows_key = tool.map_or("results", |tool| tool.output().rows_key());
        let rows = match structured.remove(rows_key) {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        };
        volatile.resize(rows.len(), None);
        let overhead =
            octocode_engine::json_size::utf16_units(&structured).unwrap_or(0) + rows_key.len() + 6;
        let total = envelope.utf16_units;
        let snapshot = super::cursor::rows_snapshot(budget, &envelope.digest);
        let template = |has_more: bool| ResponsePagination {
            scope: "rows".into(),
            current_page: total.max(1),
            total_pages: total.max(1),
            has_more,
            char_offset: total,
            char_length: total,
            total_chars: total,
            snapshot: snapshot.clone(),
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(total),
            oversized: None,
            next: None,
        };
        // Room for the envelope warning a page with rows left over leads with,
        // and for the pagination block.
        let banner = incomplete_banner_chars(&structured, rows.len());
        let row_budget = |reserve: usize| budget.saturating_sub(overhead + banner + reserve).max(1);
        let finished = block(&template(false));
        let widest = block(&template(true));
        let prepared = PreparedRows::new(rows, &volatile, tool);
        // A response whose rows, added up, fit one page beside the block a
        // finished page carries is one page of that block. Past it some row
        // splits, so every page of the plan carries the block of a page
        // with more behind it.
        let one_page = total + finished <= budget && prepared.size() <= row_budget(finished);
        let plan = prepared.plan(row_budget(if one_page { finished } else { widest }));
        Self {
            skeleton: structured,
            rows_key,
            plan,
            volatile,
            budget,
            total,
            snapshot,
        }
    }

    /// The page `options` asks for (its cursor's offset), or a restart page
    /// when its snapshot changed or it is past the last page.
    pub(super) fn page(
        &self,
        options: &ResponsePageOptions,
        remaining: &[Vec<Remaining>],
        block: &dyn Fn(&ResponsePagination) -> usize,
    ) -> (Map<String, Value>, ResponsePagination) {
        let Self {
            plan,
            rows_key,
            total,
            ..
        } = self;
        let (rows_key, total) = (*rows_key, *total);
        let mut structured = self.skeleton.clone();
        let requested = options.response_offset.unwrap_or(0);
        let changed = options.response_snapshot.as_deref() != Some(&self.snapshot);
        if requested > 0 && (changed || requested >= plan.total_pages) {
            structured.insert(rows_key.into(), Value::Array(Vec::new()));
            // The page asked for, named in the envelope so a restart says why.
            let reason = if changed {
                "the response or page size changed, or its cursor is missing; restart from the first page"
                    .to_owned()
            } else {
                format!(
                    "page {} of {} is past the last page; restart from the first page",
                    requested + 1,
                    plan.total_pages
                )
            };
            structured.shift_insert(0, "warnings".into(), json!([reason]));
            let pagination = restart_pagination(
                "rows",
                requested + 1,
                requested,
                plan.total_pages,
                total,
                self.snapshot.clone(),
                options,
                changed,
                changed,
            );
            return (structured, pagination);
        }
        let selected = plan.build_page(requested, &self.volatile, remaining);
        let later = plan.rows_after(requested);
        structured.insert(rows_key.into(), Value::Array(selected));
        if later > 0 {
            lead_with_incomplete_banner(&mut structured, later, plan.rows);
        }
        let has_more = requested + 1 < plan.total_pages;
        let char_length = octocode_engine::json_size::utf16_units(&structured).unwrap_or(0);
        let mut pagination = ResponsePagination {
            scope: "rows".into(),
            current_page: requested + 1,
            total_pages: plan.total_pages,
            has_more,
            char_offset: requested,
            char_length,
            total_chars: total,
            snapshot: self.snapshot.clone(),
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(requested + 1),
            oversized: None,
            next: None,
        };
        // Oversized: the page with its own block still passes the budget, because
        // one fragment could not be divided further.
        pagination.oversized = (char_length + block(&pagination) > self.budget).then_some(true);
        (structured, pagination)
    }
}

/// Lead the envelope with the warning that `later` of `rows` rows continue
/// on later pages, so a reader meets it before the rows.
fn lead_with_incomplete_banner(structured: &mut Map<String, Value>, later: usize, rows: usize) {
    let banner = incomplete_banner(later, rows);
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
