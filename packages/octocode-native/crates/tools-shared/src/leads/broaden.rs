//! An empty search's broadening, one rule for every search tool: its broader
//! variants as one batch lead (`broadenSearch`), and the note that names
//! what each variant drops, so no filter is dropped silently.

use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use serde_json::{Value, json};

/// What one empty search can drop to broaden: the tool's own keys a variant
/// never copies (beside the walk position), its narrowing filters, each
/// group dropped whole (a typed filter object with several filters drops
/// one filter per variant), and how a lone keyword broadens.
pub struct Broaden<'a> {
    pub strip: &'a [&'a str],
    pub groups: &'a [&'a [&'a str]],
    /// A lone keyword of several words drops one word at a time (a registry
    /// reads one keyword as several terms) instead of dropping whole.
    pub split_lone_keyword: bool,
}

/// An empty search's broader variants as one low-confidence batch lead, so
/// the variant that returns rows names what emptied the search: `row` (the
/// search as a wire row) once per filter group it sets, then once per
/// dropped keyword, the last first (the first usually names the subject); a
/// lone keyword drops whole, or word by word with
/// [`Broaden::split_lone_keyword`]. A variant the tool's contract rejects
/// (nothing left to search by) is skipped, and at most `limit` are kept.
pub fn broaden(tool: ToolId, row: &Value, drop: &Broaden<'_>, limit: usize) -> Option<Value> {
    let mut base = row.clone();
    if let Some(object) = base.as_object_mut() {
        object.retain(|key, _| {
            !octocode_runtime_base::contracts::state_fields(tool).contains(key)
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
    let mut variants = Vec::new();
    for group in drop
        .groups
        .iter()
        .filter(|group| group.iter().any(|key| base.get(*key).is_some()))
    {
        // A group that is one typed filter object drops one filter per
        // variant, so the variant that returns rows names the filter that
        // emptied the search.
        let filters = match group {
            [key] => base.get(*key).and_then(Value::as_object),
            _ => None,
        }
        .filter(|filters| filters.len() > 1);
        match (group, filters) {
            ([key], Some(filters)) => {
                for filter in filters.keys() {
                    let mut variant = base.clone();
                    if let Some(kept) = variant.get_mut(*key).and_then(Value::as_object_mut) {
                        kept.remove(filter);
                    }
                    variants.push(variant);
                }
            }
            _ => variants.push(without(group)),
        }
    }
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
            octocode_runtime_base::contracts::validate(
                tool.as_str(),
                json!({ "queries": [variant] }),
            )
            .is_ok()
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
pub fn set_broadened(value: &mut Value, _row: &Value, lead: Value) {
    octocode_runtime_base::response::result::attach_related(value, lead);
    octocode_runtime_base::response::result::add_hints(
        value,
        [
            "No match with these filters: each related[] request drops one; run it only if the wider scope still answers the question.",
        ],
    );
}

/// An empty search with nothing broader to offer (a lone keyword, no
/// filter to drop): say how GitHub matched it, so the next query changes the
/// words.
pub fn set_no_broader(value: &mut Value) {
    octocode_runtime_base::response::result::add_hints(
        value,
        [
            "No match: search terms are ANDed, so every word must match; try one distinctive word, or a different spelling.",
        ],
    );
}

#[cfg(test)]
mod tests;
