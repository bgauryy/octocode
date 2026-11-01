use super::super::result::Remaining;
use super::arena::Window;
use super::evidence::DependencyError;
use super::{
    FragmentArena, PartShare, Placed, delivered_collections, drop_delivered_elsewhere,
    keep_row_next, row_next_chars, row_part_chars, volatile_reserve,
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

mod fields;
mod pages;
use fields::{insert_field, restore_fields, take_guidance};
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
    placed: Vec<Placed>,
    pub(super) total_pages: usize,
    /// Each row's query index ([`query_indexes`]).
    queries: Vec<usize>,
}

/// The query indexes of the rows a page leaves for later pages, in row
/// order: rows none of whose parts is on the page or an earlier one, and
/// rows split across it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Later {
    pub(super) unserved: Vec<usize>,
    pub(super) continued: Vec<usize>,
}

impl Later {
    pub(super) fn is_empty(&self) -> bool {
        self.unserved.is_empty() && self.continued.is_empty()
    }
}

/// Each row's query index: the `index` it carries, else its position.
pub(super) fn query_indexes(rows: &[Value]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .map(|(position, row)| {
            row.get("index")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(position)
        })
        .collect()
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
    volatile: Vec<Option<Map<String, Value>>>,
    queries: Vec<usize>,
}

impl PreparedRows {
    pub(super) fn new(
        mut rows: Vec<Value>,
        volatile: &[Option<Map<String, Value>>],
        tool: crate::id::ToolId,
    ) -> Self {
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
        // Each row is measured once more, without its first-fragment fields.
        let chars: Vec<usize> = rows
            .iter()
            .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0))
            .collect();
        let reserves = chars
            .iter()
            .enumerate()
            .map(|(index, chars)| {
                before[index].saturating_sub(*chars)
                    + volatile
                        .get(index)
                        .and_then(Option::as_ref)
                        .map_or(0, volatile_reserve)
            })
            .collect::<Vec<_>>();
        let sizes = chars
            .iter()
            .zip(&reserves)
            .map(|(chars, reserve)| chars + 1 + reserve)
            .collect();
        Self {
            queries: query_indexes(&rows),
            rows,
            first_fields,
            reserves,
            sizes,
            tool,
            volatile: volatile.to_vec(),
        }
    }

    /// Each row's query index.
    pub(super) fn queries(&self) -> &[usize] {
        &self.queries
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
            queries,
        } = self;
        let mut arena = FragmentArena::new(tool);
        let page_context = arena.page_context;
        let mut pages = Pages::new(row_budget);
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
            delivered.push(delivered_collections(&row, &page_context.fixed_paths));
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
            placed,
            total_pages,
            queries,
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
                drop_delivered_elsewhere(
                    &mut value,
                    &self.delivered[p.row],
                    &self.arena.page_context.optional_paths,
                );
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

    /// The rows with a part on a page after `page`, by query index: unserved
    /// when no part is on `page` or before, else continued.
    pub(super) fn later(&self, page: usize) -> Later {
        let rows = self.queries.len();
        let (mut started, mut pending) = (vec![false; rows], vec![false; rows]);
        for placed in &self.placed {
            if placed.page > page {
                pending[placed.row] = true;
            } else {
                started[placed.row] = true;
            }
        }
        let mut later = Later::default();
        for (row, query) in self.queries.iter().enumerate() {
            if !pending[row] {
                continue;
            }
            if started[row] {
                later.continued.push(*query);
            } else {
                later.unserved.push(*query);
            }
        }
        later
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
