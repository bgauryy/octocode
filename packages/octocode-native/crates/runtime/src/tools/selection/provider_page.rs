//! The provider-page selection: classified rows of a tool whose provider
//! returns its own pages of files (ghSearchCode). The page is the judged
//! source and each of its hits a candidate; its unfiltered lead keeps the page.
use super::provider_hits;
use super::{
    Candidate, Failure, Runtime, select_scoped, unclassified_warning, unfiltered, unless_transient,
};
use crate::id::ToolId;
use crate::response::selection_errors::{receipt_kind, unavailable};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// Select the sanitized evidence of one provider page. Never hydrate hidden
/// file bodies here; readers declare the evidence their candidate view contains.
pub(crate) fn apply_sanitized_rows(
    tool: ToolId,
    output: &dyn crate::response::output::ToolOutput,
    structured: &mut Value,
    queries: &[crate::security::CheckedQuery],
    row_indices: &[usize],
    response_query: &mut Value,
    runtime: &Runtime,
) -> Result<(), crate::execution::ExecutionError> {
    let (Some((scope, unit)), Some(rows)) = (
        receipt_kind(tool),
        structured.get_mut("results").and_then(Value::as_array_mut),
    ) else {
        return Ok(());
    };
    let unavailable_receipt = |total: usize, reason: &str| {
        serde_json::to_value(unavailable(scope, unit, total, reason)).unwrap_or(Value::Null)
    };
    for row in rows {
        let Some(index) = row["index"].as_u64().and_then(|i| usize::try_from(i).ok()) else {
            continue;
        };
        let Some(admitted_index) = row_indices.iter().position(|&original| original == index)
        else {
            continue;
        };
        let Some(query) = queries.get(admitted_index) else {
            continue;
        };
        if query.get("classification").is_none() && query.get("selection").is_none() {
            continue;
        }
        let failed = row["status"] == "error";
        let data = &mut row["data"];
        if failed {
            let total = data["files"].as_array().map_or(0, Vec::len);
            if !data.is_object() {
                *data = json!({});
            }
            if let Some(fields) = data.as_object_mut() {
                fields.remove("files");
            }
            data["selection"] = unavailable_receipt(total, "sourceUnavailable");
            set_next(data, "unfiltered", provider_page_unfiltered(tool, query));
            continue;
        }
        if !data.is_object() {
            *data = json!({});
        }
        let Some(Value::Array(values)) = data.get_mut("files").map(Value::take) else {
            row["status"] = json!("error");
            row["data"] = json!({
                "errorCode":ErrorCode::InvalidClassificationView,
                "error":"Classification requires an array of source evidence candidates.",
                "selection": unavailable_receipt(
                    0,
                    crate::id::error_codes::as_str(ErrorCode::InvalidClassificationView),
                ),
                "next":{"unfiltered":provider_page_unfiltered(tool, query)}
            });
            continue;
        };
        // Every hit is a candidate; a row with nothing to judge stays one
        // unknown candidate.
        let hits = values
            .iter()
            .map(|item| {
                let judged = has_text(item) && !withheld(item);
                provider_hits::hits(item, query, judged)
            })
            .collect::<Vec<_>>();
        let states = values
            .iter()
            .zip(&hits)
            .flat_map(|(item, hits)| {
                hits.iter().map(move |hit| {
                    provider_hits::state(item, hit).map(|state| runtime.judge.project(state))
                })
            })
            .collect::<Vec<_>>();
        let source = candidate_source_digest(tool, query, data, &states);
        let candidates = states.into_iter().map(Candidate::new).collect::<Vec<_>>();
        let (outcome, provider) = match unless_transient(select_scoped(
            Some(runtime),
            query,
            &source,
            query["selection"].as_str(),
            &candidates,
            None,
            scope,
            unit,
        )) {
            Ok(selected) => selected,
            Err(Failure::Cancelled) => return Err(crate::execution::ExecutionError::Cancelled),
            Err(Failure::Provider(failure)) => {
                row["status"] = json!("error");
                row["data"] = failure.row(provider_page_unfiltered(tool, query));
                continue;
            }
            Err(Failure::Stale) => {
                mark_stale(row, tool, query);
                continue;
            }
        };
        let total = hits.iter().map(Vec::len).sum();
        let shown = outcome.shown(total);
        let withheld_note = withheld_note(&values);
        let (kept, hidden) = keep_shown(values, &hits, &shown, &outcome);
        data["files"] = Value::Array(kept);
        data["selection"] = serde_json::to_value(&outcome.receipt).unwrap_or(Value::Null);
        strip_plan_from_tool_continuations(data, tool);
        // The unfiltered page reaches what selection left out; a page that
        // kept every candidate (or had none, or shows every one because the
        // provider could not answer) leaves nothing out.
        if outcome.leaves_out() {
            set_next(data, "unfiltered", provider_page_unfiltered(tool, query));
        }
        if let Some(token) = outcome.next_candidates() {
            let mut next_query = (**query).clone();
            next_query["selection"] = json!(token);
            set_next(
                data,
                "nextCandidatePage",
                crate::response::result::Continuation::continued(tool, next_query).build(),
            );
        }
        output.finish_selection(data, query, &hidden);
        crate::response::paging::push_warnings(
            data,
            provider
                .as_ref()
                .map(|error| unclassified_warning(error, "this page", "its files are"))
                .into_iter()
                .chain(withheld_note),
        );
        let response_row = if queries.len() == 1 {
            Some(&mut *response_query)
        } else {
            response_query.pointer_mut(&format!("/queries/{admitted_index}"))
        };
        if let Some(response_row) = response_row.and_then(Value::as_object_mut) {
            response_row.insert("selection".into(), json!(outcome.token));
        }
    }
    Ok(())
}

