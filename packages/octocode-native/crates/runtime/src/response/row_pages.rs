//! Row-aware response pages: every page is a complete envelope of whole
//! rows, or of parts of a row too large for one page. Oversized rows are
//! planned into fragments by size; only the requested page is built.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ops::Range;

use super::channels::{HINTS_KEY, PAGES_KEY};
use super::pager::{ResponsePageOptions, ResponsePagination, restart_pagination};
use crate::tools::stream_page::json_chars;

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
    json_chars(&shape(&Value::Object(fields.clone()))) - 1
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

/// The largest array with at least two elements reachable from `value`
/// through objects and single-element arrays, as (JSON pointer, size).
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
                consider(largest_array(
                    child,
                    &format!("{pointer}/{}", escape_pointer(key)),
                ));
            }
        }
        Value::Array(items) if items.len() >= 2 => {
            consider(Some((pointer.to_owned(), json_chars(value))));
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
fn enclosing_element(pointer: &str) -> Option<(String, usize)> {
    let mut prefix = String::new();
    for segment in pointer.trim_start_matches('/').split('/') {
        if let Ok(index) = segment.parse::<usize>() {
            return Some((prefix, index));
        }
        prefix = format!("{prefix}/{segment}");
    }
    None
}

/// Serialized length a `rowPart.continues` marker adds.
fn continues_chars(array: &str, index: usize) -> usize {
    json_chars(&json!({"continues": {"array": array, "index": index}})) - 1
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

/// One row fragment, planned by size before anything is built. Oversized rows
/// are split into a skeleton (every field except the largest array, emptied)
/// plus that array's items; a fragment is a slice of those items. An element
/// that alone exceeds the budget recurses on a one-item row; an indivisible
/// element becomes one oversized fragment rather than being cut mid-value.
enum Fragment {
    Whole(usize),
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

struct RowSplit {
    skeleton: Value,
    pointer: String,
    items: Vec<Value>,
}

#[derive(Default)]
struct FragmentArena {
    wholes: Vec<Value>,
    splits: Vec<RowSplit>,
}

impl FragmentArena {
    /// Plan `row` into `(fragment, serialized chars)`; `row` is consumed and
    /// its largest array moved, never cloned. The row's first fragment is
    /// bounded by `head`, every later one by `budget`.
    fn plan(&mut self, mut row: Value, head: usize, budget: usize, out: &mut Vec<Planned>) {
        let row_chars = json_chars(&row);
        let taken = (row_chars > head)
            .then(|| take_largest_array(&mut row))
            .flatten();
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
        let base = json_chars(&row);
        // Every fragment repeats the skeleton (stats, pagination, ...). When
        // the skeleton nearly fills the budget, one item per fragment would
        // multiply the payload; let each fragment carry at least as much
        // evidence as skeleton, trading a looser page bound for far fewer
        // near-duplicate pages.
        let budget = budget.max(base.saturating_mul(2));
        let head = head.max(base.saturating_mul(2)).min(budget);
        let item_chars: Vec<usize> = items.iter().map(json_chars).collect();
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

    /// Build one planned fragment, moving its data out of the arena.
    fn materialize(&mut self, fragment: &Fragment) -> Value {
        note_materialized();
        self.build(fragment)
    }

    fn build(&mut self, fragment: &Fragment) -> Value {
        match fragment {
            Fragment::Whole(index) => std::mem::take(&mut self.wholes[*index]),
            Fragment::Chunk { split, start, end } => {
                let row_split = &mut self.splits[*split];
                let items = row_split.items[*start..*end]
                    .iter_mut()
                    .map(std::mem::take)
                    .collect();
                let mut value = row_split.skeleton.clone();
                if *start > 0 {
                    empty_secondary_lists(&mut value, &row_split.pointer);
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
                let RowSplit { pointer, items, .. } = &mut self.splits[*split];
                if let Some(Value::Array(slot)) = value.pointer_mut(pointer) {
                    let middle = std::mem::take(slot);
                    let mut take = |range: &Range<usize>| {
                        items[range.clone()]
                            .iter_mut()
                            .map(std::mem::take)
                            .collect::<Vec<_>>()
                    };
                    let before = take(before);
                    let after = take(after);
                    *slot = before.into_iter().chain(middle).chain(after).collect();
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
                self.mark_continuations(arena, at, index, out);
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
                self.mark_continuations(arena, at, index, out);
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
            self.mark_continuations(arena, at, index, out);
        }
        let last = out.len() - 1;
        self.open = Open::Tail {
            at: last,
            start: index + 1,
            chars: out[last].1,
        };
    }

    /// Every part of item `index` after its first (planned from `at`)
    /// continues that element of this split's array.
    fn mark_continuations(
        &self,
        arena: &FragmentArena,
        at: usize,
        index: usize,
        out: &mut [Planned],
    ) {
        let array = &arena.splits[self.split].pointer;
        for (fragment, size) in out.iter_mut().skip(at + 1) {
            let inner = std::mem::replace(fragment, Fragment::Whole(0));
            *fragment = Fragment::Continues {
                inner: Box::new(inner),
                array: array.clone(),
                index,
            };
            *size += continues_chars(array, index);
        }
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
            .then(|| enclosing_element(&self.pointer))
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
                if *name == key {
                    continue;
                }
                // Records (objects, or tuples such as a runtime cycle's
                // members), not descriptor lists of scalars.
                if let Some(items) = value.as_array_mut()
                    && items.iter().all(|item| item.is_object() || item.is_array())
                {
                    items.clear();
                }
            }
        }
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|at| items.get_mut(at)),
            _ => None,
        };
    }
}

/// Serialized length `rowPart` adds to a non-empty row object.
fn row_part_chars(part: usize, of: usize) -> usize {
    json_chars(&json!({"rowPart": {"part": part, "of": of}})) - 1
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
        let page = container == PAGES_KEY && crate::tools::id::resumes_after_shown(name);
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
                json_chars(&json!({ container: kept })) - 1
            }
        })
        .sum()
}

