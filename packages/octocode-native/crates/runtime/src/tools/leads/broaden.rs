//! An empty search's broadening, one rule for every search tool: its broader
//! variants as one batch lead (`broadenSearch`), and the note that names
//! what each variant drops, so no filter is dropped silently.

use crate::id::ToolId;
use crate::response::result::Continuation;
use serde_json::{Value, json};

/// What one empty search can drop to broaden: the tool's own keys a variant
/// never copies (beside the walk position), its narrowing filters, each
/// group dropped whole (a typed filter object), and how a lone
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
        object.retain(|key, _| {
            !crate::contracts::state_fields(tool).contains(key)
                && !drop.strip.contains(&key.as_str())
        });
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
    (!variants.is_empty())
        .then(|| Continuation::related_input(tool, json!({ "queries": variants })).build())
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

/// Add broader fresh requests and explain that their scope differs.
pub(crate) fn set_broadened(value: &mut Value, _row: &Value, lead: Value) {
    if value.get("related").is_none() {
        value["related"] = json!([]);
    }
    if let Some(related) = value.get_mut("related").and_then(Value::as_array_mut) {
        related.push(lead);
    }
    super::push_hint(
        value,
        "Related requests broaden the search by removing a filter; inspect each request before running it.",
    );
}

#[cfg(test)]
mod tests;
