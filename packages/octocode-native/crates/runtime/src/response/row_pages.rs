//! Row-aware response pages: every page is a complete envelope of whole
//! rows, or of parts of a row too large for one page. Oversized rows are
//! planned into fragments by size; only the requested page is built.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::ops::Range;

use super::pager::{ResponsePageOptions, ResponsePagination, restart_pagination};
use super::pages::Remaining;
use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};

/// Page-plan size reserved for a row's per-call facts. It depends only on
/// their shape, so executions that differ in the values plan identical pages.
fn volatile_reserve(fields: &Map<String, Value>) -> usize {
    fn shape(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, child)| (key.clone(), shape(child)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(shape).collect()),
            _ => json!("x".repeat(40)),
        }
    }
    let shaped: Map<String, Value> = fields
        .iter()
        .map(|(key, child)| (key.clone(), shape(child)))
        .collect();
    octocode_engine::json_size::utf16_units(&shaped).unwrap_or(0) - 1
}

#[cfg(test)]
thread_local! {
    /// Row fragments built by the current thread's pager (test sensor).
    static MATERIALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn note_materialized() {
    #[cfg(test)]
    MATERIALIZED.with(|count| count.set(count.get() + 1));
}

fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The largest evidence array with at least two elements reachable through
/// objects and single-element arrays. Executable calls remain indivisible.
fn largest_array(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let mut best: Option<(String, usize)> = None;
    let mut consider = |candidate: Option<(String, usize)>| {
        if let Some(candidate) = candidate
            && best.as_ref().is_none_or(|current| candidate.1 > current.1)
        {
            best = Some(candidate);
        }
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == PAGES_KEY || key == HINTS_KEY {
                    continue;
                }
                consider(largest_array(
                    child,
                    &format!("{pointer}/{}", escape_pointer(key)),
                ));
            }
        }
        Value::Array(items) if items.len() >= 2 => {
            consider(Some((
                pointer.to_owned(),
                octocode_engine::json_size::utf16_units(value).unwrap_or(0),
            )));
        }
        Value::Array(items) => {
            if let Some(item) = items.first() {
                consider(largest_array(item, &format!("{pointer}/0")));
            }
        }
        _ => {}
    }
    best
}

/// Empty every remaining array of `skeleton` large enough (over a quarter
/// of `budget`) to matter if repeated per fragment. Returns, when any was
/// emptied, the split's rest: the skeleton as it was, holding them and the
/// lists beside the paged array at `pointer`. The rest is the split's first
/// part, so its slices carry those lists emptied.
fn lean_skeleton(skeleton: &mut Value, budget: usize, pointer: &str) -> Option<Value> {
    let mut rest = None;
    while let Some((relative, chars)) = skeleton
        .get("data")
        .and_then(|data| largest_array(data, ""))
        && chars.saturating_mul(4) > budget
    {
        if rest.is_none() {
            rest = Some(skeleton.clone());
        }
        let path = format!("/data{relative}");
        match skeleton.pointer_mut(&path).and_then(Value::as_array_mut) {
            Some(items) => items.clear(),
            None => break,
        }
    }
    if rest.is_some() {
        empty_secondary_lists(skeleton, pointer);
    }
    rest
}

/// The outermost array element a paged array at `pointer` lies in (reached
/// through single-element arrays), as (array pointer, element index).
fn enclosing_element(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let mut prefix = String::new();
    let mut node = value;
    for segment in pointer.trim_start_matches('/').split('/') {
        let key = segment.replace("~1", "/").replace("~0", "~");
        if node.is_array() {
            return key.parse().ok().map(|index| (prefix, index));
        }
        node = node.as_object()?.get(&key)?;
        prefix = format!("{prefix}/{segment}");
    }
    None
}

/// Serialized length a `rowPart.continues` marker adds.
fn continues_chars(array: &str, index: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"continues": {"array": array, "index": index}}))
        .unwrap_or(0)
        - 1
}

/// Move the largest array out of `row`'s data, as (JSON pointer, items).
fn take_largest_array(row: &mut Value) -> Option<(String, Vec<Value>)> {
    let (relative, _) = row.get("data").and_then(|data| largest_array(data, ""))?;
    let pointer = format!("/data{relative}");
    let items = row
        .pointer_mut(&pointer)
        .and_then(Value::as_array_mut)
        .map(std::mem::take)?;
    Some((pointer, items))
}

