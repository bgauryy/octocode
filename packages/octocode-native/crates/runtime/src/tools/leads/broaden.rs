//! An empty search's broadening, one rule for every search tool: its broader
//! variants as one batch lead (`broadenSearch`), and the note that names
//! what each variant drops, so no filter is dropped silently.

use crate::id::ToolId;
use crate::response::result::{CURSOR_FIELDS, Confidence, Continuation};
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
            .confidence(Confidence::Low)
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

/// `next.broadenSearch` on an empty result, with one hint naming what each
/// of its rows drops or sets relative to `row` (the search it broadens), so
/// no filter is dropped silently.
pub(crate) fn set_broadened(value: &mut Value, row: &Value, lead: Value) {
    let note = broadened_note(row, &lead);
    value["next"]["broadenSearch"] = lead;
    if let Some(note) = note {
        super::push_hint(value, note);
    }
}

/// `broadenSearch rows: queries[0] drops qualifiers; queries[1] drops
/// keyword "b"`: per row of `lead`, the keys of `row` it lacks (each removed
/// keyword by value, each removed word of a split lone keyword) and the
/// values it rewrites. Paging keys are not filters and go unnamed.
fn broadened_note(row: &Value, lead: &Value) -> Option<String> {
    let source = row.as_object()?;
    let rows = lead.pointer("/query/queries")?.as_array()?;
    let parts = rows
        .iter()
        .enumerate()
        .filter_map(|(index, variant)| {
            let (mut dropped, mut set) = (Vec::new(), Vec::new());
            for (key, value) in source {
                if stripped(key) {
                    continue;
                }
                match (key.as_str(), variant.get(key)) {
                    (_, Some(kept)) if kept == value => {}
                    ("keywords", Some(Value::Array(kept))) => {
                        dropped.extend(dropped_terms(value, kept));
                    }
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
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| format!("broadenSearch rows: {}", parts.join("; ")))
}

/// The terms of `source` keywords a variant's `kept` keywords lack: each
/// removed keyword by value, or, when a lone keyword was split, each removed
/// word.
fn dropped_terms(source: &Value, kept: &[Value]) -> Vec<String> {
    let source = source.as_array().map_or(&[][..], Vec::as_slice);
    if let ([Value::String(lone)], [Value::String(left)]) = (source, kept) {
        let mut left = left.split_whitespace().collect::<Vec<_>>();
        return lone
            .split_whitespace()
            .filter(|word| match left.iter().position(|kept| kept == word) {
                Some(at) => {
                    left.remove(at);
                    false
                }
                None => true,
            })
            .map(|word| format!("word {}", json!(word)))
            .collect();
    }
    source
        .iter()
        .filter(|keyword| !kept.contains(keyword))
        .map(|keyword| format!("keyword {keyword}"))
        .collect()
}

#[cfg(test)]
mod tests;