/// The rows holding a `shown` hit, cut to those hits, and the rows judged yet
/// neither shown nor excluded: those holding related or unknown hits.
fn keep_shown(
    values: Vec<Value>,
    hits: &[Vec<provider_hits::Hit>],
    shown: &[usize],
    outcome: &super::Outcome,
) -> (Vec<Value>, Vec<Value>) {
    let mut kept = Vec::with_capacity(values.len());
    let mut hidden = Vec::new();
    let mut ordinal = 0;
    for (item, hits) in values.into_iter().zip(hits) {
        let ordinals = ordinal..ordinal + hits.len();
        ordinal = ordinals.end;
        let chosen = ordinals
            .clone()
            .filter(|at| shown.binary_search(at).is_ok())
            .map(|at| at - ordinals.start)
            .collect::<Vec<_>>();
        let undecided = chosen.is_empty()
            && !withheld(&item)
            && ordinals.clone().any(|at| {
                outcome.considers(at)
                    && !outcome.excluded.contains(&at)
                    && !outcome.unjudged.contains(&at)
            });
        if undecided {
            hidden.push(item.clone());
        }
        kept.extend(provider_hits::keep(item, hits, &chosen));
    }
    (kept, hidden)
}

/// Whether a file's evidence shows any text to judge: a non-blank line or
/// matched value.
fn has_text(item: &Value) -> bool {
    let non_blank = |value: &Value| value.as_str().is_some_and(|text| !text.trim().is_empty());
    item["lines"]
        .as_array()
        .is_some_and(|lines| lines.iter().any(non_blank))
        || item["matches"]
            .as_array()
            .is_some_and(|matches| matches.iter().any(|matched| non_blank(&matched["value"])))
}

/// Whether the classification egress withholds this candidate's file by name:
/// it stays unknown, with nothing of it in any provider request.
fn withheld(item: &Value) -> bool {
    item["path"]
        .as_str()
        .is_some_and(crate::policy::discovery::is_sensitive_repository_file)
}

/// The disclosure of a page's withheld candidates, which no receipt names.
fn withheld_note(items: &[Value]) -> Option<String> {
    let paths = items
        .iter()
        .filter(|item| withheld(item))
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    (!paths.is_empty()).then(|| {
        format!(
            "Not judged: the classification egress withholds credential-named files, so nothing of these was read or sent to the provider: {}.",
            paths.join(", ")
        )
    })
}

/// Set the continuation `key` of a row's data.
fn set_next(data: &mut Value, key: &str, lead: Value) {
    if !data["next"].is_object() {
        data["next"] = json!({});
    }
    data["next"][key] = lead;
}

/// Answer a row whose snapshot went stale with the restart of its query.
fn mark_stale(row: &mut Value, tool: ToolId, query: &Value) {
    let stale = crate::response::result::stale_snapshot(crate::response::result::restart_query(
        tool, query,
    ));
    row["data"] = json!({
        "errorCode":stale["errorCode"],
        "error":crate::response::result::STALE_SNAPSHOT_ERROR,
        "next":stale["next"],
    });
    row["status"] = stale["status"].clone();
}

/// A provider page keeps its page: it is the judged source.
fn provider_page_unfiltered(tool: ToolId, query: &Value) -> Value {
    unfiltered(tool, query, &["page"])
}

fn candidate_source_digest(
    tool: ToolId,
    query: &Value,
    data: &Value,
    states: &[Option<Value>],
) -> String {
    let mut source = json!({
        "tool":tool.as_str(), "path":query.get("path"),
        "owner":query.get("owner"), "repo":query.get("repo"), "page":query.get("page"),
        "sourceOwner":data.get("owner"), "sourceRepo":data.get("repo"),
        "commitSha":data.get("commitSha"),
        "items":states,
    });
    source.sort_all_objects();
    crate::digest::json_sha256(&source)
}

/// Only the response pager replays this plan; tool continuations change the
/// candidate set. Do not traverse arbitrary tool-shaped JSON inside evidence.
pub(super) fn strip_plan_from_tool_continuations(value: &mut Value, tool: ToolId) {
    let Some(fields) = value.as_object_mut() else {
        return;
    };
    if let Some(leads) = fields.get_mut("next").and_then(Value::as_object_mut) {
        for lead in leads.values_mut() {
            scrub_lead(lead, tool, false);
        }
    }
    if let Some(leads) = fields.get_mut("related").and_then(Value::as_array_mut) {
        for lead in leads {
            scrub_lead(lead, tool, true);
        }
    }
    for name in ["files", "matches"] {
        if let Some(items) = fields.get_mut(name).and_then(Value::as_array_mut) {
            for item in items {
                strip_plan_from_tool_continuations(item, tool);
            }
        }
    }
}

fn scrub_lead(lead: &mut Value, tool: ToolId, hint: bool) {
    let Some(fields) = lead.as_object_mut() else {
        return;
    };
    let Some(lead_tool) = fields.get("tool").and_then(Value::as_str) else {
        return;
    };
    let same_tool = lead_tool == tool.as_str();
    let Some(rows) = fields
        .get_mut("input")
        .and_then(|query| query.get_mut("queries"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        row.remove("selection");
        if hint || !same_tool {
            row.remove("classification");
        }
    }
}