/// Resolve declared collection paths without entering arbitrary answer values
/// or treating coordinate tuples and criteria as evidence lists.
fn collection_pointer(row: &Value, paths: &[&str]) -> Option<String> {
    fn visit(value: &Value, segments: &[&str], pointer: &str) -> Option<String> {
        let Some((segment, rest)) = segments.split_first() else {
            let len = match value {
                Value::Array(items) => items.len(),
                Value::Object(items) => items.len(),
                _ => 0,
            };
            return (len > 1).then(|| pointer.to_owned());
        };
        if *segment == "*" {
            return match value {
                Value::Object(map) => map.iter().find_map(|(key, child)| {
                    visit(child, rest, &format!("{pointer}/{}", escape_pointer(key)))
                }),
                Value::Array(items) if items.len() == 1 => {
                    visit(&items[0], rest, &format!("{pointer}/0"))
                }
                _ => None,
            };
        }
        let key = segment.replace("~1", "/").replace("~0", "~");
        let child = value.as_object()?.get(&key)?;
        visit(child, rest, &format!("{pointer}/{}", escape_pointer(&key)))
    }
    paths.iter().find_map(|path| {
        let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        visit(row, &segments, "")
    })
}

/// One row fragment, planned by size before anything is built. Oversized rows
/// are split into a skeleton (every field except the largest array, emptied)
/// plus that array's items; a fragment is a slice of those items. An element
/// that alone exceeds the budget recurses on a one-item row; an indivisible
/// element becomes one oversized fragment rather than being cut mid-value.
enum Fragment {
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
        inner: Box<Fragment>,
        split: usize,
        before: Range<usize>,
        after: Range<usize>,
    },
    /// `inner` is a later part of element `index` of the array at `array`,
    /// shown at position 0 of that array: its `rowPart.continues` marker.
    Continues {
        inner: Box<Fragment>,
        array: String,
        index: usize,
    },
}

/// A planned fragment and its serialized chars.
type Planned = (Fragment, usize);

/// Later fragments of an array element retain its original position.
fn mark_continuations(array: &str, at: usize, index: usize, out: &mut [Planned]) {
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

struct RowSplit {
    skeleton: Value,
    pointer: String,
    items: Vec<Value>,
}

struct RecordSplit {
    skeleton: Value,
    pointer: String,
    records: Vec<(String, Value)>,
}

#[derive(Default)]
struct FragmentArena {
    wholes: Vec<Value>,
    records: Vec<RecordSplit>,
    collection_paths: Option<&'static [&'static str]>,
    splits: Vec<RowSplit>,
    /// Fields an entry schema of the responding tool requires: a later
    /// slice keeps them on the elements it passes through.
    required: Option<&'static std::collections::BTreeSet<String>>,
}

