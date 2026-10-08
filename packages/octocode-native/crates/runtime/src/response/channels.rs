//! Output channels for follow-up calls, classified by the core contract's
//! `continuationChannels` (generated into
//! [`crate::tools::id::continuation_channels`]; classified by
//! [`crate::tools::id::channel`]):
//! - `next` keeps pages: continuations that reach unshown data of the same
//!   result. A caller follows every one.
//! - `hints` is one object of optional agent guidance: prose tips under
//!   `text` and lead calls under their own names.
//!
//! [`shape`] applies the channel policy to one object; the continuation
//! stage ([`super::continuations::finalize`]) calls it for every object of
//! a response.

use serde_json::{Map, Value, json};

pub use crate::tools::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY, PAGES_KEY};
use crate::tools::id::{Channel, ToolId, channel, continuation_channels, kind};

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
///   (empty or failed) row, and `why` only there, kept short;
/// - repeated pages of one kind merge into as few calls as the target's row
///   limit allows ([`merge_repeats`]);
/// - leads move from `next` to `hints`, which keeps at most
///   [`MAX_HINT_LEADS`] of them, each reading only its top hit;
/// - `confidence` goes from every call once it has ranked the leads.
///
/// Idempotent.
pub(super) fn shape(object: &mut Map<String, Value>, tool: ToolId, recovery: bool) {
    for key in [PAGES_KEY, HINTS_KEY] {
        let Some(Value::Object(calls)) = object.get_mut(key) else {
            continue;
        };
        calls.retain(|name, _| {
            recovery || !continuation_channels::RECOVERY_LEADS.contains(&name.as_str())
        });
        for call in calls.values_mut().filter_map(Value::as_object_mut) {
            match call.get("why").and_then(Value::as_str) {
                Some(why) if recovery => {
                    let short = super::rows::concise(why);
                    call.insert("why".into(), Value::String(short));
                }
                Some(_) => {
                    call.shift_remove("why");
                }
                None => {}
            }
        }
        if calls.is_empty() {
            object.remove(key);
        }
    }
    if let Some(Value::Object(next)) = object.get_mut(PAGES_KEY) {
        merge_repeats(next, tool);
    }
    split(object, tool);
    if single_hit(object)
        && let Some(Value::Object(hints)) = object.get_mut(HINTS_KEY)
    {
        cap_leads(hints, 1);
    }
    for key in [PAGES_KEY, HINTS_KEY] {
        if let Some(Value::Object(calls)) = object.get_mut(key) {
            for (name, call) in calls.iter_mut() {
                let Some(call) = call.as_object_mut() else {
                    continue;
                };
                call.shift_remove("confidence");
                if key == HINTS_KEY && !ALTERNATIVE_ROW_LEADS.contains(&name.as_str()) {
                    top_hit(call);
                }
            }
        }
    }
}

/// Leads whose rows are alternatives to try (drop-one-keyword variants), not
/// other rows already shown: they keep every row.
const ALTERNATIVE_ROW_LEADS: [&str; 1] = ["broadenSearch"];

/// A lead is an optional route, so it carries the minimal runnable row:
/// the top hit's row. The other rows are already shown, and withheld
/// evidence rides `next`. A row's own ranges stay whole: they may be one
/// unit (the rest of a declaration around a shown window).
fn top_hit(call: &mut Map<String, Value>) {
    if let Some(rows) = call
        .get_mut("query")
        .and_then(|query| query.get_mut("queries"))
        .and_then(Value::as_array_mut)
    {
        rows.truncate(1);
    }
}

