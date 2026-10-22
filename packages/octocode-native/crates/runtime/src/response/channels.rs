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

use serde_json::{Map, Value, json};

use crate::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY, PAGES_KEY};
use crate::id::{Channel, ToolId, channel, continuation_channels, kind};

/// Publish optional guidance once, after its calls have been validated and
/// paths compacted. Required continuations remain structured in `next`.
pub(crate) fn publish_hints(value: &mut Value, tool: ToolId) {
    let Ok(contract) = crate::contracts::tool_contract(tool) else {
        return;
    };
    let schema = &contract["outputSchema"];
    publish_at(value, &[schema], &schema["$defs"]);
}

fn publish_at(value: &mut Value, schemas: &[&Value], defs: &Value) {
    match value {
        Value::Array(items) => {
            let items_schemas: Vec<_> = schemas
                .iter()
                .flat_map(|schema| crate::contracts::item_schemas(schema, defs))
                .collect();
            if !items_schemas.is_empty() {
                for item in items {
                    publish_at(item, &items_schemas, defs);
                }
            }
        }
        Value::Object(object) => {
            let mut empty_hints = false;
            for (key, child) in object.iter_mut() {
                let declared: Vec<_> = schemas
                    .iter()
                    .flat_map(|schema| crate::contracts::property_schemas(schema, defs, key))
                    .collect();
                if declared.is_empty() {
                    continue;
                }
                if key == HINTS_KEY {
                    let guidance = match child {
                        Value::Array(items) => items.iter().all(Value::is_string),
                        Value::Object(fields) => fields.iter().all(|(key, value)| {
                            (key == HINT_TEXT_KEY
                                && value
                                    .as_array()
                                    .is_some_and(|items| items.iter().all(Value::is_string)))
                                || crate::response::result::is_call(value)
                        }),
                        _ => false,
                    };
                    if guidance {
                        let mut strings = Vec::new();
                        collect_hints(
                            child.take(),
                            None,
                            &mut strings,
                            &mut std::collections::HashSet::new(),
                        );
                        empty_hints = strings.is_empty();
                        *child = Value::Array(strings);
                    }
                } else if key != PAGES_KEY {
                    publish_at(child, &declared, defs);
                }
            }
            if empty_hints {
                object.remove(HINTS_KEY);
            }
        }
        _ => {}
    }
}

fn collect_hints(
    value: Value,
    label: Option<&str>,
    strings: &mut Vec<Value>,
    seen: &mut std::collections::HashSet<String>,
) {
    let text = match value {
        Value::String(text) => text,
        Value::Array(items) => {
            items
                .into_iter()
                .for_each(|item| collect_hints(item, None, strings, seen));
            return;
        }
        call @ Value::Object(_) if crate::response::result::is_call(&call) => {
            let mut identity = call.clone();
            identity["query"].sort_all_objects();
            let (Some(line), Some(identity)) = (
                crate::response::result::call_line(&call),
                crate::response::result::call_line(&identity),
            ) else {
                return;
            };
            if !seen.insert(identity) {
                return;
            }
            match label {
                Some(label) => format!("{label}: {line}"),
                None => line,
            }
        }
        Value::Object(mut fields) => {
            if let Some(text) = fields.shift_remove(HINT_TEXT_KEY) {
                collect_hints(text, None, strings, seen);
            }
            fields
                .into_iter()
                .for_each(|(name, hint)| collect_hints(hint, Some(&name), strings, seen));
            return;
        }
        _ => return,
    };
    let text = Value::String(text.trim().to_owned());
    if text.as_str().is_some_and(|text| !text.is_empty()) && !strings.contains(&text) {
        strings.push(text);
    }
}

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
mod tests {
    use super::{lead_continuations, shape};
    use crate::id::ToolId;
    use crate::id::{Channel, channel, continuation_channels, is_remaining};
    use crate::response::render::{TextFormat, render_tool};
    use serde_json::{Value, json};

