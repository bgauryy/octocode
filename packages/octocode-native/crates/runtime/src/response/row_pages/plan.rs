use super::super::pages::Remaining;
use super::{
    FragmentArena, PartShare, Placed, fair_shares, keep_row_next, row_next_chars, row_part_chars,
    volatile_reserve,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// The page plan of a row-scoped response: every fragment of every row,
/// placed on a page by its size.
pub(super) struct RowPlan {
    arena: FragmentArena,
    first_fields: Vec<Map<String, Value>>,
    placed: Vec<Placed>,
    pub(super) total_pages: usize,
    pub(super) rows: usize,
}

impl RowPlan {
    pub(super) fn new(
        mut rows: Vec<Value>,
        volatile: &[Option<Map<String, Value>>],
        row_budget: usize,
        tool: Option<crate::id::ToolId>,
    ) -> Self {
        let first_names = tool.map_or(&[][..], |tool| tool.output().first_fragment_fields());
        let first_fields = rows
            .iter_mut()
            .map(|row| {
                first_names
                    .iter()
                    .filter_map(|name| {
                        row.as_object_mut()?
                            .shift_remove(*name)
                            .map(|value| ((*name).to_owned(), value))
                    })
                    .collect::<Map<_, _>>()
            })
            .collect::<Vec<_>>();
        let reserves: Vec<usize> = volatile
            .iter()
            .zip(&first_fields)
            .map(|(fields, first)| {
                fields.as_ref().map_or(0, volatile_reserve)
                    + if first.is_empty() {
                        0
                    } else {
                        octocode_engine::json_size::utf16_units(first).unwrap_or(0) - 1
                    }
            })
            .collect();
        let sizes: Vec<usize> = rows
            .iter()
            .zip(&reserves)
            .map(|(row, reserve)| {
                octocode_engine::json_size::utf16_units(row).unwrap_or(0) + 1 + reserve
            })
            .collect();
        let shares = fair_shares(&sizes, row_budget);
        let mut arena = FragmentArena {
            required: tool.map(crate::contracts::shared_fields::required_entry_fields),
            collection_paths: tool.and_then(|tool| tool.output().collection_paths()),
            ..FragmentArena::default()
        };
        let mut heads = Vec::with_capacity(rows.len());
        let mut tails = Vec::new();
        for (position, row) in rows.into_iter().enumerate() {
            let head = head_limit(sizes[position], shares[position], reserves[position]);
            let parts = plan_row(
                &mut arena,
                row,
                position,
                head,
                row_budget,
                reserves[position],
            );
            for placed in parts {
                if placed.0.part == 1 {
                    heads.push(placed);
                } else {
                    tails.push(placed);
                }
            }
        }
        let (placed, total_pages) = assign_pages(heads.into_iter().chain(tails), row_budget);
        Self {
            arena,
            first_fields,
            placed,
            total_pages,
            rows: sizes.len(),
        }
    }

    /// Build page `page`'s rows in row order, materializing only its
    /// fragments; part one regains its row's per-call provider facts.
    pub(super) fn build_page(
        &self,
        page: usize,
        volatile: &[Option<Map<String, Value>>],
        remaining: &[Vec<Remaining>],
    ) -> Vec<Value> {
        let mut parts: Vec<&Placed> = self.placed.iter().filter(|p| p.page == page).collect();
        parts.sort_by_key(|p| (p.row, p.part));
        parts
            .into_iter()
            .map(|p| {
                let mut value = self.arena.materialize(&p.fragment);
                if p.part == 1
                    && let Some(row) = value.as_object_mut()
                {
                    row.extend(self.first_fields[p.row].clone());
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
                value
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
            .saturating_sub(1 + reserve + row_part_chars(1, 1000))
            .max(1)
    }
}

/// Plan one row into its parts, each sized as it serializes on a page: its
/// comma and `rowPart`, the per-call facts part one regains, less the
/// continuations and hints the part does not carry ([`PartShare`]).
fn plan_row(
    arena: &mut FragmentArena,
    row: Value,
    position: usize,
    head: usize,
    row_budget: usize,
    reserve: usize,
) -> Vec<(Placed, usize)> {
    let next_chars = row_next_chars(&row, PartShare::Whole);
    let kept_next_chars = [PartShare::First, PartShare::Middle, PartShare::Last]
        .map(|share| row_next_chars(&row, share));
    let mut fragments = Vec::new();
    arena.plan(row, head, row_budget, &mut fragments);
    let of = fragments.len();
    fragments
        .into_iter()
        .enumerate()
        .map(|(index, (fragment, chars))| {
            let part = index + 1;
            let added = if part == 1 { reserve } else { 0 };
            let stripped = match PartShare::of(part, of) {
                PartShare::Whole => 0,
                PartShare::First => next_chars - kept_next_chars[0],
                PartShare::Middle => next_chars - kept_next_chars[1],
                PartShare::Last => next_chars - kept_next_chars[2],
            };
            let part_chars = if of > 1 { row_part_chars(part, of) } else { 0 };
            let placed = Placed {
                fragment,
                row: position,
                part,
                of,
                page: 0,
            };
            (
                placed,
                (chars + 1 + part_chars + added).saturating_sub(stripped),
            )
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
