use super::{
    Open, Slicer, collection_pointer, continues_chars, drop_emptied_lists, empty_secondary_lists,
    enclosing_element, lean_elements, lean_skeleton, note_materialized, take_largest_array,
};
use serde_json::{Map, Value, json};
use std::ops::Range;

/// One row fragment, planned by size before anything is built. Oversized rows
/// are split into a skeleton (every field except the largest array, emptied)
/// plus that array's items; a fragment is a slice of those items. An element
/// that alone exceeds the budget recurses on a one-item row; an indivisible
/// element becomes one oversized fragment rather than being cut mid-value.
pub(super) enum Fragment {
    Whole(usize),
    Records {
        split: usize,
        start: usize,
        end: usize,
    },
    Chunk {
        split: usize,
        start: usize,
        end: usize,
    },
    /// `inner`, whose array at the `split` pointer holds one part of an
    /// element (or nothing, for the split's rest), between that split's
    /// items `before` and `after`: a recursed element's neighbours ride its
    /// fragments instead of becoming parts of their own.
    Joined {
        inner: Box<Self>,
        split: usize,
        before: Range<usize>,
        after: Range<usize>,
    },
    /// `inner` is a later part of element `index` of the array at `array`,
    /// shown at position 0 of that array: its `rowPart.continues` marker.
    Continues {
        inner: Box<Self>,
        array: String,
        index: usize,
    },
}

/// A planned fragment and its serialized chars.
pub(super) type Planned = (Fragment, usize);

/// Later fragments of an array element retain its original position.
pub(super) fn mark_continuations(array: &str, at: usize, index: usize, out: &mut [Planned]) {
    for (fragment, size) in out.iter_mut().skip(at + 1) {
        let inner = std::mem::replace(fragment, Fragment::Whole(0));
        *fragment = Fragment::Continues {
            inner: Box::new(inner),
            array: array.to_owned(),
            index,
        };
        *size += continues_chars(array, index);
    }
}

pub(super) struct RowSplit {
    pub(super) skeleton: Value,
    pub(super) pointer: String,
    pub(super) items: Vec<Value>,
}

pub(super) struct RecordSplit {
    skeleton: Value,
    pointer: String,
    records: Vec<(String, Value)>,
}

#[derive(Default)]
pub(super) struct FragmentArena {
    pub(super) wholes: Vec<Value>,
    pub(super) records: Vec<RecordSplit>,
    pub(super) collection_paths: Option<&'static [&'static str]>,
    pub(super) splits: Vec<RowSplit>,
    /// Core-declared required and optional context for later entry fragments.
    pub(super) page_context: Option<&'static crate::contracts::output_fields::PageContextFields>,
}

impl FragmentArena {
    /// Plan `row` into `(fragment, serialized chars)`; `row` is consumed and
    /// its largest array moved, never cloned. The row's first fragment is
    /// bounded by `head`, every later one by `budget`.
    pub(super) fn plan(&mut self, row: Value, head: usize, budget: usize, out: &mut Vec<Planned>) {
        let row_chars = octocode_engine::json_size::utf16_units(&row).unwrap_or(0);
        self.plan_sized(row, row_chars, head, budget, out);
    }

    /// [`Self::plan`] of a row already sized: it serializes to `row_chars`.
    pub(super) fn plan_sized(
        &mut self,
        mut row: Value,
        row_chars: usize,
        head: usize,
        budget: usize,
        out: &mut Vec<Planned>,
    ) {
        let declared = (row_chars > head)
            .then(|| {
                self.collection_paths
                    .and_then(|paths| collection_pointer(&row, paths))
            })
            .flatten();
        if let Some(pointer) = &declared
            && let Some(records) = row.pointer_mut(pointer).and_then(Value::as_object_mut)
        {
            let records = std::mem::take(records);
            self.plan_records(row, pointer.clone(), records, head, budget, out);
            return;
        }
        let taken = if let Some(pointer) = declared {
            row.pointer_mut(&pointer)
                .and_then(Value::as_array_mut)
                .map(std::mem::take)
                .map(|items| (pointer, items))
        } else if row_chars > head && self.collection_paths.is_none() {
            take_largest_array(&mut row)
        } else {
            None
        };
        let Some((pointer, items)) = taken else {
            out.push((Fragment::Whole(self.wholes.len()), row_chars));
            self.wholes.push(row);
            return;
        };
        // Another large array would repeat in every slice of this one: slices
        // carry it emptied, and the row minus this array follows as its own
        // fragments, so each item is delivered exactly once.
        let rest = lean_skeleton(&mut row, budget, &pointer);
        // The rest (the row's other lists and fields) is the split's first
        // part, so the actionable records lead and its slices follow.
        let first = out.len();
        if let Some(rest) = rest {
            self.plan(rest, head, budget, out);
        }
        let base = octocode_engine::json_size::utf16_units(&row).unwrap_or(0);
        // Every fragment repeats the skeleton (stats, pagination, ...). When
        // the skeleton nearly fills the budget, one item per fragment would
        // multiply the payload; let each fragment carry at least as much
        // evidence as skeleton, trading a looser page bound for far fewer
        // near-duplicate pages.
        let budget = budget.max(base.saturating_mul(2));
        let head = head.max(base.saturating_mul(2)).min(budget);
        let item_chars: Vec<usize> = items
            .iter()
            .map(|value| octocode_engine::json_size::utf16_units(value).unwrap_or(0))
            .collect();
        let open = match out.len() {
            // Items join the rest's last part while they fit.
            planned if planned > first => Open::Tail {
                at: planned - 1,
                start: 0,
                chars: out[planned - 1].1,
            },
            _ => Open::Chunk {
                start: 0,
                chars: base,
            },
        };
        let mut slicer = Slicer {
            split: self.splits.len(),
            enclosing: enclosing_element(&row, &pointer),
            pointer: pointer.clone(),
            first,
            head,
            budget,
            base,
            open,
        };
        self.splits.push(RowSplit {
            skeleton: row,
            pointer,
            items,
        });
        for (index, chars) in item_chars.iter().enumerate() {
            slicer.push(self, index, chars + 1, out);
        }
        slicer.close(item_chars.len(), out);
    }

