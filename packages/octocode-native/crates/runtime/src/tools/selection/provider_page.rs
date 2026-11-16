//! The provider-page selection: classified rows of a tool whose provider
//! returns its own pages of files (ghSearchCode). The page is the judged
//! source and each of its hits a candidate; its unfiltered lead keeps the page.
use super::provider_hits;
use super::row_units::Rows;
use super::{
    Candidate, Failure, Runtime, Unclassified, select_scoped, unclassified_notice, unfiltered,
    unless_transient,
};
use crate::id::ToolId;
use crate::response::selection_errors::unavailable;
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
        tool.selection_receipt(),
        structured.get_mut("results").and_then(Value::as_array_mut),
    ) else {
        return Ok(());
    };
    let cx = RowContext {
        tool,
        output,
        runtime,
        scope,
        unit,
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
        let response_at = (queries.len() != 1).then_some(admitted_index);
        select_row(&cx, row, query, response_query, response_at)?;
    }
    Ok(())
}

/// What every row of one page's selection shares.
struct RowContext<'a> {
    tool: ToolId,
    output: &'a dyn crate::response::output::ToolOutput,
    runtime: &'a Runtime,
    scope: octocode_contract_types::SelectionScope,
    unit: octocode_contract_types::SelectionUnit,
}

/// Select one classified row of the page; `response_at` places its query in
/// a multi-row response query (`None`: the response query is the row's).
/// Whatever the outcome, text only the judge read leaves the row.
fn select_row(
    cx: &RowContext<'_>,
    row: &mut Value,
    query: &crate::security::CheckedQuery,
    response_query: &mut Value,
    response_at: Option<usize>,
) -> Result<(), crate::execution::ExecutionError> {
    let selected = judge_row(cx, row, query, response_query, response_at);
    cx.output.strip_judge_only(&mut row["data"], query);
    selected
}

fn judge_row(
    cx: &RowContext<'_>,
    row: &mut Value,
    query: &crate::security::CheckedQuery,
    response_query: &mut Value,
    response_at: Option<usize>,
) -> Result<(), crate::execution::ExecutionError> {
    let RowContext {
        tool,
        output,
        runtime,
        scope,
        unit,
    } = *cx;
    let unavailable_receipt = |total: usize, reason: &str| {
        serde_json::to_value(unavailable(scope, unit, total, reason)).unwrap_or(Value::Null)
    };
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
        crate::response::result::set_next(
            data,
            "unfiltered",
            provider_page_unfiltered(tool, query),
        );
        return Ok(());
    }
    if !data.is_object() {
        *data = json!({});
    }
    // Code search judges the hits of its files; every other classified
    // tool judges its listed rows (`Rows`), which stay in the data.
    let rows = (tool != ToolId::GhSearchCode).then(|| Rows::of(tool, data));
    let taken = match rows {
        Some(_) => Some(Value::Array(Vec::new())),
        None => data.get_mut("files").map(Value::take),
    };
    let Some(Value::Array(values)) = taken else {
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
        return Ok(());
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
    let states = match &rows {
        Some(rows) => rows
            .states()
            .into_iter()
            .map(|state| state.map(|state| runtime.judge.project(state)))
            .collect::<Vec<_>>(),
        None => values
            .iter()
            .zip(&hits)
            .flat_map(|(item, hits)| {
                hits.iter().map(move |hit| {
                    provider_hits::state(item, hit).map(|state| runtime.judge.project(state))
                })
            })
            .collect::<Vec<_>>(),
    };
    let total = states.len();
    let source = candidate_source_digest(tool, query, data, &states);
    let candidates = states.into_iter().map(Candidate::new).collect::<Vec<_>>();
    let (outcome, provider) = match unless_transient(select_scoped(
        Some(runtime),
        tool,
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
            return Ok(());
        }
        Err(Failure::Stale) => {
            mark_stale(row, tool, query);
            return Ok(());
        }
    };
    let shown = outcome.shown(total);
    // An undecided view lists the units its walk left unknown, unfiltered:
    // they are not selected evidence, so it carries no receipt and no lead.
    let view = outcome.lists_undecided();
    let disclosed = if view {
        Some(super::undecided_note(shown.len()))
    } else {
        super::kept_note(&outcome, shown.len())
    };
    let (notes, hidden, removed, unread_sections) = match &rows {
        Some(rows) => {
            let (notes, unread_sections) = rows.notes(tool, query, data);
            let removed = rows.retain(data, &shown);
            (notes, Vec::new(), removed, unread_sections)
        }
        None => {
            let note = withheld_note(&values);
            let (kept, hidden) = keep_shown(values, &hits, &shown, &outcome);
            data["files"] = Value::Array(kept);
            (note.into_iter().collect(), hidden, Vec::new(), false)
        }
    };
    if !view {
        data["selection"] = receipt(&outcome, unread_sections);
    }
    strip_plan_from_tool_continuations(data, tool);
    set_leads(data, tool, query, &outcome);
    output.finish_selection(data, query, &hidden, &removed);
    crate::response::paging::push_warnings(
        data,
        provider
            .as_ref()
            .map(|error| unclassified_notice(error, Unclassified::Page))
            .into_iter()
            .chain(disclosed)
            .chain(outcome.long_context_notice.clone())
            .chain(notes),
    );
    let response_row = match response_at {
        None => Some(&mut *response_query),
        Some(admitted_index) => response_query.pointer_mut(&format!("/queries/{admitted_index}")),
    };
    if let Some(response_row) = response_row.and_then(Value::as_object_mut) {
        response_row.insert("selection".into(), json!(outcome.token));
    }

    Ok(())
}

