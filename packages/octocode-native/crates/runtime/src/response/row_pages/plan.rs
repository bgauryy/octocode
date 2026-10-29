use super::super::result::Remaining;
use super::arena::Window;
use super::evidence::DependencyError;
use super::{
    FragmentArena, PartShare, Placed, delivered_collections, drop_delivered_elsewhere, fair_shares,
    keep_row_next, row_next_chars, row_part_chars, volatile_reserve,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use tokio_util::sync::CancellationToken;

mod fields;
use fields::{insert_field, restore_fields, take_guidance};

/// The page plan of a row-scoped response: every fragment of every row,
/// placed on a page by its size.
pub(super) struct RowPlan {
    arena: FragmentArena,
    first_fields: Vec<Map<String, Value>>,
    /// Each row's continuations, hints and related calls, set aside while
    /// its parts are packed and restored on the parts that carry them.
    guidance: Vec<Map<String, Value>>,
    /// Each row's fixed collections that held items: a part carrying one
    /// empty had it delivered on another part.
    delivered: Vec<Vec<String>>,
    placed: Vec<Placed>,
    pub(super) total_pages: usize,
    pub(super) rows: usize,
}

/// The rows of one envelope readied for planning at any row budget: their
/// first-fragment fields set aside and each row sized, once. The size of the
/// whole ([`Self::size`]) tells which page-block reserve a plan needs before
/// any plan is built, so no plan is built twice.
#[derive(Clone)]
pub(super) struct PreparedRows {
    rows: Vec<Value>,
    first_fields: Vec<Map<String, Value>>,
    reserves: Vec<usize>,
    /// Each row's chars on a page: its comma and reserve added.
    sizes: Vec<usize>,
    tool: Option<crate::id::ToolId>,
    volatile: Vec<Option<Map<String, Value>>>,
}

impl PreparedRows {
    pub(super) fn new(
        mut rows: Vec<Value>,
        volatile: &[Option<Map<String, Value>>],
        tool: Option<crate::id::ToolId>,
    ) -> Self {
        let first_names = tool.map_or(&[][..], |tool| tool.output().first_fragment_fields());
        let before = rows
            .iter()
            .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0))
            .collect::<Vec<_>>();
        let first_fields = rows
            .iter_mut()
            .map(|row| {
                let mut fields = Map::new();
                for pointer in first_names {
                    let Some((parent, name)) = pointer.rsplit_once('/') else {
                        continue;
                    };
                    let key = name.replace("~1", "/").replace("~0", "~");
                    let Some(value) = row
                        .pointer_mut(parent)
                        .and_then(Value::as_object_mut)
                        .and_then(|object| object.shift_remove(&key))
                    else {
                        continue;
                    };
                    let segments = parent
                        .split('/')
                        .skip(1)
                        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
                        .chain(std::iter::once(key))
                        .collect::<Vec<_>>();
                    insert_field(&mut fields, &segments, value);
                }
                fields
            })
            .collect::<Vec<_>>();
        let reserves = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                before[index]
                    .saturating_sub(octocode_engine::json_size::utf16_units(row).unwrap_or(0))
                    + volatile
                        .get(index)
                        .and_then(Option::as_ref)
                        .map_or(0, volatile_reserve)
            })
            .collect::<Vec<_>>();
        let chars: Vec<usize> = rows
            .iter()
            .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0))
            .collect();
        let sizes = chars
            .iter()
            .zip(&reserves)
            .map(|(chars, reserve)| chars + 1 + reserve)
            .collect();
        Self {
            rows,
            first_fields,
            reserves,
            sizes,
            tool,
            volatile: volatile.to_vec(),
        }
    }

    /// The rows' sizes added up: what they take on a page when none is split.
    pub(super) fn size(&self) -> usize {
        self.sizes.iter().sum()
    }

    #[cfg(test)]
    pub(super) fn plan(
        self,
        row_budget: usize,
        cancelled: &CancellationToken,
    ) -> Result<RowPlan, DependencyError> {
        self.plan_with_terminal(row_budget, None, cancelled)
    }

    /// Only the final query row's tail may use the finished envelope allowance.
    pub(super) fn plan_with_terminal(
        self,
        row_budget: usize,
        terminal_budget: Option<usize>,
        cancelled: &CancellationToken,
    ) -> Result<RowPlan, DependencyError> {
        let Self {
            rows,
            first_fields,
            reserves,
            sizes,
            tool,
            volatile,
        } = self;
        let shares = fair_shares(&sizes, row_budget);
        let page_context = tool.map(crate::contracts::output_fields::page_context_fields);
        let mut arena = FragmentArena {
            page_context,
            collection_routes: page_context.map(|context| &context.collection_routes),
            ..FragmentArena::default()
        };
        let mut heads = Vec::with_capacity(rows.len());
        let mut tails = Vec::new();
        let mut guidance = Vec::with_capacity(rows.len());
        let mut delivered = Vec::with_capacity(rows.len());
        for (position, mut row) in rows.into_iter().enumerate() {
            // Each part carries only its share of the row's guidance
            // ([`PartShare`]): parts are packed without it, then sized with
            // the share they carry.
            let kept = [PartShare::Whole, PartShare::First, PartShare::Last]
                .map(|share| row_next_chars(&row, share));
            guidance.push(take_guidance(&mut row));
            let catalog = arena.take_catalog(&mut row, cancelled)?;
            delivered.push(page_context.map_or_else(Vec::new, |context| {
                delivered_collections(&row, &context.fixed_paths)
            }));
            let mut extra = arena.dependencies(
                catalog,
                &Value::Object(first_fields[position].clone()),
                cancelled,
            )?;
            if let Some(fields) = volatile.get(position).and_then(Option::as_ref) {
                let added =
                    arena.dependencies(catalog, &Value::Object(fields.clone()), cancelled)?;
                arena.merge_evidence(&mut extra, &added, cancelled)?;
            }
            let extra_chars = arena.evidence_chars(&extra)?;
            let head = head_limit(sizes[position], shares[position], reserves[position]);
            let parts = plan_row(
                &mut arena,
                row,
                catalog,
                position,
                Window {
                    head: head.saturating_sub(extra_chars + kept[1]).max(1),
                    budget: row_budget
                        .saturating_sub(1 + row_part_chars(sizes[position], sizes[position]))
                        .max(1),
                    terminal_budget: terminal_budget.filter(|_| position + 1 == sizes.len()).map(
                        |budget| {
                            budget
                                .saturating_sub(
                                    1 + row_part_chars(sizes[position], sizes[position]),
                                )
                                .max(1)
                        },
                    ),
                    cancelled,
                    continuation_chars: 0,
                    tail: kept[2],
                },
                reserves[position],
                &extra,
                kept,
            )?;
            for placed in parts {
                if placed.0.part == 1 {
                    heads.push(placed);
                } else {
                    tails.push(placed);
                }
            }
        }
        let (placed, total_pages) = assign_pages(heads.into_iter().chain(tails), row_budget);
        Ok(RowPlan {
            arena,
            first_fields,
            guidance,
            delivered,
            placed,
            total_pages,
            rows: sizes.len(),
        })
    }
}

