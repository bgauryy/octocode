//! An empty search's broadening, one rule for every search tool: its broader
//! variants as one batch lead (`broadenSearch`), and the note that names
//! what each variant drops, so no filter is dropped silently.

use crate::id::ToolId;
use crate::response::result::{CURSOR_FIELDS, Continuation};
use serde_json::{Value, json};

/// Keys no broadened variant copies beside [`CURSOR_FIELDS`]: a variant is
/// a fresh search from its first page, at the default page size. The brief
/// and `extras` stay: the continuation stage owns them.
const PAGE_SIZE: &str = "pageSize";

/// Whether a broadened variant never copies `key`.
fn stripped(key: &str) -> bool {
    key == PAGE_SIZE || CURSOR_FIELDS.contains(&key)
}

/// What one empty search can drop to broaden: the tool's own keys a variant
/// never copies (beside the walk position), its narrowing filters, each
/// group dropped whole (`since` and `until` are one window), and how a lone
/// keyword broadens.
pub(crate) struct Broaden<'a> {
    pub(crate) strip: &'a [&'a str],
    pub(crate) groups: &'a [&'a [&'a str]],
    /// A lone keyword of several words drops one word at a time (a registry
    /// reads one keyword as several terms) instead of dropping whole.
    pub(crate) split_lone_keyword: bool,
}

/// An empty search's broader variants as one low-confidence batch lead, so
/// the variant that returns rows names what emptied the search: `row` (the
/// search as a wire row) once per filter group it sets, then once per
/// dropped keyword, the last first (the first usually names the subject); a
/// lone keyword drops whole, or word by word with
/// [`Broaden::split_lone_keyword`]. A variant the tool's contract rejects
/// (nothing left to search by) is skipped, and at most `limit` are kept.
pub(crate) fn broaden(
    tool: ToolId,
    row: &Value,
    drop: &Broaden<'_>,
    limit: usize,
) -> Option<Value> {
    let mut base = row.clone();
    if let Some(object) = base.as_object_mut() {
        object.retain(|key, _| !stripped(key) && !drop.strip.contains(&key.as_str()));
    }
    let without = |keys: &[&str]| {
        let mut variant = base.clone();
        if let Some(object) = variant.as_object_mut() {
            for key in keys {
                object.remove(*key);
            }
        }
        variant
    };
    let with_keywords = |keywords: Value| {
        let mut variant = base.clone();
        variant["keywords"] = keywords;
        variant
    };
    let mut variants = drop
        .groups
        .iter()
        .filter(|group| group.iter().any(|key| base.get(*key).is_some()))
        .map(|group| without(group))
        .collect::<Vec<_>>();
    let keywords = base["keywords"].as_array().map_or(&[][..], Vec::as_slice);
    let lone_words = match keywords {
        [Value::String(lone)] if drop.split_lone_keyword => {
            Some(lone.split_whitespace().collect::<Vec<_>>()).filter(|words| words.len() > 1)
        }
        _ => None,
    };
    match (keywords, lone_words) {
        (_, Some(words)) => variants.extend(
            (0..words.len())
                .rev()
                .map(|dropped| with_keywords(json!([without_index(&words, dropped).join(" ")]))),
        ),
        ([], None) => {}
        ([_], None) => variants.push(without(&["keywords"])),
        (many, None) => variants.extend(
            (0..many.len())
                .rev()
                .map(|dropped| with_keywords(Value::Array(without_index(many, dropped)))),
        ),
    }
    let variants = variants
        .into_iter()
        .filter(|variant| {
            crate::contracts::validate(tool.as_str(), json!({ "queries": [variant] })).is_ok()
        })
        .take(limit)
        .collect::<Vec<_>>();
    (!variants.is_empty()).then(|| {
        Continuation::input(tool, json!({ "queries": variants }))
            .approximate()
            .build()
    })
}

/// `items` without the one at `dropped`.
fn without_index<T: Clone>(items: &[T], dropped: usize) -> Vec<T> {
    items
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != dropped)
        .map(|(_, item)| item.clone())
        .collect()
}

/// `next.broadenSearch` on an empty result, with hints naming what each of
/// its rows drops or sets relative to `row` (the search it broadens), so no
/// filter is dropped silently.
pub(crate) fn set_broadened(value: &mut Value, row: &Value, lead: Value) {
    let notes = broadened_notes(row, &lead);
    value["next"]["broadenSearch"] = lead;
    for note in notes {
        super::push_hint(value, note);
    }
}