    fn plan_records(
        &mut self,
        row: Value,
        pointer: String,
        records: Map<String, Value>,
        head: usize,
        budget: usize,
        out: &mut Vec<Planned>,
    ) {
        let base = octocode_engine::json_size::utf16_units(&row).unwrap_or(0);
        let budget = budget.max(base.saturating_mul(2));
        let head = head.max(base.saturating_mul(2)).min(budget);
        let enclosing = enclosing_element(&row, &pointer);
        let first = out.len();
        let split = self.records.len();
        let mut start = 0;
        let mut size = base;
        let mut limit = head;
        let records: Vec<_> = records.into_iter().collect();
        let sizes = records
            .iter()
            .map(|(key, value)| {
                octocode_engine::json_size::utf16_units(key).unwrap_or(0)
                    + 1
                    + octocode_engine::json_size::utf16_units(value).unwrap_or(0)
            })
            .collect::<Vec<_>>();
        self.records.push(RecordSplit {
            skeleton: row,
            pointer,
            records,
        });
        for (index, chars) in sizes.iter().copied().enumerate() {
            let comma = usize::from(index > start);
            if index > start && size + comma + chars > limit {
                out.push((
                    Fragment::Records {
                        split,
                        start,
                        end: index,
                    },
                    size,
                ));
                start = index;
                size = base;
                limit = budget;
            }
            if base + chars > limit {
                let record = &mut self.records[split];
                let (key, value) = &mut record.records[index];
                let mut single = record.skeleton.clone();
                if let Some(slot) = single.pointer_mut(&record.pointer) {
                    *slot = Value::Object(Map::from_iter([(key.clone(), std::mem::take(value))]));
                }
                self.plan(single, limit, budget, out);
                start = index + 1;
                size = base;
                limit = budget;
                continue;
            }
            size += usize::from(index > start) + chars;
        }
        if start < sizes.len() {
            out.push((
                Fragment::Records {
                    split,
                    start,
                    end: sizes.len(),
                },
                size,
            ));
        }
        if let Some((array, index)) = enclosing {
            mark_continuations(&array, first, index, out);
        }
    }

    /// Build one planned fragment from a copy of its data: a saved plan
    /// cuts any page again.
    pub(super) fn materialize(&self, fragment: &Fragment) -> Value {
        note_materialized();
        let later = self.later_pointers(fragment);
        let mut value = self.build(fragment);
        // Pruned once the fragment is whole: a list the skeleton emptied
        // may be the slot a joined split fills.
        if let Some(context) = self.page_context {
            for pointer in later {
                drop_emptied_lists(&mut value, &pointer, &context.required);
            }
        }
        value
    }

    /// The paged pointers of `fragment`'s later slices (a chunk past its
    /// split's first item).
    fn later_pointers(&self, fragment: &Fragment) -> Vec<String> {
        match fragment {
            Fragment::Whole(_) => Vec::new(),
            Fragment::Records { split, start, .. } => (*start > 0)
                .then(|| self.records[*split].pointer.clone())
                .into_iter()
                .collect(),
            Fragment::Chunk { split, start, .. } => (*start > 0)
                .then(|| self.splits[*split].pointer.clone())
                .into_iter()
                .collect(),
            Fragment::Joined { inner, .. } | Fragment::Continues { inner, .. } => {
                self.later_pointers(inner)
            }
        }
    }

    fn build(&self, fragment: &Fragment) -> Value {
        match fragment {
            Fragment::Whole(index) => self.wholes[*index].clone(),
            Fragment::Records { split, start, end } => {
                let split = &self.records[*split];
                let mut row = split.skeleton.clone();
                if let Some(slot) = row.pointer_mut(&split.pointer) {
                    *slot = Value::Object(split.records[*start..*end].iter().cloned().collect());
                }
                row
            }
            Fragment::Chunk { split, start, end } => {
                let row_split = &self.splits[*split];
                let items = row_split.items[*start..*end].to_vec();
                let mut value = row_split.skeleton.clone();
                if *start > 0 {
                    empty_secondary_lists(&mut value, &row_split.pointer);
                    if let Some(context) = self.page_context {
                        lean_elements(&mut value, &row_split.pointer, context);
                    }
                }
                if let Some(slot) = value.pointer_mut(&row_split.pointer) {
                    *slot = Value::Array(items);
                }
                value
            }
            Fragment::Joined {
                inner,
                split,
                before,
                after,
            } => {
                let mut value = self.build(inner);
                let RowSplit { pointer, items, .. } = &self.splits[*split];
                if let Some(Value::Array(slot)) = value.pointer_mut(pointer) {
                    let middle = std::mem::take(slot);
                    *slot = items[before.clone()]
                        .iter()
                        .cloned()
                        .chain(middle)
                        .chain(items[after.clone()].iter().cloned())
                        .collect();
                }
                value
            }
            Fragment::Continues {
                inner,
                array,
                index,
            } => {
                let mut value = self.build(inner);
                // The outermost element wins over a nested one's marker.
                value["rowPart"] = json!({"continues": {"array": array, "index": index}});
                value
            }
        }
    }
}
