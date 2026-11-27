//! Preserve columns that distinguish hits on the same source line. AST
//! matches keep columns whenever they share a start line. Other tools can
//! use [`repeated`] to detect rows that also share their displayed text.
//! Each tool checks the full result before paging and calls [`mark`], so
//! hits on opposite sides of a page boundary keep their columns.
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

/// The field that carries the mark from where a tool holds the whole result
/// to where it finishes the row; [`take_mark`] removes it, so it is never
/// served.
const MARK: &str = "sharesLine";

/// For each row of `rows[page]`, whether another row of `rows` has the same
/// key. `key` names a row's place, `(file and line, text)`; `rows` is the
/// whole result in source order, so the rows of one place lie together and
/// only the rows of the page's first and last place are read beyond it.
pub fn repeated<'a, T, P, K>(
    rows: &'a [T],
    page: Range<usize>,
    key: impl Fn(&'a T) -> (P, K),
) -> Vec<bool>
where
    P: Hash + Eq,
    K: Hash + Eq,
{
    let page = page.start.min(rows.len())..page.end.min(rows.len());
    if page.is_empty() {
        return Vec::new();
    }
    let place = |index: usize| key(&rows[index]).0;
    let mut start = page.start;
    let first = place(start);
    while start > 0 && place(start - 1) == first {
        start -= 1;
    }
    let mut end = page.end;
    let last = place(end - 1);
    while end < rows.len() && place(end) == last {
        end += 1;
    }
    let mut counts = HashMap::new();
    for row in &rows[start..end] {
        *counts.entry(key(row)).or_insert(0usize) += 1;
    }
    rows[page]
        .iter()
        .map(|row| counts.get(&key(row)).is_some_and(|count| *count > 1))
        .collect()
}

/// Mark `row` as one whose column tells it apart from another row.
pub fn mark(row: &mut Value) {
    if let Some(fields) = row.as_object_mut() {
        fields.insert(MARK.to_owned(), Value::Bool(true));
    }
}

/// Remove `row`'s mark: whether its column tells it apart from another row.
pub fn take_mark(row: &mut Map<String, Value>) -> bool {
    row.remove(MARK).is_some()
}

#[cfg(test)]
mod tests;