/// The leads of a classified page. The unfiltered page reaches what selection
/// left out (a page that kept every candidate, had none, or shows every one
/// because the provider could not answer leaves nothing out); then the same
/// page replaying the selection, unbilled: the units judged later, and
/// exactly the units the walk left unknown. An undecided view is that list
/// itself and leads nowhere.
fn set_leads(
    data: &mut Value,
    tool: ToolId,
    query: &crate::security::CheckedQuery,
    outcome: &super::Outcome,
) {
    if outcome.lists_undecided() {
        return;
    }
    // A page the provider failed shows every unit already: only a page that
    // left units out leads to the whole or the undecided ones.
    let leaves_out = outcome.leaves_out();
    if leaves_out {
        crate::response::result::set_next(
            data,
            "unfiltered",
            provider_page_unfiltered(tool, query),
        );
    }
    let replays = [
        (
            "nextCandidatePage",
            outcome.next_candidates().map(str::to_owned),
        ),
        (
            "unfilteredUnknown",
            outcome.undecided_view().filter(|_| leaves_out),
        ),
    ];
    for (channel, token) in replays {
        if let Some(token) = token
            && let Some(call) = crate::response::collection::continued_page(
                tool,
                (**query).clone(),
                "selection",
                token,
                &[],
            )
        {
            crate::response::result::set_next(data, channel, call.build());
        }
    }
}

/// The row's selection receipt. A read that held none of the entries this
/// tool judges, while its warnings name the sections that hold them, says so:
/// no hit was found wanting.
fn receipt(outcome: &super::Outcome, unread_sections: bool) -> Value {
    let mut receipt = serde_json::to_value(&outcome.receipt).unwrap_or(Value::Null);
    if unread_sections && receipt["reason"] == "noCandidates" {
        receipt["reason"] = json!("unreadSections");
    }
    receipt
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
    item["matches"].as_array().is_some_and(|matches| {
        matches.iter().any(|matched| {
            matched["value"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
        })
    })
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
    withheld_paths_note(&paths)
}

/// The one disclosure of candidates the classification egress withholds.
pub(super) fn withheld_paths_note<S: AsRef<str>>(paths: &[S]) -> Option<String> {
    (!paths.is_empty()).then(|| {
        let paths = paths.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        format!(
            "Not judged: the classification egress withholds credential-named files, so nothing of these was read or sent to the provider: {}.",
            paths.join(", ")
        )
    })
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

/// A provider page keeps its page: it is the judged source, so the lead
/// carries every position field the tool's own continuations use (its
/// contract's `x-cursor` fields) and drops only the selection plan.
fn provider_page_unfiltered(tool: ToolId, query: &Value) -> Value {
    let position = crate::contracts::state_fields(tool)
        .iter()
        .map(String::as_str)
        .filter(|field| *field != "selection")
        .collect::<Vec<_>>();
    unfiltered(tool, query, &position)
}

fn candidate_source_digest(
    tool: ToolId,
    query: &Value,
    data: &Value,
    states: &[Option<Value>],
) -> String {
    // A field stated at its contract default (`page:1`) is its omission, as
    // the continuation that drops it runs.
    let mut canonical = query.clone();
    crate::response::result::canonical_row(tool, &mut canonical);
    let query = &canonical;
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
    // A collection's page cursor rides its `sectionPagination` block.
    if let Some(blocks) = fields
        .get_mut("sectionPagination")
        .and_then(Value::as_object_mut)
    {
        for block in blocks.values_mut() {
            if let Some(leads) = block.get_mut("next").and_then(Value::as_object_mut) {
                for lead in leads.values_mut() {
                    scrub_lead(lead, tool, false);
                }
            }
        }
    }
    for name in ["files", "matches", "pullRequests", "issues"] {
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