/// Pages of one kind (`readHits`, `readHits2`, …) that call the same tool
/// with the same response window differ only in their rows: their distinct
/// rows ride as few calls as the target's row limit allows, named like the
/// tool names them.
fn merge_repeats(next: &mut Map<String, Value>, tool: ToolId) {
    struct Group {
        kind: String,
        key: Value,
        members: Vec<String>,
        rows: Vec<Value>,
    }
    let mut groups: Vec<Group> = Vec::new();
    for (name, call) in next.iter() {
        if channel(tool, name) != Channel::Page {
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
            .map_or(1, max_rows)
            .max(1);
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

/// Rows one call of `tool` accepts (the contract's `queries.maxItems`).
fn max_rows(tool: ToolId) -> usize {
    crate::contracts::tool_contract(tool)
        .ok()
        .and_then(|contract| contract.pointer("/inputSchema/properties/queries/maxItems"))
        .and_then(Value::as_u64)
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(1)
}

/// Moves every lead out of `next` into `hints`, capped.
fn split(object: &mut Map<String, Value>, tool: ToolId) {
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
    match object.get_mut(HINTS_KEY) {
        Some(Value::Object(hints)) => {
            hints.extend(moved);
            cap_leads(hints, MAX_HINT_LEADS);
        }
        _ if !moved.is_empty() => {
            cap_leads(&mut moved, MAX_HINT_LEADS);
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

/// Lead calls one `hints` object may offer; prose tips are not leads and
/// pages (`next`) are never capped. A complete single-hit answer
/// ([`single_hit`]) offers one.
pub const MAX_HINT_LEADS: usize = 2;

/// Keys beside the evidence lists: guidance and coverage notes.
const NOT_EVIDENCE: [&str; 4] = ["warnings", "partialReasons", "diagnostics", HINTS_KEY];

/// A complete answer of one hit: no page left (`next`), not partial, and
/// exactly one non-empty evidence list holding one entry with no list of 2+
/// records inside (one file with several matches is several hits).
fn single_hit(object: &Map<String, Value>) -> bool {
    if object.contains_key(PAGES_KEY) || object.get("isPartial") == Some(&Value::Bool(true)) {
        return false;
    }
    let mut lists = object
        .iter()
        .filter(|(key, _)| !NOT_EVIDENCE.contains(&key.as_str()))
        .filter_map(|(_, value)| value.as_array())
        .filter(|list| !list.is_empty());
    match (lists.next(), lists.next()) {
        (Some(only), None) => only.len() == 1 && !holds_records(&only[0]),
        _ => false,
    }
}

/// Whether `value` holds a list of 2+ objects at any depth.
fn holds_records(value: &Value) -> bool {
    match value {
        Value::Array(items) => {
            items.iter().filter(|item| item.is_object()).count() >= 2
                || items.iter().any(holds_records)
        }
        Value::Object(fields) => fields.values().any(holds_records),
        _ => false,
    }
}

/// Keeps the first `limit` leads. The contract's lead priority
/// (`continuationChannels.leadPriority`) goes first; other leads follow in
/// the order the tool emitted them (new evidence first). A lead marked
/// `confidence: "low"` yields its place to any other. The choice is
/// deterministic: a stable order, never a tie broken by hashing.
fn cap_leads(hints: &mut Map<String, Value>, limit: usize) {
    let rank = |name: &str| {
        continuation_channels::LEAD_PRIORITY
            .iter()
            .position(|lead| *lead == name)
            .unwrap_or(continuation_channels::LEAD_PRIORITY.len())
    };
    let mut leads: Vec<((bool, usize, usize), String)> = hints
        .iter()
        .filter(|(name, _)| name.as_str() != HINT_TEXT_KEY)
        .enumerate()
        .map(|(order, (name, call))| {
            (
                (call["confidence"] == "low", rank(name), order),
                name.clone(),
            )
        })
        .collect();
    if leads.len() <= limit {
        return;
    }
    leads.sort();
    for (_, name) in leads.into_iter().skip(limit) {
        hints.shift_remove(&name);
    }
}

#[cfg(test)]
mod tests {
    use super::shape;
    use crate::tools::id::ToolId;
    use crate::tools::id::{Channel, channel, continuation_channels, is_remaining};
    use serde_json::{Value, json};

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
        match value {
            Value::Array(items) => items.iter_mut().for_each(|item| walk(item, tool)),
            Value::Object(object) => {
                if object.get("tool").is_some() && object.get("query").is_some() {
                    return;
                }
                shape(object, tool, false);
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
            "continue",
            "continuePatch",
            "expandScan",
            "retry",
        ] {
            assert!(is_remaining(ToolId::LocalSearch, name), "{name}");
        }
        for name in [
            "restart",
            "restartDiagnostics",
            "readBody",
            "read",
            "nextpage",
        ] {
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
            "hints":["Add noIgnore:true."],
            "next":{"nextPage":page,"read":lead},
            "pullRequests":[{"number":1,"next":{"readBody":lead}}]
        }}],"responsePagination":{"next":{"tool":"localSearch","query":{}}}});
        split_hints(&mut structured, ToolId::LocalSearch);
        let data = &structured["results"][0]["data"];
        assert_eq!(data["next"], json!({"nextPage":page}));
        assert_eq!(
            data["hints"],
            json!({"text":["Add noIgnore:true."],"read":lead})
        );
        assert_eq!(data["pullRequests"][0]["hints"], json!({"readBody":lead}));
        assert!(data["pullRequests"][0].get("next").is_none());
        assert!(structured["responsePagination"]["next"].is_object());
        let once = structured.clone();
        split_hints(&mut structured, ToolId::LocalSearch);
        assert_eq!(structured, once, "idempotent");
    }

    #[test]
    fn hints_offer_at_most_two_leads_and_never_cap_pages_or_text() {
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
        assert_eq!(leads, vec!["readDefinition", "findCallers"], "{data}");
        let once = structured.clone();
        split_hints(&mut structured, ToolId::LspSearch);
        assert_eq!(structured, once, "idempotent");
    }

    /// B9: a complete answer of one hit offers one lead; several hits, a
    /// partial answer, or one with pages left keep the two-lead menu.
    #[test]
    fn a_complete_single_hit_answer_offers_one_lead() {
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
        assert_eq!(leads(one, ToolId::GhGetHistoryItem), ["readFiles"]);
        let two = json!({"pullRequests":[{"number":1},{"number":2}],"next":menu()});
        assert_eq!(leads(two, ToolId::GhGetHistoryItem).len(), 2);
        let partial = json!({"pullRequests":[{"number":1}],"isPartial":true,"next":menu()});
        assert_eq!(leads(partial, ToolId::GhGetHistoryItem).len(), 2);
        // One file with two matches is two hits.
        let file = json!({"files":[{"path":"a.rs","matches":[{"line":1},{"line":2}]}],
            "next":{"read":call("localFetch"),"callers":call("lspSearch")}});
        assert_eq!(leads(file, ToolId::LocalSearch).len(), 2);
        let hit = json!({"files":[{"path":"a.rs","matches":[{"line":1,"enclosing":{"line":1}}]}],
            "next":{"read":call("localFetch"),"callers":call("lspSearch")}});
        assert_eq!(leads(hit, ToolId::LocalSearch), ["read"]);
        let paged = json!({"files":[{"path":"a.rs","matches":[{"line":1}]}],
            "next":{"nextPage":call("localSearch"),"read":call("localFetch"),"callers":call("lspSearch")}});
        assert_eq!(leads(paged, ToolId::LocalSearch).len(), 2);
    }

    #[test]
    fn contract_priority_leads_survive_the_cap_ahead_of_rereads() {
        assert_eq!(
            continuation_channels::LEAD_PRIORITY.first(),
            Some(&"readAtMerge")
        );
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "pullRequests":[{"number":1,"next":{
                "readRawBody":call("ghGetHistoryItem"),
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
            Some(vec!["readRawBody".to_owned(), "readAtMerge".to_owned()]),
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
    fn the_skipped_binary_listing_is_a_lead_that_survives_the_cap() {
        assert_eq!(channel(ToolId::LocalSearch, "binarySkipped"), Channel::Lead);
        let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
        let mut structured = json!({"results":[{"index":0,"data":{"next":{
            "nextPage":call("localSearch"),
            "read":call("localFetch"),
            "binarySkipped":call("structureSearch"),
            "references":call("lspSearch")
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
            Some(vec!["read".to_owned(), "binarySkipped".to_owned()]),
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
                "next":{"readBlock":lead}
            }}]});
            split_hints(&mut structured, ToolId::LocalFetch);
            let kept = &structured["results"][0]["data"]["hints"]["readBlock"]["query"]["queries"];
            assert_eq!(kept, &json!([row]), "{structured}");
        }
    }

    /// A lead is an optional route, so it carries the minimal runnable row:
    /// the top hit's row. Pages keep every row.
    #[test]
    fn a_lead_reads_only_the_top_hit_and_pages_keep_every_row() {
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
        assert_eq!(
            data["hints"]["read"]["query"],
            json!({"queries":[{"path":"a.rs","ranges":["1-3","9-12"]}]})
        );
        assert_eq!(
            data["files"][0]["hints"]["read"]["query"]["ranges"],
            json!(["1-3", "9-12"]),
            "a resource-shaped read (clasify's located lines) is one answer"
        );
        let once = structured.clone();
        split_hints(&mut structured, ToolId::AstSearch);
        assert_eq!(structured, once, "idempotent");
    }

    /// SH5: `broadenSearch` rows are alternatives (drop-one-keyword
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
    fn confidence_goes_from_every_call_and_low_still_yields_its_place() {
        let call = |tool: &str, confidence: &str| json!({"tool":tool,"confidence":confidence,"query":{"queries":[{"path":"a"}],"confidence":"exact"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "next": {
                "nextPage": call("localSearch", "exact"),
                "widen": call("localSearch", "low"),
                "read": call("localFetch", "high"),
                "references": call("lspSearch", "medium")
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
            vec!["read", "references"],
            "a low lead yields its place: {data}"
        );
        assert!(
            leads.values().all(|lead| lead.get("confidence").is_none()),
            "{data}"
        );
        assert!(
            data["files"][0]["hints"]["read"]
                .get("confidence")
                .is_none(),
            "{data}"
        );
        assert_eq!(data["confidence"], "exact", "only continuations change");
    }

    /// Evidence a row withholds (a capped file's hits, a capped body, a
    /// clipped line, narrowed patches) is the unread rest of the result: it
    /// stays in `next`, past the lead cap, as one call per row limit.
    #[test]
    fn withheld_evidence_reads_stay_in_next_as_one_call() {
        for name in [
            "readHits",
            "readHits2",
            "readDeclaration3",
            "wholeLines",
            "readBoundedLines",
            "readFullPatches2",
        ] {
            assert_eq!(channel(ToolId::GhSearchCode, name), Channel::Page, "{name}");
        }
        assert_eq!(channel(ToolId::GhSearchCode, "read2"), Channel::Lead);
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
            "searchCode".into(),
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
            Some(2),
            "optional routes keep the lead cap: {data}"
        );
        let once = structured.clone();
        split_hints(&mut structured, ToolId::GhSearchCode);
        assert_eq!(structured, once, "idempotent");
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
}