impl FragmentArena {
    /// Plan `row` into `(fragment, serialized chars)`; `row` is consumed and
    /// its largest array moved, never cloned. The row's first fragment is
    /// bounded by `head`, every later one by `budget`.
    fn plan(&mut self, mut row: Value, head: usize, budget: usize, out: &mut Vec<Planned>) {
        let row_chars = octocode_engine::json_size::utf16_units(&row).unwrap_or(0);
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
    fn materialize(&self, fragment: &Fragment) -> Value {
        note_materialized();
        let later = self.later_pointers(fragment);
        let mut value = self.build(fragment);
        // Pruned once the fragment is whole: a list the skeleton emptied
        // may be the slot a joined split fills.
        if let Some(required) = self.required {
            for pointer in later {
                drop_emptied_lists(&mut value, &pointer, required);
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
                    if let Some(required) = self.required {
                        lean_elements(&mut value, &row_split.pointer, required);
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

/// The items of a split not yet in a fragment.
#[derive(Clone, Copy)]
enum Open {
    /// Items from `start` form a new slice of `chars` (skeleton included).
    Chunk { start: usize, chars: usize },
    /// Items from `start` join fragment `at`, the last part of a recursed
    /// element, now `chars`.
    Tail {
        at: usize,
        start: usize,
        chars: usize,
    },
}

/// Greedy slicing of one split's items: the row's first fragment is bounded
/// by `head`, every later one by `budget`.
struct Slicer {
    split: usize,
    enclosing: Option<(String, usize)>,
    /// The paged array's pointer.
    pointer: String,
    first: usize,
    head: usize,
    budget: usize,
    base: usize,
    open: Open,
}

impl Slicer {
    /// The bound of the fragment at position `at` of the plan.
    fn limit(&self, at: usize) -> usize {
        if at == self.first {
            self.head
        } else {
            self.budget
        }
    }

    /// Place item `index` of `item` serialized chars (its comma included).
    fn push(
        &mut self,
        arena: &mut FragmentArena,
        index: usize,
        item: usize,
        out: &mut Vec<Planned>,
    ) {
        if self.base + item > self.limit(out.len()) {
            self.oversized(arena, index, out);
            return;
        }
        self.open = match self.open {
            Open::Chunk { start, chars }
                if index > start && chars + item > self.limit(out.len()) =>
            {
                self.flush(start, index, chars, out);
                Open::Chunk {
                    start: index,
                    chars: self.base + item,
                }
            }
            Open::Chunk { start, chars } => Open::Chunk {
                start,
                chars: chars + item,
            },
            Open::Tail { at, start, chars } if chars + item > self.limit(at) => {
                self.close_tail(at, start..index, chars, out);
                Open::Chunk {
                    start: index,
                    chars: self.base + item,
                }
            }
            Open::Tail { at, start, chars } => Open::Tail {
                at,
                start,
                chars: chars + item,
            },
        };
    }

    /// Item `index` alone exceeds a fragment: it recurses on a one-item row.
    /// The open slice before it joins the element's first fragment when both
    /// fit the bound, and the items after it join its last fragment while
    /// they fit, so a seam never leaves a part short of its page.
    fn oversized(&mut self, arena: &mut FragmentArena, index: usize, out: &mut Vec<Planned>) {
        if let Open::Chunk { start, chars } = self.open
            && index > start
        {
            let pending = chars - self.base;
            let at = out.len();
            let limit = self.limit(at);
            let mut trial = Vec::new();
            let head = limit.saturating_sub(pending).max(1);
            arena.plan(self.single(arena, index, at), head, self.budget, &mut trial);
            if trial[0].1 + pending <= limit {
                out.extend(trial);
                mark_continuations(&self.pointer, at, index, out);
                self.join_before(at, start..index, pending, out);
            } else {
                // The trial's arena entries are never built.
                self.flush(start, index, chars, out);
                let at = out.len();
                arena.plan(
                    self.single(arena, index, at),
                    self.limit(at),
                    self.budget,
                    out,
                );
                mark_continuations(&self.pointer, at, index, out);
            }
        } else {
            self.close(index, out);
            let at = out.len();
            arena.plan(
                self.single(arena, index, at),
                self.limit(at),
                self.budget,
                out,
            );
            mark_continuations(&self.pointer, at, index, out);
        }
        let last = out.len() - 1;
        self.open = Open::Tail {
            at: last,
            start: index + 1,
            chars: out[last].1,
        };
    }

    /// Item `index` as a one-item row planned at position `at`; after the
    /// split's first fragment it carries the secondary lists emptied.
    fn single(&self, arena: &FragmentArena, index: usize, at: usize) -> Value {
        let row_split = &arena.splits[self.split];
        let mut single = row_split.skeleton.clone();
        if at != self.first {
            empty_secondary_lists(&mut single, &row_split.pointer);
        }
        if let Some(slot) = single.pointer_mut(&row_split.pointer) {
            *slot = Value::Array(vec![row_split.items[index].clone()]);
        }
        single
    }

    fn close(&mut self, end: usize, out: &mut Vec<Planned>) {
        match self.open {
            Open::Chunk { start, chars } => self.flush(start, end, chars, out),
            Open::Tail { at, start, chars } => self.close_tail(at, start..end, chars, out),
        }
        self.open = Open::Chunk {
            start: end,
            chars: self.base,
        };
    }

    /// Push the slice of items `start..end`. A slice after the split's
    /// first part continues the element its array lies in, if any.
    fn flush(&self, start: usize, end: usize, chars: usize, out: &mut Vec<Planned>) {
        if end <= start {
            return;
        }
        let split = self.split;
        let chunk = Fragment::Chunk { split, start, end };
        let element = (out.len() > self.first)
            .then(|| self.enclosing.clone())
            .flatten();
        out.push(match element {
            Some((array, index)) => {
                let size = chars - 1 + continues_chars(&array, index);
                let inner = Box::new(chunk);
                (
                    Fragment::Continues {
                        inner,
                        array,
                        index,
                    },
                    size,
                )
            }
            None => (chunk, chars - 1),
        });
    }

    /// Items `before` precede the one element part of fragment `at`.
    fn join_before(&self, at: usize, before: Range<usize>, chars: usize, out: &mut [Planned]) {
        let (fragment, size) = &mut out[at];
        let inner = std::mem::replace(fragment, Fragment::Whole(0));
        *fragment = Fragment::Joined {
            inner: Box::new(inner),
            split: self.split,
            before,
            after: 0..0,
        };
        *size += chars;
    }

    /// Items `after` follow the one element part of fragment `at`, now `chars`.
    fn close_tail(&self, at: usize, after: Range<usize>, chars: usize, out: &mut [Planned]) {
        if after.is_empty() {
            return;
        }
        let (fragment, size) = &mut out[at];
        match fragment {
            Fragment::Joined {
                split, after: slot, ..
            } if *split == self.split => *slot = after,
            _ => {
                let inner = std::mem::replace(fragment, Fragment::Whole(0));
                *fragment = Fragment::Joined {
                    inner: Box::new(inner),
                    split: self.split,
                    before: 0..0,
                    after,
                };
            }
        }
        *size = chars;
    }
}

/// A later slice of a split row repeats its skeleton; the lists of records
/// beside the paged array, at each level of its `pointer` (diagnostics
/// beside the paged files, a cycle's edges beside its files), were delivered
/// with the first part, so later slices carry them emptied: each record
/// once.
fn empty_secondary_lists(skeleton: &mut Value, pointer: &str) {
    fn empty_records(value: &mut Value) {
        match value {
            Value::Array(items) if items.iter().all(|item| item.is_object() || item.is_array()) => {
                items.clear();
            }
            Value::Object(map) => {
                for (name, child) in map {
                    if name != PAGES_KEY && name != HINTS_KEY {
                        empty_records(child);
                    }
                }
            }
            _ => {}
        }
    }
    let mut segments = pointer.trim_start_matches('/').split('/');
    if segments.next() != Some("data") {
        return;
    }
    let mut node = skeleton.get_mut("data");
    for segment in segments {
        let key = segment.replace("~1", "/").replace("~0", "~");
        let Some(current) = node else {
            return;
        };
        if let Some(map) = current.as_object_mut() {
            for (name, value) in map.iter_mut() {
                if *name == key || name == PAGES_KEY || name == HINTS_KEY {
                    continue;
                }
                // Records (objects, or tuples such as a runtime cycle's
                // members), including nested lists, not scalar descriptors
                // or executable continuation queries.
                empty_records(value);
            }
        }
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|at| items.get_mut(at)),
            _ => None,
        };
    }
}

/// A later slice of a split element (a pull request whose comments page)
/// names the element, not its description again: on each array element the
/// paged `pointer` passes through, its descriptive scalars (a body, a head
/// SHA) ride the first part only; the fields its entry schema requires
/// (`required`), its lists (emptied beside the paged one, and dropped by
/// [`drop_emptied_lists`] once the part is whole) and nested objects stay.
fn lean_elements(
    skeleton: &mut Value,
    pointer: &str,
    required: &std::collections::BTreeSet<String>,
) {
    let segments = pointer
        .trim_start_matches('/')
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let mut node = Some(skeleton);
    let mut in_element = false;
    for key in &segments {
        let Some(current) = node else {
            return;
        };
        if in_element && let Some(map) = current.as_object_mut() {
            map.retain(|name, value| {
                name == key
                    || name == PAGES_KEY
                    || name == HINTS_KEY
                    || required.contains(name)
                    || value.is_array()
                    || value.is_object()
            });
        }
        in_element = current.is_array();
        node = match current {
            Value::Object(map) => map.get_mut(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
    }
}

/// On a later slice, each array element the paged `pointer` passes through
/// drops its lists left empty (an emptied `reviews: []` says nothing) unless
/// its entry schema requires them; the paged array itself stays.
fn drop_emptied_lists(
    value: &mut Value,
    pointer: &str,
    required: &std::collections::BTreeSet<String>,
) {
    let segments = pointer
        .trim_start_matches('/')
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let mut node = Some(value);
    let mut in_element = false;
    for key in &segments {
        let Some(current) = node else {
            return;
        };
        if in_element && let Some(map) = current.as_object_mut() {
            map.retain(|name, value| {
                name == key
                    || required.contains(name)
                    || value.as_array().is_none_or(|items| !items.is_empty())
            });
        }
        in_element = current.is_array();
        node = match current {
            Value::Object(map) => map.get_mut(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
    }
}

/// Serialized length `rowPart` adds to a non-empty row object.
fn row_part_chars(part: usize, of: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"rowPart": {"part": part, "of": of}}))
        .unwrap_or(0)
        - 1
}

/// The part of a split row a `next.*` call rides. Page continuations
/// (`next*`, `continue*`) resume after the row's last shown evidence, so they
/// ride its last part: following one from an earlier part would silently
/// skip the parts between. Every other call (drill-down, handoff, recovery)
/// rides the first part, beside the head of the evidence. Middle parts carry
/// none, so each call appears exactly once across the part set.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PartShare {
    Whole,
    First,
    Middle,
    Last,
}

impl PartShare {
    fn of(part: usize, of: usize) -> Self {
        match (part, of) {
            (_, 1) => Self::Whole,
            (1, _) => Self::First,
            (part, of) if part == of => Self::Last,
            _ => Self::Middle,
        }
    }

    /// Whether this part keeps entry `name` of the row's `container`
    /// (`next` or `hints`). Hint leads and text are never pages, so they
    /// ride the first part.
    fn keeps(self, container: &str, name: &str) -> bool {
        let page = container == PAGES_KEY && crate::id::resumes_after_shown(name);
        match self {
            Self::Whole => true,
            Self::First => !page,
            Self::Middle => false,
            Self::Last => page,
        }
    }
}

/// The `next` and `hints` objects a row carries under `data` and at its top
/// level, with their container name.
fn row_nexts(row: &Value) -> impl Iterator<Item = (&'static str, &Map<String, Value>)> {
    [row.get("data"), Some(row)]
        .into_iter()
        .flatten()
        .flat_map(|slot| {
            [PAGES_KEY, HINTS_KEY].into_iter().filter_map(move |key| {
                slot.get(key)
                    .and_then(Value::as_object)
                    .map(|map| (key, map))
            })
        })
}

/// Serialized `"next":…,`/`"hints":…,` chars a row's `share` keeps.
fn row_next_chars(row: &Value, share: PartShare) -> usize {
    row_nexts(row)
        .map(|(container, entries)| {
            let kept: Map<String, Value> = entries
                .iter()
                .filter(|(name, _)| share.keeps(container, name))
                .map(|(name, call)| (name.clone(), call.clone()))
                .collect();
            if kept.is_empty() {
                0
            } else {
                octocode_engine::json_size::utf16_units(&json!({ container: kept })).unwrap_or(0)
                    - 1
            }
        })
        .sum()
}

/// Keep only the continuations and hints a row part rides (see
/// [`PartShare`]); the row's remaining-page `notes` are re-rendered for the
/// pages the part carries or moved.
fn keep_row_next(value: &mut Value, share: PartShare, notes: &[Remaining]) {
    let retain = |slot: Option<&mut Map<String, Value>>| {
        let Some(slot) = slot else {
            return;
        };
        let mut moved = Vec::new();
        for container in [PAGES_KEY, HINTS_KEY] {
            if let Some(entries) = slot.get_mut(container).and_then(Value::as_object_mut) {
                entries.retain(|name, _| {
                    let kept = share.keeps(container, name);
                    if !kept && container == PAGES_KEY {
                        moved.push(name.clone());
                    }
                    kept
                });
                if entries.is_empty() {
                    slot.remove(container);
                }
            }
        }
        // Only the first part holds the row's base data; its warning names
        // the pages that moved to the response pager.
        if share == PartShare::First {
            super::pages::redisclose(slot, notes, &moved);
        }
    };
    retain(value.get_mut("data").and_then(Value::as_object_mut));
    retain(value.as_object_mut());
}

/// Page-one share of `capacity` for rows of `sizes`: an equal split, where a
/// row smaller than its share keeps only its size and leaves the rest to the
/// larger rows.
pub(crate) fn fair_shares(sizes: &[usize], capacity: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&row| (sizes[row], row));
    let mut shares = vec![0; sizes.len()];
    let mut left = capacity;
    for (taken, &row) in order.iter().enumerate() {
        shares[row] = sizes[row].min(left / (sizes.len() - taken));
        left -= shares[row];
    }
    shares
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

/// Upper bound of the `incomplete — …` envelope warning and its key.
const INCOMPLETE_BANNER_CHARS: usize = 160;

/// One page of the [`RowSource`] of `structured`, with no room kept for the
/// page's `responsePagination` block (the plan tests measure rows alone).
#[cfg(test)]
pub(super) fn paginate_rows(
    structured: Map<String, Value>,
    full: &str,
    options: &ResponsePageOptions,
    volatile: Vec<Option<Map<String, Value>>>,
    remaining: &[Vec<Remaining>],
    tool: Option<crate::id::ToolId>,
) -> (Map<String, Value>, ResponsePagination) {
    RowSource::new(structured, full, options, volatile, tool, &|_| 0).page(
        options,
        remaining,
        &|_| 0,
    )
}

/// Row-aware pages: the plan that packs whole rows (or fragments of
/// oversized rows) of one envelope into complete envelopes, built once and
/// cut into the zero-based page `responseOffset` addresses. A saved response
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
    /// Plan `structured` (serialized as `full`) at the `responseLength` of
    /// `options`.
    pub(super) fn new(
        mut structured: Map<String, Value>,
        full: &str,
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
        let total = full.encode_utf16().count();
        let snapshot = format!(
            "response-rows-v6:{budget}:{}",
            octocode_engine::digest::sha256(full.as_bytes())
        );
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
        let plan_with = |reserve: usize, rows: Vec<Value>| {
            let row_budget = budget
                .saturating_sub(overhead + INCOMPLETE_BANNER_CHARS + reserve)
                .max(1);
            RowPlan::new(rows, &volatile, row_budget, tool)
        };
        let finished = block(&template(false));
        let widest = block(&template(true));
        let plan = if total + finished <= budget {
            let plan = plan_with(finished, rows.clone());
            // By size alone one page holds it; a plan that splits it anyway
            // carries the block of a page with more on every page.
            if plan.total_pages == 1 {
                plan
            } else {
                plan_with(widest, rows)
            }
        } else {
            plan_with(widest, rows)
        };
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

    /// The page `options` asks for (`responseOffset`), or a restart page
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
                "the response or page size changed, or its snapshot is missing; restart from responseOffset=0"
                    .to_owned()
            } else {
                format!(
                    "page {} of {} is past the last page; restart from responseOffset=0",
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
    let banner = Value::String(format!(
        "incomplete — {later} of {rows} rows continue on later response pages; follow responsePagination.next"
    ));
    let mut warnings = match structured.shift_remove("warnings") {
        Some(Value::Array(warnings)) => warnings,
        _ => Vec::new(),
    };
    warnings.push(banner);
    structured.shift_insert(0, "warnings".into(), Value::Array(warnings));
}

/// The page plan of a row-scoped response: every fragment of every row,
/// placed on a page by its size.
struct RowPlan {
    arena: FragmentArena,
    first_fields: Vec<Map<String, Value>>,
    placed: Vec<Placed>,
    total_pages: usize,
    rows: usize,
}

impl RowPlan {
    fn new(
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
    fn build_page(
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
    fn rows_after(&self, page: usize) -> usize {
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lazy_page_tests;
