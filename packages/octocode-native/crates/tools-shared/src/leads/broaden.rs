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

/// Attach an empty search's broader requests (`lead`) to `value`, the row of
/// `row`. When each request drops exactly one narrowing of `row` (a field, a
/// filter, a keyword) and changes nothing else, the hint names what they
/// drop: their rows answer a wider question than asked. A request that
/// rewrites the search (globs rebased under `path`) or drops nothing is a
/// repair its tool's own hint explains, and a hint already on the row is the
/// more specific one: no generic hint beside either.
pub fn set_broadened(value: &mut Value, row: &Value, lead: Value) {
    let dropped = dropped_narrowings(row, &lead);
    octocode_runtime_base::response::result::attach_related(value, lead);
    if let Some(dropped) = dropped.filter(|_| !has_hints(value)) {
        octocode_runtime_base::response::result::add_hints(value, [broadened_hint(&dropped)]);
    }
}

/// An empty search with nothing broader to offer whose lone keyword is a
/// phrase (several words matched exactly): the next query changes its
/// words. A hint already on the row is the more specific one.
pub fn set_no_broader(value: &mut Value) {
    if !has_hints(value) {
        octocode_runtime_base::response::result::add_hints(
            value,
            ["No match for the phrase: try fewer or other words."],
        );
    }
}

/// Whether the row already carries a hint.
fn has_hints(value: &Value) -> bool {
    value
        .get("hints")
        .and_then(Value::as_array)
        .is_some_and(|hints| !hints.is_empty())
}

/// The hint beside broader requests that each drop one of `dropped`.
fn broadened_hint(dropped: &[String]) -> String {
    match dropped {
        [one] => format!(
            "No match: the related[] query drops {one}; run it only if that wider scope still answers."
        ),
        many => format!(
            "No match: each related[] query drops one of {}; run one only if that wider scope still answers.",
            many.join(", ")
        ),
    }
}

/// What each request of `lead` drops of `row`, one narrowing per request,
/// in request order. `row` is compared as a related request copies it (no
/// private progress, no defaults), and a field every one of several requests
/// leaves out (one the tool never copies, like an index-only search's `ref`)
/// tells none apart, so it is not counted. `None` when a request drops none
/// or several, or adds or rewrites a field.
fn dropped_narrowings(row: &Value, lead: &Value) -> Option<Vec<String>> {
    use octocode_runtime_base::response::result::{canonical_row, fresh_query_row};
    let tool = lead
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)?;
    let mut asked = fresh_query_row(tool, row);
    canonical_row(tool, &mut asked);
    let per_request = lead
        .pointer("/input/queries")?
        .as_array()?
        .iter()
        .map(|request| narrowings_left_out(&asked, request))
        .collect::<Option<Vec<_>>>()?;
    let left_out_by_all: Vec<String> = match per_request.as_slice() {
        [first, _, ..] => first
            .iter()
            .filter(|item| per_request.iter().all(|items| items.contains(item)))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    per_request
        .into_iter()
        .map(|items| {
            let mut items = items
                .into_iter()
                .filter(|item| !left_out_by_all.contains(item));
            match (items.next(), items.next()) {
                (Some(one), None) => Some(one),
                _ => None,
            }
        })
        .collect()
}

/// The narrowings of `asked` that `request` leaves out: a field (`path`),
/// an entry of an object field (`filters.language`), or a keyword
/// (`` `cache` ``; keywords are ANDed, so each one narrows). `None` when
/// `request` adds or changes anything.
fn narrowings_left_out(asked: &Value, request: &Value) -> Option<Vec<String>> {
    let (asked, request) = (asked.as_object()?, request.as_object()?);
    if request.keys().any(|key| !asked.contains_key(key)) {
        return None;
    }
    let mut dropped = Vec::new();
    for (key, value) in asked {
        match (value, request.get(key)) {
            (_, None) => dropped.extend(whole_field(key, value)),
            (value, Some(kept)) if value == kept => {}
            (Value::Object(entries), Some(Value::Object(kept))) => {
                if kept
                    .iter()
                    .any(|(entry, held)| entries.get(entry) != Some(held))
                {
                    return None;
                }
                dropped.extend(
                    entries
                        .keys()
                        .filter(|entry| !kept.contains_key(*entry))
                        .map(|entry| format!("{key}.{entry}")),
                );
            }
            (Value::Array(words), Some(Value::Array(kept))) if key == "keywords" => {
                dropped.extend(keywords_left_out(words, kept)?);
            }
            _ => return None,
        }
    }
    Some(dropped)
}

/// A field `request` leaves out whole, named by what it held: its one entry
/// (`filters.language`), its keywords, else the field.
fn whole_field(key: &str, value: &Value) -> Vec<String> {
    match value {
        Value::Object(entries) if entries.len() == 1 => entries
            .keys()
            .map(|entry| format!("{key}.{entry}"))
            .collect(),
        Value::Array(words) if key == "keywords" => {
            keywords_left_out(words, &[]).unwrap_or_default()
        }
        _ => vec![key.to_owned()],
    }
}

/// The keywords of `words` that `kept` (the same keywords in order, some
/// left out) does not hold; `None` when `kept` is not such a selection.
fn keywords_left_out(words: &[Value], kept: &[Value]) -> Option<Vec<String>> {
    let mut kept = kept.iter().peekable();
    let mut dropped = Vec::new();
    for word in words {
        if kept.peek() == Some(&word) {
            kept.next();
        } else {
            dropped.push(match word {
                Value::String(word) => format!("`{word}`"),
                other => format!("`{other}`"),
            });
        }
    }
    kept.next().is_none().then_some(dropped)
}

#[cfg(test)]
mod tests;