impl RowPlan {
    /// Build page `page`'s rows in row order, materializing only its
    /// fragments; part one regains its row's per-call provider facts.
    pub(super) fn build_page(
        &self,
        page: usize,
        volatile: &[Option<Map<String, Value>>],
        remaining: &[Vec<Remaining>],
        cancelled: &CancellationToken,
    ) -> Result<Vec<Value>, DependencyError> {
        let mut parts: Vec<&Placed> = self.placed.iter().filter(|p| p.page == page).collect();
        parts.sort_by_key(|p| (p.row, p.part));
        parts
            .into_iter()
            .map(|p| {
                if cancelled.is_cancelled() {
                    return Err(DependencyError::Cancelled);
                }
                let mut value = self.arena.materialize(&p.fragment);
                if let Some(context) = self.arena.page_context {
                    drop_delivered_elsewhere(
                        &mut value,
                        &self.delivered[p.row],
                        &context.optional_paths,
                    );
                }
                if let Some(row) = value.as_object_mut() {
                    restore_fields(row, self.guidance[p.row].clone());
                }
                if p.part == 1
                    && let Some(row) = value.as_object_mut()
                {
                    restore_fields(row, self.first_fields[p.row].clone());
                }
                if p.part == 1
                    && let (Some(fields), Some(data)) = (
                        volatile[p.row].as_ref(),
                        value.get_mut("data").and_then(Value::as_object_mut),
                    )
                {
                    data.extend(fields.clone());
                }
                keep_row_next(
                    &mut value,
                    PartShare::of(p.part, p.of),
                    remaining.get(p.row).map_or(&[][..], Vec::as_slice),
                );
                let continues = value
                    .as_object_mut()
                    .and_then(|row| row.shift_remove("rowPart"))
                    .and_then(|mut marker| marker.get_mut("continues").map(Value::take));
                if p.of > 1 {
                    value["rowPart"] = json!({"part": p.part, "of": p.of});
                    if let Some(continues) = continues {
                        value["rowPart"]["continues"] = continues;
                    }
                }
                self.arena
                    .attach_evidence(&p.fragment, &mut value, cancelled)?;
                Ok(value)
            })
            .collect()
    }