    #[test]
    fn published_guidance_is_deduplicated_text_and_keeps_pages_executable() {
        let call =
            json!({"tool":"localFetch","query":{"queries":[{"path":"a.rs","ranges":["2-4"]}]}});
        let mut response = json!({"results":[{"data":{
            "next":{"continueWindow":call},
            "hints":{"text":[" narrow ","narrow",""],"read":call,"duplicate":call},
            "pullRequests":[{"hints":{"read":call}}]
        }}]});
        super::publish_hints(&mut response, ToolId::GhGetHistoryItem);
        let data = &response["results"][0]["data"];
        assert_eq!(data["next"]["continueWindow"], call);
        let hints = data["hints"].as_array().expect("string hints");
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0], "narrow");
        let suggestion = hints[1].as_str().expect("suggested call");
        let query: Value =
            serde_json::from_str(suggestion.strip_prefix("read: localFetch ").expect("tool"))
                .expect("JSON input");
        assert_eq!(query, call["query"]);
        assert_eq!(data["pullRequests"][0]["hints"], json!([suggestion]));
        let once = response.clone();
        super::publish_hints(&mut response, ToolId::GhGetHistoryItem);
        assert_eq!(response, once);
        let mut evidence = json!({"queries":[{"answers":{"score":{"type":"score",
            "legend":{"0":{"hints":[" repeated "," repeated "],"nested":{"hints":{"read":call}}}}}}}]});
        let original = evidence.clone();
        super::publish_hints(&mut evidence, ToolId::Clasify);
        assert_eq!(evidence, original, "evidence fields are not guidance");
    }

    #[test]
    fn equivalent_hint_calls_ignore_object_order_but_keep_batch_order() {
        let first = json!({"queries":[{"path":"a.rs","ranges":["2-4"]},{"path":"b.rs"}]});
        let reordered = json!({"queries":[{"ranges":["2-4"],"path":"a.rs"},{"path":"b.rs"}]});
        let reversed = json!({"queries":[{"path":"b.rs"},{"path":"a.rs","ranges":["2-4"]}]});
        let mut response = json!({"results":[{"data":{"hints":{
            "read":{"tool":"localFetch","query":first},
            "same":{"tool":"localFetch","query":reordered},
            "reversed":{"tool":"localFetch","query":reversed}
        }}}]});
        super::publish_hints(&mut response, ToolId::LocalFetch);
        assert_eq!(
            response["results"][0]["data"]["hints"],
            json!([
                format!("read: localFetch {first}"),
                format!("reversed: localFetch {reversed}")
            ])
        );
    }

    /// [`shape`] on every object of every row, as the continuation stage
    /// applies it.
    fn split_hints(structured: &mut Value, tool: ToolId) {
        let key = if tool == ToolId::Clasify {
            "queries"
        } else {
            "results"
        };
        for row in structured[key].as_array_mut().into_iter().flatten() {
            if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
                super::move_text(data);
            }
            if let Some(object) = row.as_object_mut() {
                super::move_text(object);
            }
        }
        walk(&mut structured[key], tool);
    }

    fn walk(value: &mut Value, tool: ToolId) {
        if crate::response::result::is_call(value) {
            return;
        }
        match value {
            Value::Array(items) => items.iter_mut().for_each(|item| walk(item, tool)),
            Value::Object(object) => {
                shape(object, tool, false, false);
                for (key, child) in object.iter_mut() {
                    if key != "next" && key != "hints" {
                        walk(child, tool);
                    }
                }
            }
            _ => {}
        }
    }

    #[test]
    fn only_pages_that_leave_more_to_read_are_remaining() {
        for name in [
            "nextPage",
            "continueWindow",
            "continuePatch",
            "expandScan",
            "retry",
        ] {
            assert!(is_remaining(ToolId::LocalSearch, name), "{name}");
        }
        for name in ["restart", "restartScan", "readBody", "read", "nextpage"] {
            assert!(!is_remaining(ToolId::LocalSearch, name), "{name}");
        }
        assert!(is_remaining(ToolId::Clasify, "clasify"));
    }

    #[test]
    fn every_declared_continuation_kind_has_its_contract_channel() {
        // A tool whose name no kind shares, so the own-walk rule stays out.
        for name in continuation_channels::PAGE_KINDS {
            assert_eq!(channel(ToolId::LspSearch, name), Channel::Page, "{name}");
        }
        for name in continuation_channels::LEAD_KINDS {
            assert_eq!(channel(ToolId::LspSearch, name), Channel::Lead, "{name}");
        }
        assert_eq!(channel(ToolId::Clasify, "clasify"), Channel::Page);
        assert_eq!(channel(ToolId::LocalSearch, "clasify"), Channel::Lead);
        assert_eq!(channel(ToolId::Clasify, "read"), Channel::Lead);
    }

    #[test]
    fn splits_leads_and_prose_out_of_next_without_touching_pages() {
        let page = json!({"tool":"localSearch","query":{"path":"/r","matchString":"x","page":2}});
        let lead = json!({"tool":"localFetch","query":{"path":"/r/a"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "hints":["Add includeIgnored:true."],
            "next":{"nextPage":page,"read":lead},
            "pullRequests":[{"number":1,"next":{"readBody":lead}}]
        }}],"responsePagination":{"next":{"tool":"localSearch","query":{}}}});
        split_hints(&mut structured, ToolId::LocalSearch);
        let data = &structured["results"][0]["data"];
        assert_eq!(data["next"], json!({"nextPage":page}));
        assert_eq!(
            data["hints"],
            json!({"text":["Add includeIgnored:true."],"read":lead})
        );
        assert_eq!(data["pullRequests"][0]["hints"], json!({"readBody":lead}));
        assert!(data["pullRequests"][0].get("next").is_none());
        assert!(structured["responsePagination"]["next"].is_object());
        let once = structured.clone();
        split_hints(&mut structured, ToolId::LocalSearch);
        assert_eq!(structured, once, "idempotent");
    }

    #[test]
    fn hints_keep_distinct_leads_without_capping_pages_or_text() {
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"/r/a"}});
        let mut low = call("localSearch");
        low["confidence"] = json!("low");
        let mut structured = json!({"results":[{"index":0,"data":{
            "hints":["Tip one.","Tip two.","Tip three."],
            "next":{
                "nextPage":call("lspSearch"),
                "continueWalk":call("lspSearch"),
                "continueWalk2":json!({"tool":"lspSearch","query":{"path":"/r/b"}}),
                "widen":low,
                "readDefinition":call("localFetch"),
                "findCallers":call("lspSearch"),
                "read":call("localFetch")
            }
        }}]});
        split_hints(&mut structured, ToolId::LspSearch);
        let data = &structured["results"][0]["data"];
        assert_eq!(
            data["next"].as_object().map(|next| next.len()),
            Some(3),
            "pages are never capped: {data}"
        );
        let hints = data["hints"].as_object().expect("hints");
        assert_eq!(hints["text"].as_array().map(Vec::len), Some(3));
        let leads = hints
            .keys()
            .filter(|key| *key != "text")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            leads,
            vec!["readDefinition", "findCallers", "read", "widen"],
            "{data}"
        );
        let once = structured.clone();
        split_hints(&mut structured, ToolId::LspSearch);
        assert_eq!(structured, once, "idempotent");
    }

    /// Result counts do not arbitrarily remove distinct optional routes.
    #[test]
    fn lead_availability_does_not_depend_on_the_result_count() {
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let leads = |data: Value, tool: ToolId| {
            let mut structured = json!({"results":[{"index":0,"data":data}]});
            split_hints(&mut structured, tool);
            structured["results"][0]["data"]["hints"]
                .as_object()
                .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let menu = || json!({"readFiles":call("ghGetHistoryItem"),"readDiscussion":call("ghGetHistoryItem")});
        let one =
            json!({"pullRequests":[{"number":1,"labels":["bug"]}],"warnings":["w"],"next":menu()});
        assert_eq!(
            leads(one, ToolId::GhGetHistoryItem),
            ["readFiles", "readDiscussion"]
        );
        let two = json!({"pullRequests":[{"number":1},{"number":2}],"next":menu()});
        assert_eq!(leads(two, ToolId::GhGetHistoryItem).len(), 2);
        let partial = json!({"pullRequests":[{"number":1}],"isPartial":true,"next":menu()});
        assert_eq!(leads(partial, ToolId::GhGetHistoryItem).len(), 2);
        // One file with two matches is two hits.
        let file = json!({"files":[{"path":"a.rs","matches":[{"line":1},{"line":2}]}],
            "next":{"read":call("localFetch"),"findCallers":call("lspSearch")}});
        assert_eq!(leads(file, ToolId::LocalSearch).len(), 2);
        let hit = json!({"files":[{"path":"a.rs","matches":[{"line":1,"enclosing":{"line":1}}]}],
            "next":{"read":call("localFetch"),"findCallers":call("lspSearch")}});
        assert_eq!(leads(hit, ToolId::LocalSearch), ["read", "findCallers"]);
        // Exact routes and optional alternatives remain available.
        let exact = |tool: &str| json!({"tool":tool,"confidence":"exact","query":{"path":"a"}});
        let named = json!({"pullRequests":[{"number":1}],
            "next":{"readFiles":exact("ghGetHistoryItem"),"readDiscussion":exact("ghGetHistoryItem")}});
        assert_eq!(
            leads(named, ToolId::GhGetHistoryItem),
            ["readFiles", "readDiscussion"]
        );
        let alternative = json!({"pullRequests":[{"number":1}],
            "next":{"readFiles":exact("ghGetHistoryItem"),"readDiscussion":call("ghGetHistoryItem")}});
        assert_eq!(
            leads(alternative, ToolId::GhGetHistoryItem),
            ["readFiles", "readDiscussion"]
        );
        let paged = json!({"files":[{"path":"a.rs","matches":[{"line":1}]}],
            "next":{"nextPage":call("localSearch"),"read":call("localFetch"),"findCallers":call("lspSearch")}});
        assert_eq!(leads(paged, ToolId::LocalSearch).len(), 2);
    }

    #[test]
    fn contract_priority_leads_precede_rereads() {
        assert_eq!(
            continuation_channels::LEAD_PRIORITY.first(),
            Some(&"readAtMerge")
        );
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "pullRequests":[{"number":1,"next":{
                "readBody":call("ghGetHistoryItem"),
                "readUntrimmed":call("ghGetHistoryItem"),
                "readAtMerge":call("ghGetFileContent")
            }}]
        }}]});
        split_hints(&mut structured, ToolId::GhGetHistoryItem);
        let hints = &structured["results"][0]["data"]["pullRequests"][0]["hints"];
        let leads = hints
            .as_object()
            .map(|h| h.keys().cloned().collect::<Vec<_>>());
        assert_eq!(
            leads,
            Some(vec![
                "readAtMerge".to_owned(),
                "readBody".to_owned(),
                "readUntrimmed".to_owned()
            ]),
            "{hints}"
        );
    }

    /// A wide semantic search page appends its clasify handoff after the
    /// tool's own reads; the contract priority keeps it under the cap, so
    /// the screening route is reachable at all.
    #[test]
    fn the_clasify_search_handoff_survives_the_cap() {
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let mut structured = json!({"results":[{"index":0,"data":{"next":{
            "nextPage":call("ghSearchCode"),
            "read":call("ghGetFileContent"),
            "readHits":call("ghGetFileContent"),
            "clasify":call("clasify")
        }}}]});
        split_hints(&mut structured, ToolId::GhSearchCode);
        let data = &structured["results"][0]["data"];
        assert!(data["next"]["nextPage"].is_object(), "{data}");
        let leads = data["hints"]
            .as_object()
            .map(|h| h.keys().cloned().collect::<Vec<_>>());
        assert_eq!(
            leads,
            Some(vec!["read".to_owned(), "clasify".to_owned()]),
            "{data}"
        );
    }

    /// The listing of skipped binaries is a lead (the search is complete
    /// without it) that a full lead menu still keeps: its warning only
    /// counts the files.
    #[test]
    fn the_skipped_binary_listing_remains_a_lead() {
        assert_eq!(channel(ToolId::LocalSearch, "binarySkipped"), Channel::Lead);
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let mut structured = json!({"results":[{"index":0,"data":{"next":{
            "nextPage":call("localSearch"),
            "read":call("localFetch"),
            "binarySkipped":call("structureSearch"),
            "findReferences":call("lspSearch")
        }}}]});
        split_hints(&mut structured, ToolId::LocalSearch);
        let data = &structured["results"][0]["data"];
        assert!(data["next"]["nextPage"].is_object(), "{data}");
        assert!(data["next"].get("binarySkipped").is_none(), "{data}");
        let leads = data["hints"]
            .as_object()
            .map(|h| h.keys().cloned().collect::<Vec<_>>());
        assert_eq!(
            leads,
            Some(vec![
                "read".to_owned(),
                "binarySkipped".to_owned(),
                "findReferences".to_owned()
            ]),
            "{data}"
        );
    }

    /// Minimizing a lead never touches the fields that select its evidence:
    /// every `ranges` entry (the rest of a cut declaration is two spans),
    /// `matchString` and `path` reach the target unchanged.
    #[test]
    fn lead_minimization_keeps_every_evidence_selecting_field() {
        let row = json!({"path":"src/lib.rs","ranges":["1-9","15-21"],"matchString":"needle","contextLines":2});
        for lead in [
            json!({"tool":"localFetch","query":{"queries":[row.clone()]}}),
            json!({"tool":"localFetch","query":{"queries":[row.clone(), {"path":"b.rs"}]}}),
        ] {
            let mut structured = json!({"results":[{"index":0,"data":{
                "next":{"read":lead.clone()}
            }}]});
            split_hints(&mut structured, ToolId::LocalFetch);
            let kept = &structured["results"][0]["data"]["hints"]["read"]["query"]["queries"];
            assert_eq!(kept, &lead["query"]["queries"], "{structured}");
        }
    }

    /// A lead keeps every row its tool built, like a page: its rows may be
    /// routes the response never shows (pruned directories at their depth).
    #[test]
    fn a_lead_keeps_every_row_its_tool_built() {
        let rows = json!([
            {"path":"a.rs","ranges":["1-3","9-12"]},
            {"path":"b.rs","ranges":["4-5"]}
        ]);
        let mut structured = json!({"results":[{"index":0,"data":{
            "next":{
                "nextPage":{"tool":"localFetch","query":{"queries":rows}},
                "read":{"tool":"localFetch","query":{"queries":rows}}
            },
            "files":[{"path":"a.rs","hints":{"read":{"tool":"localFetch","query":{"path":"a.rs","ranges":["1-3","9-12"]}}}}]
        }}]});
        split_hints(&mut structured, ToolId::AstSearch);
        let data = &structured["results"][0]["data"];
        assert_eq!(data["next"]["nextPage"]["query"]["queries"], rows);
        assert_eq!(data["hints"]["read"]["query"], json!({"queries":rows}));
        assert_eq!(
            data["files"][0]["hints"]["read"]["query"]["ranges"],
            json!(["1-3", "9-12"]),
            "a resource-shaped read (clasify's located lines) is one answer"
        );
        let once = structured.clone();
        split_hints(&mut structured, ToolId::AstSearch);
        assert_eq!(structured, once, "idempotent");
    }

    /// `broadenSearch` rows are alternatives (drop-one-keyword
    /// variants), not other rows already shown: the lead keeps them all.
    #[test]
    fn a_broaden_search_lead_keeps_every_alternative_row() {
        let rows = json!([
            {"owner":"o","repo":"r","operation":"pullRequest","keywords":["a"]},
            {"owner":"o","repo":"r","operation":"pullRequest","keywords":["b"]}
        ]);
        let mut structured = json!({"results":[{"index":0,"status":"empty","data":{
            "next":{"broadenSearch":{"tool":"ghSearchHistory","query":{"queries":rows}}}
        }}]});
        split_hints(&mut structured, ToolId::GhSearchHistory);
        assert_eq!(
            structured["results"][0]["data"]["hints"]["broadenSearch"]["query"]["queries"], rows,
            "{structured}"
        );
    }

    #[test]
    fn confidence_goes_from_every_call_but_low_which_still_yields_its_place() {
        let call = |tool: &str, confidence: &str| json!({"tool":tool,"confidence":confidence,"query":{"queries":[{"path":"a"}],"confidence":"exact"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "next": {
                "nextPage": call("localSearch", "exact"),
                "widen": call("localSearch", "low"),
                "read": call("localFetch", "high"),
                "findReferences": call("lspSearch", "medium")
            },
            "files": [{"path":"a","hints":{"read":call("localFetch", "exact")}}],
            "confidence": "exact"
        }}]});
        split_hints(&mut structured, ToolId::LocalSearch);
        let data = &structured["results"][0]["data"];
        assert!(
            data["next"]["nextPage"].get("confidence").is_none(),
            "{data}"
        );
        assert_eq!(data["next"]["nextPage"]["query"]["confidence"], "exact");
        let leads = data["hints"].as_object().expect("hints");
        assert_eq!(
            leads.keys().cloned().collect::<Vec<_>>(),
            vec!["read", "findReferences", "widen"],
            "low-confidence leads follow exact routes: {data}"
        );
        assert!(
            leads.iter().all(|(name, lead)| if name == "widen" {
                lead["confidence"] == "low"
            } else {
                lead.get("confidence").is_none()
            }),
            "{data}"
        );
        assert!(
            data["files"][0]["hints"]["read"]
                .get("confidence")
                .is_none(),
            "{data}"
        );
        assert_eq!(data["confidence"], "exact", "only continuations change");
        // A low call that keeps its place keeps its caveat.
        let mut lossy = json!({"results":[{"index":0,"status":"error","data":{
            "next": {"repair": call("localSearch", "low")}
        }}]});
        split_hints(&mut lossy, ToolId::LocalSearch);
        assert_eq!(
            lossy["results"][0]["data"]["hints"]["repair"]["confidence"], "low",
            "{lossy}"
        );
    }

    /// Evidence a row withholds (a capped file's hits, a capped body, a
    /// clipped line, narrowed patches) is the unread rest of the result: it
    /// stays in `next` as one call per admitted batch.
    #[test]
    fn withheld_evidence_reads_stay_in_next_as_one_call() {
        for name in [
            "readHits",
            "readHits2",
            "readDeclaration3",
            "wholeLines",
            "readBoundedLines",
        ] {
            assert_eq!(channel(ToolId::GhSearchCode, name), Channel::Page, "{name}");
        }
        assert_eq!(channel(ToolId::GhSearchCode, "read2"), Channel::Lead);
        // A matchString patch view shows what was asked: the whole patch is a lead.
        assert_eq!(
            channel(ToolId::GhGetHistoryItem, "readFullPatches2"),
            Channel::Lead
        );
        let read = |path: &str| json!({"tool":"ghGetFileContent","query":{"queries":[{"owner":"o","repo":"r","path":path}]}});
        let mut next = serde_json::Map::new();
        next.insert("nextPage".into(), json!({"tool":"ghSearchCode","query":{"queries":[{"owner":"o","keywords":["k"],"page":2}]}}));
        next.insert("read".into(), read("top"));
        for index in 1..=7 {
            let name = if index == 1 {
                "readHits".to_owned()
            } else {
                format!("readHits{index}")
            };
            next.insert(name, read(&format!("f{index}")));
        }
        next.insert("readHits8".into(), read("f1"));
        next.insert(
            "viewRepo".into(),
            json!({"tool":"ghStructure","query":{"queries":[{"owner":"o","repo":"r"}]}}),
        );
        next.insert(
            "findCode".into(),
            json!({"tool":"ghSearchCode","query":{"queries":[{"owner":"o"}]}}),
        );
        let mut structured = json!({"results":[{"index":0,"data":{"next":next}}]});
        split_hints(&mut structured, ToolId::GhSearchCode);
        let data = &structured["results"][0]["data"];
        let pages = data["next"].as_object().expect("next");
        assert_eq!(
            pages.keys().cloned().collect::<Vec<_>>(),
            vec!["nextPage", "readHits", "readHits2"],
            "{data}"
        );
        let paths = |name: &str| {
            pages[name]["query"]["queries"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(paths("readHits"), ["f1", "f2", "f3", "f4", "f5"]);
        assert_eq!(paths("readHits2"), ["f6", "f7"], "duplicates merge once");
        assert_eq!(
            data["hints"].as_object().map(|h| h.len()),
            Some(3),
            "distinct optional routes remain available: {data}"
        );
        let once = structured.clone();
        split_hints(&mut structured, ToolId::GhSearchCode);
        assert_eq!(structured, once, "idempotent");
    }

    /// Every lead keeps all of its rows, with or without a next page.
    #[test]
    fn a_partial_object_without_a_page_keeps_all_leads_and_rows() {
        let rows = json!([{"path":"a"},{"path":"b"},{"path":"c"}]);
        let lead = |tool: &str| json!({"tool":tool,"query":{"queries":rows}});
        let next = |page: bool| {
            let mut next = json!({
                "findText":lead("localSearch"),
                "readDefinition":lead("localFetch"),
                "findCallers":lead("lspSearch")
            });
            if page {
                next["nextImporterPage"] = lead("lspSearch");
            }
            next
        };
        let mut open =
            json!({"results":[{"index":0,"data":{"isPartial":true,"next":next(false)}}]});
        split_hints(&mut open, ToolId::LspSearch);
        let hints = open["results"][0]["data"]["hints"]
            .as_object()
            .expect("hints");
        assert_eq!(hints.len(), 3, "{open}");
        assert!(
            hints.values().all(|call| call["query"]["queries"] == rows),
            "{open}"
        );
        let once = open.clone();
        split_hints(&mut open, ToolId::LspSearch);
        assert_eq!(open, once, "idempotent");
        let mut paged =
            json!({"results":[{"index":0,"data":{"isPartial":true,"next":next(true)}}]});
        split_hints(&mut paged, ToolId::LspSearch);
        let hints = paged["results"][0]["data"]["hints"]
            .as_object()
            .expect("hints");
        assert_eq!(hints.len(), 3, "all distinct optional routes stay: {paged}");
        assert!(
            hints.values().all(|call| call["query"]["queries"] == rows),
            "a lead keeps its rows: {paged}"
        );
    }

    /// Opt-in leads leave before ordering: a default lead emitted after
    /// optional ones is not capped away by leads the default output drops.
    #[test]
    fn a_default_lead_remains_when_opt_in_leads_are_hidden() {
        let lead = |tool: &str| json!({"tool":tool,"query":{"queries":[{"path":"a"}]}});
        let mut object = json!({"next":{
            "ignoreCase":lead("localSearch"),
            "findFile":lead("structureSearch"),
            "narrowScope":lead("localSearch"),
            "viewFileCount":lead("localSearch")
        }})
        .as_object()
        .cloned()
        .expect("object");
        shape(&mut object, ToolId::LocalSearch, false, true);
        let kept = object["hints"]
            .as_object()
            .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(kept.contains(&"viewFileCount".to_owned()), "{kept:?}");
        assert!(kept.contains(&"narrowScope".to_owned()), "{kept:?}");
        assert!(!kept.contains(&"ignoreCase".to_owned()), "{kept:?}");
    }

    /// With `extras: ["hints"]` (opt-in leads allowed), a default lead
    /// still outranks an opt-in one the tool emitted first: asking for more
    /// hints never hides one the default output shows.
    #[test]
    fn opting_into_hints_keeps_the_default_leads() {
        let lead = |tool: &str| json!({"tool":tool,"query":{"queries":[{"path":"a"}]}});
        let mut object = json!({"next":{
            "read":lead("localFetch"),
            "includeSkipped":lead("localSearch"),
            "viewFileCount":lead("localSearch")
        }})
        .as_object()
        .cloned()
        .expect("object");
        shape(&mut object, ToolId::LocalSearch, false, false);
        let kept = object["hints"]
            .as_object()
            .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(
            kept,
            ["includeSkipped", "viewFileCount", "read"],
            "{object:?}"
        );
    }

    /// A terminal page (`terminalLimit`) is partial because the rest is on
    /// no page, and no lead reaches it either: its leads are optional like
    /// any complete row's, so opt-in and the cap apply.
    #[test]
    fn a_terminal_page_keeps_the_opt_in_lead_policy() {
        let rows = json!([{"path":"a"},{"path":"b"}]);
        let lead = |tool: &str| json!({"tool":tool,"query":{"queries":rows}});
        let shaped = |terminal: bool| {
            let mut object = json!({
                "isPartial":true,
                "next":{
                    "findText":lead("localSearch"),
                    "readDefinition":lead("localFetch"),
                    "findCallers":lead("lspSearch")
                }
            });
            if terminal {
                object["terminalLimit"] = json!(true);
            }
            let mut object = object.as_object().cloned().expect("object");
            shape(&mut object, ToolId::LspSearch, false, true);
            Value::Object(object)
        };
        let open = shaped(false);
        assert_eq!(
            open["hints"].as_object().map(serde_json::Map::len),
            Some(3),
            "{open}"
        );
        let terminal = shaped(true);
        let kept = terminal["hints"]
            .as_object()
            .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(
            kept.iter()
                .all(|name| continuation_channels::DEFAULT_LEADS.contains(&name.as_str())),
            "{terminal}"
        );
        assert!(
            terminal
                .get("hints")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|hints| hints.values())
                .all(|call| call["query"]["queries"].as_array().map(Vec::len) == Some(1)),
            "{terminal}"
        );
    }

    #[test]
    fn clasify_keeps_its_walk_in_next_and_moves_reads_to_hints() {
        let read = json!({"tool":"localFetch","query":{"path":"/r/a","ranges":["1-2"]}});
        let mut structured = json!({"queries":[{"id":"q","hints":["Use localSearch."],
            "next":{"clasify":{"resources":[],"questions":[]},"read":read},
            "resources":[{"resourceId":"f","pages":[{"next":{"read":read}}]}]}]});
        split_hints(&mut structured, ToolId::Clasify);
        let query = &structured["queries"][0];
        assert!(query["next"]["clasify"].is_object());
        assert_eq!(
            query["hints"],
            json!({"text":["Use localSearch."],"read":read})
        );
        assert_eq!(
            query["resources"][0]["pages"][0]["hints"],
            json!({"read":read})
        );
    }

    /// One row whose evidence precedes its page state and `next`, as the
    /// GitHub tools wrote it before the rule.
    fn evidence_first_response(tool: ToolId) -> Value {
        let call = json!({"tool": tool.as_str(), "query": {"queries": [{"page": 2}]}});
        let data = json!({
            "evidenceRows": [{"path": "a"}],
            "contentPagination": {"currentPage": 1, "hasMore": true},
            "hints": {"text": ["tip"]},
            "pagination": {"currentPage": 1, "hasMore": true},
            "next": {"nextPage": call},
            "warnings": ["w"],
        });
        if tool.output().resource_major() {
            let mut query = data;
            query["id"] = json!("q");
            json!({"queries": [query]})
        } else {
            json!({"results": [{"index": 0, "status": "ok", "data": data}]})
        }
    }

    fn row_object(response: &Value, tool: ToolId) -> &Value {
        if tool.output().resource_major() {
            &response["queries"][0]
        } else {
            &response["results"][0]["data"]
        }
    }

    #[test]
    fn every_tool_leads_pagination_and_next_before_its_evidence() {
        for tool in ToolId::ALL {
            let mut response = evidence_first_response(tool);
            lead_continuations(&mut response, tool);
            let keys: Vec<&str> = row_object(&response, tool)
                .as_object()
                .expect("row object")
                .keys()
                .map(String::as_str)
                .collect();
            let at = |name: &str| keys.iter().position(|key| *key == name).expect(name);
            let evidence = at("evidenceRows");
            for lead in ["pagination", "contentPagination", "next"] {
                assert!(
                    at(lead) < evidence,
                    "{tool:?}: {lead} after the rows: {keys:?}"
                );
            }
            assert!(
                at("pagination") < at("contentPagination"),
                "{tool:?}: {keys:?}"
            );
            assert!(at("contentPagination") < at("next"), "{tool:?}: {keys:?}");
            let anchors = if tool.output().resource_major() { 2 } else { 1 };
            assert!(at("warnings") < anchors, "{tool:?}: {keys:?}");
        }
    }

    #[test]
    fn the_rule_is_idempotent_and_keeps_every_key() {
        for tool in ToolId::ALL {
            let mut once = evidence_first_response(tool);
            lead_continuations(&mut once, tool);
            let mut twice = once.clone();
            lead_continuations(&mut twice, tool);
            assert_eq!(once.to_string(), twice.to_string(), "{tool:?}");
            let before = evidence_first_response(tool);
            assert_eq!(
                row_object(&once, tool)
                    .as_object()
                    .map(|object| object.len()),
                row_object(&before, tool)
                    .as_object()
                    .map(|object| object.len()),
                "{tool:?}"
            );
        }
    }

    #[test]
    fn both_text_encodings_show_pagination_and_next_before_the_rows() {
        for tool in ToolId::ALL {
            let mut response = evidence_first_response(tool);
            lead_continuations(&mut response, tool);
            for format in [TextFormat::Json, TextFormat::Yaml] {
                let text = render_tool(tool, &response, &json!({}), format);
                let (page, next, rows) = (
                    text.find("pagination"),
                    text.find("next"),
                    text.find("evidenceRows"),
                );
                assert!(
                    page < rows && next < rows,
                    "{tool:?} {format:?}: continuation after the rows:\n{text}"
                );
                // YAML prints each call as the one line the CLI runs.
                let call = &row_object(&response, tool)["next"]["nextPage"];
                let line = crate::response::result::call_line(call).expect("call");
                assert!(
                    format == TextFormat::Json || text.contains(&format!("nextPage: {line}\n")),
                    "{tool:?}: the call is not one line:\n{text}"
                );
            }
        }
    }
}
