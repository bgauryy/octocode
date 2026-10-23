//! Output channels for follow-up calls, classified by the core contract's
//! `continuationChannels` (generated into
//! [`crate::id::continuation_channels`]; classified by
//! [`crate::id::channel`]):
//! - `next` keeps pages: continuations that reach unshown data of the same
//!   result. A caller follows every one.
//! - `hints` carries optional guidance. Internally named calls retain their
//!   validation and ordering; [`publish_hints`] emits deduplicated strings at
//!   the public boundary, after path compaction.
//!
//! [`shape`] applies the channel policy to one object; the continuation
//! stage ([`super::continuations::finalize`]) calls it for every object of
//! a response.

mod publish;
pub(crate) use publish::publish_hints;
use serde_json::{Map, Value, json};

use crate::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY, PAGES_KEY};
use crate::id::{Channel, ToolId, channel, continuation_channels, kind};

/// Keys that name a row, and notes that lead it: they stay ahead of the
/// continuation keys.
const ROW_ANCHORS: [&str; 2] = ["id", "warnings"];

/// One rule for every tool: an object's page state and `next` come
/// before its evidence rows, in both the structured and the text channel,
/// so a client that cuts long output keeps the continuation. Order inside
/// the object: anchors (`id`, `warnings`), pages (`pagination`, then the
/// `<part>Pagination` fields), `next`, then everything else as the tool
/// wrote it. The row's `data`, or a resource-major query itself, is the
/// object; `responsePagination` belongs to the envelope and is not touched.
/// A resource-major query (clasify) also leads `hints` before `next`: its
/// `hints.read` is the move to make before walking on.
pub(super) fn lead_continuations(structured: &mut Value, tool: ToolId) {
    let rows_key = tool.output().rows_key();
    for row in structured
        .get_mut(rows_key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let object = if tool.output().resource_major() {
            row.as_object_mut()
        } else {
            row.get_mut("data").and_then(Value::as_object_mut)
        };
        if let Some(object) = object {
            lead_object(object, tool.output().resource_major());
        }
    }
}

fn lead_object(object: &mut Map<String, Value>, hints_before_next: bool) {
    let mut ordered = Map::new();
    let mut take = |object: &mut Map<String, Value>, wanted: &dyn Fn(&str) -> bool| {
        let keys: Vec<String> = object.keys().filter(|key| wanted(key)).cloned().collect();
        for key in keys {
            if let Some(value) = object.shift_remove(&key) {
                ordered.insert(key, value);
            }
        }
    };
    take(object, &|key| ROW_ANCHORS.contains(&key));
    take(object, &|key| key == "pagination");
    take(object, &|key| key.ends_with("Pagination"));
    take(object, &|key| hints_before_next && key == HINTS_KEY);
    take(object, &|key| key == PAGES_KEY);
    ordered.append(object);
    *object = ordered;
}

/// A row's bare prose `hints: [..]` becomes `hints.text` (nested objects,
/// such as a clasify page error, keep their own prose arrays).
pub(super) fn move_text(object: &mut Map<String, Value>) {
    if let Some(Value::Array(_)) = object.get(HINTS_KEY)
        && let Some(text) = object.remove(HINTS_KEY)
    {
        let mut hints = Map::new();
        hints.insert(HINT_TEXT_KEY.to_owned(), text);
        object.insert(HINTS_KEY.to_owned(), Value::Object(hints));
    }
}