    /// How many rows have a part on a page after `page`.
    pub(super) fn rows_after(&self, page: usize) -> usize {
        let mut later: Vec<usize> = self
            .placed
            .iter()
            .filter(|p| p.page > page)
            .map(|p| p.row)
            .collect();
        later.sort_unstable();
        later.dedup();
        later.len()
    }
}

/// A row's first-fragment bound: unbounded when the row fits its fair
/// share of page one, else that share less the row's comma, per-call
/// facts, and `rowPart`.
fn head_limit(size: usize, share: usize, reserve: usize) -> usize {
    if size <= share {
        usize::MAX
    } else {
        share
            .saturating_sub(1 + reserve + row_part_chars(1, size.max(1)))
            .max(1)
    }
}

/// Plan one row into its parts, each sized as it
/// serializes on a page: its comma and `rowPart`, the per-call facts part one
/// regains, less the continuations and hints the part does not carry
/// ([`PartShare`]).
fn plan_row(
    arena: &mut FragmentArena,
    row: Value,
    catalog: Option<usize>,
    position: usize,
    window: Window<'_>,
    reserve: usize,
    extra: &super::arena::Evidence,
    [whole, first, last]: [usize; 3],
) -> Result<Vec<(Placed, usize)>, DependencyError> {
    let mut fragments = Vec::new();
    arena.plan_in(row, catalog, window, &mut fragments)?;
    if let Some((first, chars)) = fragments.first_mut() {
        *chars += arena.evidence_delta(&first.evidence, extra, window.cancelled)?;
        arena.merge_evidence(&mut first.evidence, extra, window.cancelled)?;
    }
    let of = fragments.len();
    fragments
        .into_iter()
        .enumerate()
        .map(|(index, (fragment, chars))| {
            let part = index + 1;
            let added = if part == 1 { reserve } else { 0 };
            let kept = match PartShare::of(part, of) {
                PartShare::Whole => whole,
                PartShare::First => first,
                PartShare::Middle => 0,
                PartShare::Last => last,
            };
            let part_chars = if of > 1 { row_part_chars(part, of) } else { 0 };
            let placed = Placed {
                fragment,
                row: position,
                part,
                of,
                page: 0,
            };
            Ok((placed, chars + 1 + part_chars + added + kept))
        })
        .collect()
}

/// Assign pages in order: a page takes fragments while they fit
/// `row_budget`, and never two parts of one row, so the entry for a row on
/// a page is all of that row the page holds. Returns the fragments and the
/// page count.
fn assign_pages(
    order: impl Iterator<Item = (Placed, usize)>,
    row_budget: usize,
) -> (Vec<Placed>, usize) {
    let mut placed = Vec::new();
    let mut page = 0usize;
    let mut page_chars = 0usize;
    let mut page_rows = BTreeSet::new();
    for (mut fragment, chars) in order {
        if !page_rows.is_empty()
            && (page_rows.contains(&fragment.row) || page_chars + chars > row_budget)
        {
            page += 1;
            page_chars = 0;
            page_rows.clear();
        }
        page_chars += chars;
        page_rows.insert(fragment.row);
        fragment.page = page;
        placed.push(fragment);
    }
    (placed, page + 1)
}
