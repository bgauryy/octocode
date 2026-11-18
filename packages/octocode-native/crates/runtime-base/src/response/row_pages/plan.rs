use super::arena::Window;
use super::evidence::DependencyError;
use super::{
    FragmentArena, PartShare, Placed, delivered_collections, drop_delivered_elsewhere,
    drop_filled_elsewhere, filled_collections, keep_row_next, row_next_chars, row_part_chars,
};
use octocode_io::cancellation::Cancellation;
use serde_json::{Map, Value, json};

mod fields;
mod pages;
use fields::{insert_field, restore_fields, take_guidance};
pub(super) use pages::PageBudget;
use pages::{Pages, head_limit};

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
    /// Each row's declared collections that held items, at their pointers:
    /// a part holding one empty had its items delivered on another part.
    filled: Vec<std::collections::BTreeSet<String>>,
    placed: Vec<Placed>,
    pub(super) total_pages: usize,
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
    tool: crate::id::ToolId,
}

impl PreparedRows {
    /// `rows` without their per-call provider facts: pages are planned on
    /// evidence alone, and [`RowPlan::build_page`] attaches the facts after
    /// the cut, so a run with them and one without cut the same pages.
    pub(super) fn new(mut rows: Vec<Value>, tool: crate::id::ToolId) -> Self {
        let first_names = tool.first_fragment_fields();
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
                    let key = crate::json_pointer::unescape(name);
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
                        .map(crate::json_pointer::unescape)
                        .chain(std::iter::once(key))
                        .collect::<Vec<_>>();
                    insert_field(&mut fields, &segments, value);
                }
                fields
            })
            .collect::<Vec<_>>();
        // Each row is measured once more, without its first-fragment fields.
        let chars: Vec<usize> = rows
            .iter()
            .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0))
            .collect();
        let reserves = chars
            .iter()
            .zip(&before)
            .map(|(chars, before)| before.saturating_sub(*chars))
            .collect::<Vec<_>>();
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
        cancelled: &Cancellation,
    ) -> Result<RowPlan, DependencyError> {
        self.plan_with_terminal(
            PageBudget {
                rows: row_budget,
                terminal: None,
                envelope: 0,
            },
            cancelled,
        )
    }

    /// Only the final query row's tail may use the finished envelope allowance.
    pub(super) fn plan_with_terminal(
        self,
        budget: PageBudget,
        cancelled: &Cancellation,
    ) -> Result<RowPlan, DependencyError> {
        let PageBudget {
            rows: row_budget,
            terminal: terminal_budget,
            envelope,
        } = budget;
        let Self {
            rows,
            first_fields,
            reserves,
            sizes,
            tool,
        } = self;
        let mut arena = FragmentArena::new(tool);
        let page_context = arena.page_context;
        let mut pages = Pages::new(row_budget);
        let mut guidance = Vec::with_capacity(rows.len());
        let mut delivered = Vec::with_capacity(rows.len());
        let mut filled = Vec::with_capacity(rows.len());
        for (position, mut row) in rows.into_iter().enumerate() {
            // Each part carries only its share of the row's guidance
            // ([`PartShare`]): parts are packed without it, then sized with
            // the share they carry.
            let kept = [PartShare::Whole, PartShare::First, PartShare::Last]
                .map(|share| row_next_chars(&row, share));
            guidance.push(take_guidance(&mut row));
            let catalog = arena.take_catalog(&mut row, cancelled)?;
            delivered.push(delivered_collections(&row, &page_context.fixed_paths));
            filled.push(filled_collections(&row, &page_context.collection_routes));
            let extra = arena.dependencies(
                catalog,
                &Value::Object(first_fields[position].clone()),
                cancelled,
            )?;
            let extra_chars = arena.evidence_chars(&extra)?;
            let head = head_limit(sizes[position], pages.room(), reserves[position]);
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
                    envelope,
                },
                reserves[position],
                &extra,
                kept,
            )?;
            for (fragment, chars) in parts {
                pages.place(fragment, chars);
            }
        }
        let (placed, total_pages) = pages.finish();
        Ok(RowPlan {
            arena,
            first_fields,
            guidance,
            delivered,
            filled,
            placed,
            total_pages,
        })
    }
}

impl RowPlan {
    /// Build page `page`'s rows in row order, materializing only its
    /// fragments; part one regains its row's per-call provider facts, which
    /// no page edge counted.
    pub(super) fn build_page(
        &self,
        page: usize,
        volatile: &[Option<Map<String, Value>>],
        cancelled: &Cancellation,
    ) -> Result<Vec<Value>, DependencyError> {
        // Placement appends pages, rows and parts in order. Borrow this
        // page's range without scanning or sorting every other fragment.
        let start = self.placed.partition_point(|p| p.page < page);
        let end = self.placed.partition_point(|p| p.page <= page);
        self.placed[start..end]
            .iter()
            .map(|p| {
                if cancelled.is_cancelled() {
                    return Err(DependencyError::Cancelled);
                }
                let mut value = self.arena.materialize(&p.fragment);
                drop_delivered_elsewhere(
                    &mut value,
                    &self.delivered[p.row],
                    &self.arena.page_context.optional_paths,
                );
                if p.of > 1 {
                    let context = self.arena.page_context;
                    drop_filled_elsewhere(
                        &mut value,
                        &self.filled[p.row],
                        &context.collection_routes,
                        &|pointer| context.requires(pointer),
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
                keep_row_next(&mut value, PartShare::of(p.part, p.of));
                let mut marker = value
                    .as_object_mut()
                    .and_then(|row| row.shift_remove("rowPart"));
                let mut take = |key: &str| {
                    marker
                        .as_mut()
                        .and_then(|marker| marker.get_mut(key).map(Value::take))
                };
                let (continues, text) = (take("continues"), take("text"));
                if p.of > 1 {
                    value["rowPart"] = json!({"part": p.part, "of": p.of});
                    if let Some(continues) = continues {
                        value["rowPart"]["continues"] = continues;
                    }
                    if let Some(text) = text {
                        value["rowPart"]["text"] = text;
                    }
                }
                self.arena
                    .attach_evidence(&p.fragment, &mut value, cancelled)?;
                Ok(value)
            })
            .collect()
    }
}

/// Plan one row into its parts, each sized as it
/// serializes on a page: its comma and `rowPart`, the first-fragment fields
/// part one regains, less the continuations and hints the part does not carry
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