/// The channel policy for one object's `next` and `hints`:
/// - a recovery lead (contract `recoveryLeads`) stays only on a `recovery`
///   (empty or failed) row; no lead carries prose;
/// - repeated pages of one kind merge into as few calls as the target's row
///   limit allows ([`merge_repeats`]);
/// - leads move from `next` to the internal hint map, keeping every row and
///   field its tool built (the rows may be routes the response never
///   shows, such as pruned directories);
/// - `confidence` goes from every call once it has ranked the leads, except
///   `low`: the caveat that a call only approximates the request (a lossy
///   repair) stays with it;
/// - with `opt_in_leads` (a successful row whose `extras` lacks `hints`),
///   only the contract's `defaultLeads` stay: the row already holds the
///   path, ref and lines a next call is built from;
/// - an object that [`withholds_evidence`] has no page to reach what it
///   leaves out, so its leads are that route: they all stay, ignoring
///   `opt_in_leads`.
///
/// Idempotent.
pub(super) fn shape(
    object: &mut Map<String, Value>,
    tool: ToolId,
    recovery: bool,
    opt_in_leads: bool,
) {
    let withheld = withholds_evidence(object, tool);
    for key in [PAGES_KEY, HINTS_KEY] {
        let Some(Value::Object(calls)) = object.get_mut(key) else {
            continue;
        };
        calls.retain(|name, _| {
            recovery || !continuation_channels::RECOVERY_LEADS.contains(&name.as_str())
        });
        if calls.is_empty() {
            object.remove(key);
        }
    }
    if let Some(Value::Object(next)) = object.get_mut(PAGES_KEY) {
        merge_repeats(next, tool);
    }
    split(object, tool, withheld, opt_in_leads);
    for key in [PAGES_KEY, HINTS_KEY] {
        if let Some(Value::Object(calls)) = object.get_mut(key) {
            for call in calls.values_mut().filter_map(Value::as_object_mut) {
                if call.get("confidence").and_then(Value::as_str) != Some("low") {
                    call.shift_remove("confidence");
                }
            }
        }
    }
}

/// A partial object (`isPartial: true`) whose `next` holds no page of this
/// tool: its leads are the only route to what it withholds, so they are
/// evidence, not optional guidance. A terminal page (`terminalLimit`)
/// withholds nothing a lead reaches: its rest is on no page. One rule for
/// every tool and lead name.
fn withholds_evidence(object: &Map<String, Value>, tool: ToolId) -> bool {
    object.get("isPartial") == Some(&Value::Bool(true))
        && object.get("terminalLimit") != Some(&Value::Bool(true))
        && !matches!(object.get(PAGES_KEY), Some(Value::Object(next))
            if next.keys().any(|name| channel(tool, name) == Channel::Page))
}

/// Pages of one kind that reach the unread rest of this result
/// (`readHits`, `readHits2`, …) and call the same tool with the same
/// response window differ only in their rows: their distinct rows ride as
/// few calls as the target's row limit allows, named like the tool names
/// them. An enrichment page (`unfiltered`, `expand*`) is another view of
/// rows already shown, so two of one kind are alternative scopes (the
/// unknown evidence, the whole view), not pieces of one rest: they stay
/// apart.
fn merge_repeats(next: &mut Map<String, Value>, tool: ToolId) {
    struct Group {
        kind: String,
        key: Value,
        members: Vec<String>,
        rows: Vec<Value>,
    }
    let mut groups: Vec<Group> = Vec::new();
    for (name, call) in next.iter() {
        if !crate::id::is_remaining(tool, name) {
            continue;
        }
        let (Some(target), Some(rows)) = (
            call.get("tool").and_then(Value::as_str),
            call.pointer("/query/queries").and_then(Value::as_array),
        ) else {
            continue;
        };
        let mut window = call["query"].clone();
        if let Some(window) = window.as_object_mut() {
            window.remove("queries");
        }
        let key = json!([target, window]);
        let group = match groups
            .iter_mut()
            .position(|group| group.kind == kind(name) && group.key == key)
        {
            Some(index) => &mut groups[index],
            None => {
                groups.push(Group {
                    kind: kind(name).to_owned(),
                    key,
                    members: Vec::new(),
                    rows: Vec::new(),
                });
                let last = groups.len() - 1;
                &mut groups[last]
            }
        };
        group.members.push(name.clone());
        for row in rows {
            if !group.rows.contains(row) {
                group.rows.push(row.clone());
            }
        }
    }
    for group in groups.into_iter().filter(|group| group.members.len() > 1) {
        let Some(template) = next.get(&group.members[0]).cloned() else {
            continue;
        };
        let limit = template["tool"]
            .as_str()
            .and_then(ToolId::from_name)
            .map_or(1, crate::response::result::rows_per_call);
        let mut at = next
            .keys()
            .position(|name| *name == group.members[0])
            .unwrap_or(next.len());
        for name in &group.members {
            next.shift_remove(name);
        }
        let mut number = 1;
        for chunk in group.rows.chunks(limit) {
            let name = loop {
                let name = if number == 1 {
                    group.kind.clone()
                } else {
                    format!("{}{number}", group.kind)
                };
                number += 1;
                if !next.contains_key(&name) {
                    break name;
                }
            };
            let mut call = template.clone();
            call["query"]["queries"] = Value::Array(chunk.to_vec());
            next.shift_insert(at.min(next.len()), name, call);
            at += 1;
        }
    }
}