/// Keep only the continuations and hints a row part rides (see [`PartShare`]).
fn keep_row_next(value: &mut Value, share: PartShare) {
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
        super::pages::retarget_moved_pages(slot, &moved);
        if share == PartShare::Last {
            super::pages::disclose_carried_pages(slot);
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

/// Row-aware pages: pack whole rows (or fragments of oversized rows) into
/// complete envelopes. `responseOffset` addresses the zero-based page.
/// The first page gives every row a fair share of the budget, so each row
/// shows its head (with its drill-downs and handoffs) before any row
/// continues; the rest of each row follows in row order, and a split row's
/// page continuations ride its last part ([`PartShare`]). Pages are assigned
/// from fragment sizes; only the requested page is built.
pub(super) fn paginate_rows(
    mut structured: Map<String, Value>,
    full: &str,
    options: &ResponsePageOptions,
    mut volatile: Vec<Option<Map<String, Value>>>,
) -> (Map<String, Value>, ResponsePagination) {
    let budget = options.response_length.unwrap_or(1).max(1);
    let rows = match structured.remove("results") {
        Some(Value::Array(rows)) => rows,
        _ => Vec::new(),
    };
    volatile.resize(rows.len(), None);
    let overhead = json_chars(&Value::Object(structured.clone())) + "\"results\":[],".len();
    // Room for the envelope warning a page with rows left over leads with.
    let row_budget = budget
        .saturating_sub(overhead + INCOMPLETE_BANNER_CHARS)
        .max(1);
    let mut plan = RowPlan::new(rows, &volatile, row_budget);
    let total = full.encode_utf16().count();
    let snapshot = format!(
        "response-rows-v2:{}",
        hex::encode(Sha256::digest(full.as_bytes()))
    );
    let requested = options.response_offset.unwrap_or(0);
    let changed = options.response_snapshot.as_deref() != Some(&snapshot);
    if requested > 0 && (changed || requested >= plan.total_pages) {
        structured.insert("results".into(), Value::Array(Vec::new()));
        let pagination = restart_pagination(
            "rows",
            requested,
            plan.total_pages,
            total,
            snapshot,
            options,
            changed,
        );
        return (structured, pagination);
    }
    let selected = plan.build_page(requested, &mut volatile);
    let later = plan.rows_after(requested);
    structured.insert("results".into(), Value::Array(selected));
    if later > 0 {
        lead_with_incomplete_banner(&mut structured, later, plan.rows);
    }
    let has_more = requested + 1 < plan.total_pages;
    let char_length = json_chars(&Value::Object(structured.clone()));
    (
        structured,
        ResponsePagination {
            scope: "rows".into(),
            current_page: requested + 1,
            total_pages: plan.total_pages,
            has_more,
            char_offset: requested,
            char_length,
            total_chars: total,
            snapshot,
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(requested + 1),
            oversized: (char_length > budget).then_some(true),
            next: None,
        },
    )
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
    placed: Vec<Placed>,
    total_pages: usize,
    rows: usize,
}

impl RowPlan {
    fn new(rows: Vec<Value>, volatile: &[Option<Map<String, Value>>], row_budget: usize) -> Self {
        let reserves: Vec<usize> = volatile
            .iter()
            .map(|fields| fields.as_ref().map_or(0, volatile_reserve))
            .collect();
        let sizes: Vec<usize> = rows
            .iter()
            .zip(&reserves)
            .map(|(row, reserve)| json_chars(row) + 1 + reserve)
            .collect();
        let shares = fair_shares(&sizes, row_budget);
        let mut arena = FragmentArena::default();
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
            placed,
            total_pages,
            rows: sizes.len(),
        }
    }

    /// Build page `page`'s rows in row order, materializing only its
    /// fragments; part one regains its row's per-call provider facts.
    fn build_page(
        &mut self,
        page: usize,
        volatile: &mut [Option<Map<String, Value>>],
    ) -> Vec<Value> {
        let mut parts: Vec<&Placed> = self.placed.iter().filter(|p| p.page == page).collect();
        parts.sort_by_key(|p| (p.row, p.part));
        parts
            .into_iter()
            .map(|p| {
                let mut value = self.arena.materialize(&p.fragment);
                if p.part == 1
                    && let (Some(fields), Some(data)) = (
                        volatile[p.row].take(),
                        value.get_mut("data").and_then(Value::as_object_mut),
                    )
                {
                    data.extend(fields);
                }
                keep_row_next(&mut value, PartShare::of(p.part, p.of));
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
mod tests {
    use super::*;

    #[test]
    fn rows_scope_splits_nested_arrays_into_complete_json_pages() {
        let matches = (0..200)
            .map(|line| json!({"line":line,"value":"x".repeat(40)}))
            .collect::<Vec<_>>();
        let structured = json!({"results":[{"index":0,"data":{
            "stats":{"total":400},
            "files":[{"path":"a","matches":matches.clone()},{"path":"b","matches":matches}]
        }}]});
        let options = ResponsePageOptions {
            response_length: Some(2000),
            response_scope: Some("rows".into()),
            ..Default::default()
        };
        let full = structured.to_string();
        let (first, pagination) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &full,
            &options,
            Vec::new(),
        );
        assert!(pagination.total_pages > 2 && pagination.has_more);
        let row = &first["results"][0];
        assert_eq!(
            row["data"]["stats"]["total"], 400,
            "non-split fields are kept"
        );
        assert_eq!(row["data"]["files"].as_array().unwrap().len(), 1);
        assert!(json_chars(&Value::Object(first.clone())) <= 2000 + 200);
        let mut seen = 0;
        for page in 0..pagination.total_pages {
            let options = ResponsePageOptions {
                response_offset: Some(page),
                response_snapshot: Some(pagination.snapshot.clone()),
                ..options.clone()
            };
            let (envelope, _) = paginate_rows(
                structured.as_object().unwrap().clone(),
                &full,
                &options,
                Vec::new(),
            );
            for row in envelope["results"].as_array().unwrap() {
                for file in row["data"]["files"].as_array().unwrap() {
                    seen += file["matches"].as_array().unwrap().len();
                }
            }
        }
        assert_eq!(seen, 400, "every match appears exactly once");
        let stale = ResponsePageOptions {
            response_offset: Some(1),
            response_snapshot: Some("response-rows-v1:stale".into()),
            ..options
        };
        let (_, restart) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &full,
            &stale,
            Vec::new(),
        );
        assert_eq!(restart.restart, Some(true));
        assert_eq!(restart.next_char_offset, Some(0));
    }

    /// D4: a row with two large arrays (a comparison's commits and files)
    /// delivers each array once; slices of one never repeat the other.
    #[test]
    fn rows_scope_never_repeats_a_second_large_array_per_fragment() {
        let commits = (0..100)
            .map(|n| json!({"sha":format!("{n:040}"),"messageHeadline":"x".repeat(40)}))
            .collect::<Vec<_>>();
        let files = (0..100)
            .map(|n| format!("M +1 -1 src/file_{n:03}.rs"))
            .collect::<Vec<_>>();
        let structured = json!({"results":[{"index":0,"data":{
            "type":"compare","totalCommits":100,"commits":commits,"files":files
        }}]});
        let options = ResponsePageOptions {
            response_length: Some(6_000),
            response_scope: Some("rows".into()),
            ..Default::default()
        };
        let full = structured.to_string();
        let (_, first) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &full,
            &options,
            Vec::new(),
        );
        let (mut commits_seen, mut files_seen, mut chars) = (0, 0, 0);
        for page in 0..first.total_pages {
            let options = ResponsePageOptions {
                response_offset: Some(page),
                response_snapshot: Some(first.snapshot.clone()),
                ..options.clone()
            };
            let (envelope, _) = paginate_rows(
                structured.as_object().unwrap().clone(),
                &full,
                &options,
                Vec::new(),
            );
            chars += json_chars(&Value::Object(envelope.clone()));
            for row in envelope["results"].as_array().unwrap() {
                commits_seen += row["data"]["commits"].as_array().map_or(0, Vec::len);
                files_seen += row["data"]["files"].as_array().map_or(0, Vec::len);
            }
        }
        assert_eq!((commits_seen, files_seen), (100, 100));
        assert!(
            chars < full.len() + full.len() / 4,
            "{chars} vs {}",
            full.len()
        );
    }

    /// A skeleton (stats, pagination) that nearly fills the budget must not
    /// explode one row into one fragment per match, each repeating it.
    #[test]
    fn rows_scope_does_not_repeat_a_dominant_skeleton_per_item() {
        let matches = (0..30)
            .map(|line| json!({"line":line,"value":"x".repeat(20)}))
            .collect::<Vec<_>>();
        let structured = json!({"results":[{"index":0,"data":{
            "stats":{"note":"s".repeat(300)},
            "files":[{"path":"a","matches":matches}]
        }}]});
        let options = ResponsePageOptions {
            response_length: Some(600),
            response_scope: Some("rows".into()),
            ..Default::default()
        };
        let full = structured.to_string();
        let (first, pagination) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &full,
            &options,
            Vec::new(),
        );
        let items = first["results"][0]["data"]["files"][0]["matches"]
            .as_array()
            .map_or(0, Vec::len);
        assert!(items > 1, "each fragment carries several items: {first:?}");
        assert!(pagination.total_pages < 15, "{}", pagination.total_pages);
    }
}

#[cfg(test)]
mod lazy_page_tests {
    use super::*;

    fn envelope() -> Map<String, Value> {
        let matches = (0..400)
            .map(|line| json!({"line": line, "value": format!("fn item_{line}() {{ \u{1F600} }}")}))
            .collect::<Vec<_>>();
        let huge = json!({"line": 9999, "value": "x".repeat(900)});
        json!({"root": "/repo", "results": [
            {"index": 0, "data": {"files": [{"path": "a.rs", "matches": matches.clone()},
                                            {"path": "b.rs", "matches": [huge]}],
                                  "next": {"sidecar": {"tool": "localSearch",
                                                          "query": {"page": 2}}}}},
            {"index": 1, "data": {"content": "small"}},
            {"index": 2, "data": {"files": [{"path": "c.rs", "matches": matches}]}}
        ]})
        .as_object()
        .cloned()
        .expect("object")
    }

    fn options(length: usize, offset: usize, snapshot: Option<String>) -> ResponsePageOptions {
        ResponsePageOptions {
            response_length: Some(length),
            response_offset: Some(offset),
            response_snapshot: snapshot,
            response_scope: Some("rows".into()),
            render_text: None,
        }
    }

    /// Every page of `structured` at `length`, following the continuation.
    fn walk(
        structured: &Map<String, Value>,
        length: usize,
    ) -> Vec<(Map<String, Value>, ResponsePagination)> {
        let full = Value::Object(structured.clone()).to_string();
        let (_, first) = paginate_rows(
            structured.clone(),
            &full,
            &options(length, 0, None),
            Vec::new(),
        );
        (0..first.total_pages)
            .map(|offset| {
                let request = options(length, offset, Some(first.snapshot.clone()));
                let page = paginate_rows(structured.clone(), &full, &request, Vec::new());
                assert!(page.1.restart.is_none(), "length {length} page {offset}");
                page
            })
            .collect()
    }

    /// (row index, file path, line) of every delivered match.
    fn delivered(pages: &[(Map<String, Value>, ResponsePagination)]) -> Vec<(u64, String, u64)> {
        let mut seen = Vec::new();
        for (page, _) in pages {
            for row in page["results"].as_array().expect("rows") {
                for file in row["data"]["files"].as_array().into_iter().flatten() {
                    for item in file["matches"].as_array().into_iter().flatten() {
                        seen.push((
                            row["index"].as_u64().expect("index"),
                            file["path"].as_str().expect("path").to_owned(),
                            item["line"].as_u64().expect("line"),
                        ));
                    }
                }
            }
        }
        seen.sort();
        seen
    }

    #[test]
    fn every_row_is_delivered_exactly_once_within_the_page_bound() {
        let structured = envelope();
        let mut expected: Vec<(u64, String, u64)> = (0..400)
            .flat_map(|line| [(0, "a.rs".to_owned(), line), (2, "c.rs".to_owned(), line)])
            .chain([(0, "b.rs".to_owned(), 9999)])
            .collect();
        expected.sort();
        for length in [300, 700, 2_000, 9_000, 200_000] {
            let pages = walk(&structured, length);
            assert_eq!(delivered(&pages), expected, "length {length}");
            let mut parts: std::collections::BTreeMap<u64, Vec<(u64, u64)>> = Default::default();
            let mut small = 0;
            for (page, pagination) in &pages {
                assert!(
                    pagination.char_length <= length || pagination.oversized == Some(true),
                    "length {length}: {} chars unflagged",
                    pagination.char_length
                );
                for row in page["results"].as_array().expect("rows") {
                    small += usize::from(row["data"]["content"] == "small");
                    let index = row["index"].as_u64().expect("index");
                    let part = (
                        row["rowPart"]["part"].as_u64().unwrap_or(1),
                        row["rowPart"]["of"].as_u64().unwrap_or(1),
                    );
                    parts.entry(index).or_default().push(part);
                }
            }
            assert_eq!(small, 1, "length {length}");
            for (index, seen) in parts {
                let of = seen[0].1;
                assert_eq!(
                    seen,
                    (1..=of).map(|part| (part, of)).collect::<Vec<_>>(),
                    "length {length} row {index}"
                );
            }
        }
    }

    /// B2: one broad row must not fill the first pages alone; every row of a
    /// batch shows its head on page one, within the budget. Each split row's
    /// page continuation waits for its last part.
    #[test]
    fn a_page_with_rows_left_over_leads_with_an_incomplete_warning() {
        let rows = (0..3)
            .map(|index| json!({"index": index, "data": {"content": "x".repeat(9_000)}}))
            .collect::<Vec<_>>();
        let structured = json!({"results": rows})
            .as_object()
            .cloned()
            .expect("object");
        let pages = walk(&structured, 12_000);
        assert!(pages.len() > 1);
        let (first, _) = &pages[0];
        assert_eq!(first.keys().next().map(String::as_str), Some("warnings"));
        let banner = first["warnings"][0].as_str().expect("banner");
        assert!(
            banner.starts_with("incomplete — 2 of 3 rows continue"),
            "{banner}"
        );
        assert!(banner.contains("responsePagination.next"), "{banner}");
        let (last, pagination) = pages.last().expect("last");
        assert!(!pagination.has_more);
        assert!(last.get("warnings").is_none(), "{last:?}");
        for (_, pagination) in &pages {
            assert!(pagination.oversized.is_none());
        }
    }

    #[test]
    fn the_first_page_shows_every_rows_head() {
        let rows = (0..5)
            .map(|index| {
                let matches = (0..300)
                    .map(|line| json!({"line": line, "value": format!("row {index} {}", "x".repeat(60))}))
                    .collect::<Vec<_>>();
                json!({"index": index, "data": {"files": [{"path": format!("f{index}.rs"), "matches": matches}],
                    "next": {"nextPage": {"tool": "localSearch", "query": {"page": 2}}}}})
            })
            .collect::<Vec<_>>();
        let structured = json!({"results": rows})
            .as_object()
            .cloned()
            .expect("object");
        let pages = walk(&structured, 20_000);
        let (first, pagination) = &pages[0];
        assert!(
            pagination.char_length <= 20_000,
            "{}",
            pagination.char_length
        );
        let heads: Vec<_> = first["results"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| (row["index"].as_u64(), row["rowPart"]["part"].as_u64()))
            .collect();
        assert_eq!(
            heads,
            (0..5)
                .map(|index| (Some(index), Some(1)))
                .collect::<Vec<_>>()
        );
        let mut carriers = std::collections::BTreeMap::new();
        for (page, pagination) in &pages {
            assert!(
                pagination.char_length <= 20_000,
                "{}",
                pagination.char_length
            );
            assert!(pagination.oversized.is_none(), "{page:?}");
            for row in page["results"].as_array().expect("rows") {
                let part = row["rowPart"]["part"].as_u64().expect("split");
                let of = row["rowPart"]["of"].as_u64().expect("split");
                if row["data"].get("next").is_some() {
                    assert_eq!(part, of, "{row}");
                    *carriers.entry(row["index"].as_u64()).or_insert(0) += 1;
                }
            }
        }
        assert_eq!(carriers.len(), 5, "{carriers:?}");
        assert!(carriers.values().all(|count| *count == 1), "{carriers:?}");
        assert_eq!(delivered(&pages).len(), 1_500);
    }

    /// B3: a split row's page continuation rides its last part, so a caller
    /// following it never skips the parts in between; a drill-down rides the
    /// first part, beside the head of the evidence. Each appears once.
    #[test]
    fn split_row_continuations_ride_the_part_they_follow() {
        let mut structured = envelope();
        structured["results"][0]["data"]["next"]["readTopMatch"] =
            json!({"tool": "localFetch", "query": {"path": "a.rs"}});
        structured["results"][0]["data"]["next"]["nextPage"] =
            json!({"tool": "localSearch", "query": {"page": 2}});
        let pages = walk(&structured, 1_000);
        assert!(pages.len() > 2);
        let mut carriers = Vec::new();
        let mut parts = 0;
        for (page, _) in &pages {
            for row in page["results"].as_array().expect("rows") {
                if row["index"] != 0 {
                    continue;
                }
                let part = row["rowPart"]["part"].as_u64().expect("row 0 is split");
                parts = row["rowPart"]["of"].as_u64().expect("row 0 is split");
                for name in row["data"]["next"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(name, _)| name)
                {
                    carriers.push((name.clone(), part));
                }
            }
        }
        carriers.sort();
        assert!(parts > 2, "{parts}");
        assert_eq!(
            carriers,
            vec![
                ("nextPage".to_owned(), parts),
                ("readTopMatch".to_owned(), 1),
                ("sidecar".to_owned(), 1),
            ]
        );
    }

    /// A split row's secondary lists (diagnostics beside the paged files)
    /// are not the paged evidence: the first part delivers them once and the
    /// later parts carry them emptied, like an oversized sibling array.
    #[test]
    fn split_row_secondary_lists_ride_the_first_part_once() {
        let mut structured = envelope();
        structured["results"][0]["data"]["diagnostics"] =
            json!([{"code": "pattern.relaxed", "message": "32 matches as written"}]);
        let pages = walk(&structured, 1_000);
        let mut seen = Vec::new();
        for (page, _) in &pages {
            for row in page["results"].as_array().expect("rows") {
                if row["index"] == 0 {
                    let part = row["rowPart"]["part"].as_u64().expect("row 0 is split");
                    let listed = row["data"]["diagnostics"].as_array().map_or(0, Vec::len);
                    seen.push((part, listed));
                }
            }
        }
        assert!(seen.len() > 2, "{seen:?}");
        for (part, listed) in seen {
            assert_eq!(listed, usize::from(part == 1), "part {part}");
        }
    }

    /// An astTopology `cycles` cycle: `files` module paths, `runtime` runtime
    /// cycles of `members` paths each, and its edge records.
    fn topology_cycle(cycle: usize, files: usize, runtime: usize, members: usize) -> Value {
        let edges = |kind: &str, count: usize| {
            (0..count)
                .map(|n| {
                    json!({"from": format!("c{cycle}/{kind}/from_{n}.ts"),
                           "to": format!("c{cycle}/{kind}/to_{n}.ts"),
                           "edgeKinds": ["static-import"]})
                })
                .collect::<Vec<_>>()
        };
        json!({
            "runtimeCycleCount": runtime,
            "runtimeCycles": (0..runtime)
                .map(|r| (0..members)
                    .map(|n| format!("packages/c{cycle}/runtime_{r}/member_{n:03}.ts"))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "cycleEdges": edges("cycle", 4),
            "runtimeCycleEdges": edges("runtime", 7),
            "edgeKinds": ["static-import", "type-import"],
            "files": (0..files)
                .map(|n| format!("excalidraw-app/c{cycle}/components/Module_{n:04}.tsx"))
                .collect::<Vec<_>>(),
        })
    }

    /// Every part of row 0 across `pages`, in order: (rowPart, cycles).
    fn topology_parts(
        pages: &[(Map<String, Value>, ResponsePagination)],
    ) -> Vec<(Value, Vec<Value>)> {
        pages
            .iter()
            .flat_map(|(page, _)| page["results"].as_array().cloned().unwrap_or_default())
            .filter(|row| row["index"] == 0)
            .map(|row| {
                let cycles = row["data"]["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                (row["rowPart"].clone(), cycles)
            })
            .collect()
    }

    /// Whether `cycle` holds evidence of cycle number `id`.
    fn is_cycle(cycle: &Value, id: usize) -> bool {
        let text = cycle.to_string();
        text.contains(&format!("/c{id}/"))
    }

    /// N6: an element too large for a page splits into parts. Its secondary
    /// lists (the actionable runtime cycles and edges) ride its first part,
    /// before any `files` slice, and every later part names the element it
    /// continues on `rowPart.continues`, at position 0 of the paged array.
    #[test]
    fn split_element_later_parts_carry_continues_marker() {
        for (cycles, huge) in [
            (
                vec![topology_cycle(1, 4, 1, 2), topology_cycle(2, 349, 5, 30)],
                1usize,
            ),
            (vec![topology_cycle(2, 349, 5, 30)], 0),
        ] {
            let structured = json!({"results": [{"index": 0, "data": {
                "path": "tsx",
                "results": cycles,
                "summary": {"cycleCount": 2},
            }}]})
            .as_object()
            .cloned()
            .expect("object");
            let pages = walk(&structured, 20_000);
            let parts = topology_parts(&pages);
            let pieces: Vec<_> = parts
                .iter()
                .filter(|(_, cycles)| cycles.iter().any(|cycle| is_cycle(cycle, 2)))
                .collect();
            assert!(pieces.len() > 1, "the huge cycle is split: {parts:?}");
            let (first_part, first_cycles) = pieces[0];
            assert!(first_part.get("continues").is_none(), "{first_part}");
            let head = first_cycles
                .iter()
                .find(|cycle| is_cycle(cycle, 2))
                .expect("cycle 2");
            assert_eq!(
                head["runtimeCycles"].as_array().map(Vec::len),
                Some(5),
                "runtime cycles ride the first part: {head}"
            );
            assert_eq!(head["cycleEdges"].as_array().map(Vec::len), Some(4));
            for (part, cycles) in &pieces[1..] {
                assert_eq!(
                    part["continues"],
                    json!({"array": "/data/results", "index": huge}),
                    "{part}"
                );
                assert!(is_cycle(&cycles[0], 2), "{cycles:?}");
                assert!(
                    cycles[0]["runtimeCycles"]
                        .as_array()
                        .is_none_or(Vec::is_empty),
                    "{part}"
                );
            }
            let mut files: Vec<String> = parts
                .iter()
                .flat_map(|(_, cycles)| cycles.iter())
                .filter(|cycle| is_cycle(cycle, 2))
                .flat_map(|cycle| cycle["files"].as_array().cloned().unwrap_or_default())
                .map(|file| file.to_string())
                .collect();
            let all = files.len();
            files.sort();
            files.dedup();
            assert_eq!((all, files.len()), (349, 349), "every file once");
        }
    }

    /// N7: an astTopology `cycles` row, one huge cycle between two small
    /// ones. Each page holds at most one part of a row, so `results[i]` is
    /// the whole of that row on the page; every edge record is delivered
    /// once; and the first page is filled with the head of the evidence.
    #[test]
    fn a_split_row_has_one_part_per_page_and_never_repeats_its_records() {
        let edges = |cycle: usize, kind: &str, count: usize| {
            (0..count)
                .map(|n| {
                    json!({"from": format!("c{cycle}/{kind}/from_{n}.ts"),
                           "to": format!("c{cycle}/{kind}/to_{n}.ts"),
                           "edgeKinds": ["static-import"]})
                })
                .collect::<Vec<_>>()
        };
        let cycle = |cycle: usize, files: usize, runtime: usize, members: usize| {
            json!({
                "files": (0..files)
                    .map(|n| format!("excalidraw-app/c{cycle}/components/Module_{n:04}.tsx"))
                    .collect::<Vec<_>>(),
                "edgeKinds": ["static-import", "type-import"],
                "runtimeCycles": (0..runtime)
                    .map(|r| (0..members)
                        .map(|n| format!("packages/c{cycle}/runtime_{r}/member_{n:03}.ts"))
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                "runtimeCycleCount": runtime,
                "cycleEdges": edges(cycle, "cycle", 4),
                "runtimeCycleEdges": edges(cycle, "runtime", 7),
            })
        };
        let structured = json!({"results": [{"index": 0, "data": {
            "path": "tsx",
            "results": [cycle(1, 4, 1, 2), cycle(2, 349, 5, 30), cycle(3, 4, 1, 2)],
            "summary": {"cycleCount": 3, "runtimeCycleCount": 7},
            "hints": {"read": {"tool": "localFetch", "query": {"path": "tsx/a.ts"}}}
        }}]})
        .as_object()
        .cloned()
        .expect("object");
        let budget = 20_000;
        let pages = walk(&structured, budget);
        assert!(pages.len() > 1, "the row is split");
        let (mut files, mut edges_seen, mut runtime_seen) = (Vec::new(), Vec::new(), Vec::new());
        for (number, (page, _)) in pages.iter().enumerate() {
            let indexes: Vec<_> = page["results"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| row["index"].as_u64())
                .collect();
            let mut unique = indexes.clone();
            unique.dedup();
            assert_eq!(indexes, unique, "page {number} repeats a row index");
            for row in page["results"].as_array().expect("rows") {
                for cycle in row["data"]["results"].as_array().into_iter().flatten() {
                    files.extend(cycle["files"].as_array().into_iter().flatten().cloned());
                    for key in ["cycleEdges", "runtimeCycleEdges"] {
                        edges_seen.extend(cycle[key].as_array().into_iter().flatten().cloned());
                    }
                    runtime_seen.extend(
                        cycle["runtimeCycles"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                }
            }
        }
        let count = |values: &[Value]| {
            let mut sorted: Vec<String> = values.iter().map(Value::to_string).collect();
            sorted.sort();
            let all = sorted.len();
            sorted.dedup();
            (all, sorted.len())
        };
        assert_eq!(count(&files), (357, 357), "every file once");
        assert_eq!(count(&edges_seen), (33, 33), "every edge record once");
        assert_eq!(count(&runtime_seen), (7, 7), "every runtime cycle once");
        let first = pages[0].1.char_length;
        assert!(
            first > budget / 2,
            "page one carries {first} of {budget} chars"
        );
    }

    #[test]
    fn an_indivisible_fragment_flags_its_page_oversized() {
        let structured = json!({"results": [
            {"index": 0, "data": {"content": "x".repeat(5_000)}},
            {"index": 1, "data": {"content": "small"}}
        ]})
        .as_object()
        .cloned()
        .expect("object");
        let pages = walk(&structured, 1_000);
        let flagged: Vec<_> = pages.iter().map(|(_, p)| p.oversized).collect();
        assert!(flagged.contains(&Some(true)), "{flagged:?}");
        let whole = pages
            .iter()
            .flat_map(|(page, _)| page["results"].as_array().cloned().unwrap_or_default())
            .filter(|row| {
                row["data"]["content"]
                    .as_str()
                    .is_some_and(|c| c.len() == 5_000)
            })
            .count();
        assert_eq!(whole, 1, "the string is never cut");
    }

    #[test]
    fn a_page_request_builds_only_that_pages_fragments() {
        let structured = envelope();
        let full = Value::Object(structured.clone()).to_string();
        let (_, first) = paginate_rows(
            structured.clone(),
            &full,
            &options(700, 0, None),
            Vec::new(),
        );
        assert!(first.total_pages > 20, "{}", first.total_pages);
        let last = first.total_pages - 1;
        MATERIALIZED.with(|count| count.set(0));
        let (page, _) = paginate_rows(
            structured,
            &full,
            &options(700, last, Some(first.snapshot.clone())),
            Vec::new(),
        );
        let returned = page["results"].as_array().map_or(0, Vec::len);
        let built = MATERIALIZED.with(std::cell::Cell::get);
        assert!(
            built <= returned,
            "built {built} fragments to return {returned}"
        );
    }

    /// A split row's last part carries its pages, so it names them as well.
    #[test]
    fn a_last_part_names_the_pages_it_carries() {
        let mut row = json!({"index":0,"data":{
            "files":[],
            "next":{"nextPage":{"tool":"astSearch","query":{"queries":[{"page":2}]}}},
            "hints":{"read":{"tool":"localFetch","query":{"path":"a"}}}
        }});
        keep_row_next(&mut row, PartShare::Last);
        assert_eq!(
            row["data"]["warnings"],
            json!(["more: follow next.nextPage"]),
            "{row}"
        );
        assert!(row["data"].get("hints").is_none(), "{row}");
    }

    /// A split row's first part keeps its warnings while its pages ride the
    /// last part: a warning there names the response page, never a `next`
    /// entry the part does not carry.
    #[test]
    fn a_first_part_warning_names_only_continuations_it_carries() {
        let mut row = json!({"index":0,"data":{
            "warnings":["Regex trap.","4 more pages: follow next.nextPage"],
            "files":[],
            "next":{"nextPage":{"tool":"astSearch","query":{"page":2}}},
            "hints":{"read":{"tool":"localFetch","query":{"path":"a"}}}
        }});
        keep_row_next(&mut row, PartShare::First);
        assert!(row["data"].get("next").is_none(), "{row}");
        assert_eq!(
            row["data"]["warnings"],
            json!([
                "Regex trap.",
                "4 more pages: follow responsePagination.next"
            ]),
            "{row}"
        );
        let mut whole = json!({"index":0,"data":{
            "warnings":["4 more pages: follow next.nextPage"],
            "next":{"nextPage":{"tool":"astSearch","query":{"page":2}}}
        }});
        let before = whole.clone();
        keep_row_next(&mut whole, PartShare::Whole);
        assert_eq!(whole, before);
    }
}