const NOTE_PREFIX: &str = "broadenSearch rows: ";

/// `broadenSearch rows: queries[0] drops qualifiers; queries[1] drops
/// keyword "b"`: per row of `lead`, the keys of `row` it lacks (each removed
/// keyword by value, each removed word of a split lone keyword) and the
/// values it rewrites. Paging keys are not filters and go unnamed.
///
/// Every note fits one hint's guidance budget, whatever the number of rows:
/// rows are packed in order, and a row starts the next note when it would
/// overflow this one. A row whose own entry is over the budget (long
/// keywords or values) names them by position instead, which keeps every
/// row named; the row's query in the lead carries the values.
fn broadened_notes(row: &Value, lead: &Value) -> Vec<String> {
    let (Some(source), Some(rows)) = (
        row.as_object(),
        lead.pointer("/query/queries").and_then(Value::as_array),
    ) else {
        return Vec::new();
    };
    let note = |parts: &[String]| format!("{NOTE_PREFIX}{}", parts.join("; "));
    let fits = |text: &str| text.chars().count() <= crate::response::rows::MAX_GUIDANCE_CHARS;
    let mut notes = Vec::new();
    let mut open: Vec<String> = Vec::new();
    for (index, variant) in rows.iter().enumerate() {
        let Some(part) = row_change(index, source, variant, false)
            .filter(|part| fits(&note(std::slice::from_ref(part))))
            .or_else(|| row_change(index, source, variant, true))
        else {
            continue;
        };
        open.push(part);
        if open.len() > 1
            && !fits(&note(&open))
            && let Some(last) = open.pop()
        {
            notes.push(note(&open));
            open = vec![last];
        }
    }
    if !open.is_empty() {
        notes.push(note(&open));
    }
    notes
}

/// What one row `variant` drops or sets relative to `source`, as
/// `queries[i] drops …, sets …`; values are named by position when
/// `by_position`.
fn row_change(
    index: usize,
    source: &serde_json::Map<String, Value>,
    variant: &Value,
    by_position: bool,
) -> Option<String> {
    let (mut dropped, mut set) = (Vec::new(), Vec::new());
    for (key, value) in source {
        if stripped(key) {
            continue;
        }
        match (key.as_str(), variant.get(key)) {
            (_, Some(kept)) if kept == value => {}
            ("keywords", Some(Value::Array(kept))) => {
                dropped.extend(dropped_terms(value, kept, by_position));
            }
            (_, Some(_)) if by_position => set.push(key.clone()),
            (_, Some(new)) => set.push(format!("{key} {new}")),
            (_, None) => dropped.push(key.clone()),
        }
    }
    let changes = [("drops", dropped), ("sets", set)]
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(verb, items)| format!("{verb} {}", items.join(", ")))
        .collect::<Vec<_>>();
    (!changes.is_empty()).then(|| format!("queries[{index}] {}", changes.join(", ")))
}

/// The terms of `source` keywords a variant's `kept` keywords lack: each
/// removed keyword by value, or, when a lone keyword was split, each removed
/// word; by 1-based position (`keyword 2`, `word 1`) when `by_position`.
fn dropped_terms(source: &Value, kept: &[Value], by_position: bool) -> Vec<String> {
    let source = source.as_array().map_or(&[][..], Vec::as_slice);
    let name = |noun: &str, position: usize, value: String| {
        if by_position {
            format!("{noun} {}", position + 1)
        } else {
            format!("{noun} {value}")
        }
    };
    if let ([Value::String(lone)], [Value::String(left)]) = (source, kept) {
        let mut left = left.split_whitespace().collect::<Vec<_>>();
        return lone
            .split_whitespace()
            .enumerate()
            .filter(
                |(_, word)| match left.iter().position(|kept| kept == word) {
                    Some(at) => {
                        left.remove(at);
                        false
                    }
                    None => true,
                },
            )
            .map(|(position, word)| name("word", position, json!(word).to_string()))
            .collect();
    }
    source
        .iter()
        .enumerate()
        .filter(|(_, keyword)| !kept.contains(keyword))
        .map(|(position, keyword)| name("keyword", position, keyword.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