/// Moves every lead out of `next` into the internal hint map, retained when `withheld`
/// (see [`withholds_evidence`]).
fn split(object: &mut Map<String, Value>, tool: ToolId, withheld: bool, opt_in_leads: bool) {
    let leads: Vec<String> = match object.get(PAGES_KEY) {
        Some(Value::Object(next)) => next
            .keys()
            .filter(|name| channel(tool, name) == Channel::Lead)
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    let mut moved = Map::new();
    if let Some(Value::Object(next)) = object.get_mut(PAGES_KEY) {
        for name in leads {
            if let Some(call) = next.shift_remove(&name) {
                moved.insert(name, call);
            }
        }
        if next.is_empty() {
            object.remove(PAGES_KEY);
        }
    }
    // Optional routes are emitted only when requested; default routes stay.
    let keep = |hints: &mut Map<String, Value>| {
        if withheld {
            return;
        }
        if opt_in_leads {
            hints.retain(|name, _| {
                name == HINT_TEXT_KEY || continuation_channels::DEFAULT_LEADS.contains(&kind(name))
            });
        }
        order_leads(hints);
    };
    match object.get_mut(HINTS_KEY) {
        Some(Value::Object(hints)) => {
            hints.extend(moved);
            keep(hints);
            if hints.is_empty() {
                object.remove(HINTS_KEY);
            }
        }
        _ if !moved.is_empty() => {
            keep(&mut moved);
            if moved.is_empty() {
                return;
            }
            // Leads read before the pages that follow them (clasify's exact
            // read ahead of its walk), so `hints` takes `next`'s place.
            let at = object
                .keys()
                .position(|key| key == PAGES_KEY)
                .unwrap_or(object.len());
            object.shift_insert(at, HINTS_KEY.to_owned(), Value::Object(moved));
        }
        _ => {}
    }
}

/// Orders leads without a fixed count cap. The contract's lead priority
/// (`continuationChannels.leadPriority`) goes first, then its default leads
/// (`defaultLeads`: what the default output shows never yields to an
/// opt-in lead), then the rest in the order the tool emitted them (new
/// evidence first). A lead marked `confidence: "low"` yields its place to
/// any other. The choice is deterministic: a stable order, never a tie
/// broken by hashing.
fn order_leads(hints: &mut Map<String, Value>) {
    let rank = |name: &str| {
        continuation_channels::LEAD_PRIORITY
            .iter()
            .position(|lead| *lead == name)
            .unwrap_or(continuation_channels::LEAD_PRIORITY.len())
    };
    let opt_in = |name: &str| !continuation_channels::DEFAULT_LEADS.contains(&kind(name));
    let mut leads: Vec<((bool, usize, bool, usize), String)> = hints
        .iter()
        .filter(|(name, _)| name.as_str() != HINT_TEXT_KEY)
        .enumerate()
        .map(|(order, (name, call))| {
            (
                (call["confidence"] == "low", rank(name), opt_in(name), order),
                name.clone(),
            )
        })
        .collect();
    leads.sort();
    for (_, name) in leads {
        if let Some(call) = hints.shift_remove(&name) {
            hints.insert(name, call);
        }
    }
}

#[cfg(test)]
mod tests;
