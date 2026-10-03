//! Output channels for follow-up calls.
//!
//! Tools emit every follow-up call under `next` while they run. Before the
//! public response is validated, [`split_hints`] sorts them into two
//! channels:
//! - `next` keeps pages: continuations that reach unshown data of the same
//!   result. A caller follows every one.
//! - `hints` is one object of optional agent guidance: prose tips under
//!   `text` and lead calls under their own names.
//!
//! The classification is the core contract's `continuationChannels`,
//! generated into [`continuation_channels`].

use serde_json::{Map, Value};

pub use crate::tools::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY, PAGES_KEY};
use crate::tools::id::{ToolId, continuation_channels};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// More of the same result.
    Page,
    /// An optional follow-up call.
    Lead,
}

/// The channel of continuation `name` emitted by `tool`. An entry named
/// after its own tool resumes that tool's walk (clasify's `next.clasify`);
/// the same name on another tool is a handoff lead.
#[must_use]
pub fn channel(tool: ToolId, name: &str) -> Channel {
    let page = name == tool.as_str()
        || continuation_channels::PAGE_NAMES.contains(&name)
        || continuation_channels::PAGE_PREFIXES
            .iter()
            .any(|prefix| starts_word(name, prefix));
    if page { Channel::Page } else { Channel::Lead }
}

/// `name` is `prefix` or `prefix` followed by a capitalized word.
fn starts_word(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_uppercase()))
}

/// Moves every lead out of `next` into `hints`, and a row's prose
/// `hints: [..]` into `hints.text`, for each result row (ordinary tools) or
/// query result (clasify). Idempotent; envelope-level `responsePagination`
/// is untouched.
pub fn split_hints(structured: &mut Value, tool: ToolId) {
    let rows_key = if tool == ToolId::Clasify {
        "queries"
    } else {
        "results"
    };
    for row in structured
        .get_mut(rows_key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(data) = row.get_mut("data") {
            move_text(data);
            split_walk(data, tool);
        }
        move_text(row);
        split_walk(row, tool);
    }
}

/// A bare prose array becomes `hints.text`.
fn move_text(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(_)) = object.get(HINTS_KEY)
        && let Some(text) = object.remove(HINTS_KEY)
    {
        let mut hints = Map::new();
        hints.insert(HINT_TEXT_KEY.to_owned(), text);
        object.insert(HINTS_KEY.to_owned(), Value::Object(hints));
    }
}

/// Splits every `next` map in `value`. Continuation calls are caller content
/// and are never entered.
fn split_walk(value: &mut Value, tool: ToolId) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| split_walk(item, tool)),
        Value::Object(object) => {
            if object.get("query").is_some() && object.get("tool").is_some_and(Value::is_string) {
                return;
            }
            split_object(object, tool);
            for (key, child) in object.iter_mut() {
                if key != PAGES_KEY && key != HINTS_KEY {
                    split_walk(child, tool);
                }
            }
        }
        _ => {}
    }
}

fn split_object(object: &mut Map<String, Value>, tool: ToolId) {
    let Some(Value::Object(next)) = object.get_mut(PAGES_KEY) else {
        return;
    };
    let leads: Vec<String> = next
        .keys()
        .filter(|name| channel(tool, name) == Channel::Lead)
        .cloned()
        .collect();
    if leads.is_empty() {
        return;
    }
    let mut moved = Map::new();
    for name in leads {
        if let Some(call) = next.remove(&name) {
            moved.insert(name, call);
        }
    }
    if next.is_empty() {
        object.remove(PAGES_KEY);
    }
    match object.get_mut(HINTS_KEY) {
        Some(Value::Object(hints)) => {
            hints.extend(moved);
            cap_leads(hints);
        }
        _ => {
            cap_leads(&mut moved);
            object.insert(HINTS_KEY.to_owned(), Value::Object(moved));
        }
    }
}

/// Lead calls one `hints` object may offer; prose tips are not leads and
/// pages (`next`) are never capped.
pub const MAX_HINT_LEADS: usize = 2;

/// Keeps the first [`MAX_HINT_LEADS`] leads. A tool emits its leads in
/// preference order (new evidence first); a lead marked `confidence: "low"`
/// yields its place to any other. The choice is deterministic: a stable
/// order, never a tie broken by hashing.
fn cap_leads(hints: &mut Map<String, Value>) {
    let mut leads: Vec<((bool, usize), String)> = hints
        .iter()
        .filter(|(name, _)| name.as_str() != HINT_TEXT_KEY)
        .enumerate()
        .map(|(order, (name, call))| ((call["confidence"] == "low", order), name.clone()))
        .collect();
    if leads.len() <= MAX_HINT_LEADS {
        return;
    }
    leads.sort();
    for (_, name) in leads.into_iter().skip(MAX_HINT_LEADS) {
        hints.shift_remove(&name);
    }
}

#[cfg(test)]
mod tests {
    use super::{Channel, channel, continuation_channels, split_hints};
    use crate::tools::id::ToolId;
    use serde_json::json;

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
        let page = json!({"tool":"localSearch","query":{"path":"/r","searchText":"x","page":2}});
        let lead = json!({"tool":"localFetch","query":{"path":"/r/a"}});
        let mut structured = json!({"results":[{"index":0,"data":{
            "hints":["Add noIgnore:true."],
            "next":{"nextPage":page,"readTopMatch":lead},
            "pullRequests":[{"number":1,"next":{"getBody":lead}}]
        }}],"responsePagination":{"next":{"tool":"localSearch","query":{}}}});
        split_hints(&mut structured, ToolId::LocalSearch);
        let data = &structured["results"][0]["data"];
        assert_eq!(data["next"], json!({"nextPage":page}));
        assert_eq!(
            data["hints"],
            json!({"text":["Add noIgnore:true."],"readTopMatch":lead})
        );
        assert_eq!(data["pullRequests"][0]["hints"], json!({"getBody":lead}));
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
                "continueWalk2":call("lspSearch"),
                "widen":low,
                "readDefinition":call("localFetch"),
                "findCallers":call("lspSearch"),
                "readTopMatch":call("localFetch")
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

    #[test]
    fn clasify_keeps_its_walk_in_next_and_moves_reads_to_hints() {
        let read = json!({"tool":"localFetch","query":{"path":"/r/a","startLine":1,"endLine":2}});
        let mut structured = json!({"queries":[{"queryId":"q","hints":["Use localSearch."],
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
