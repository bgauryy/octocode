//! The finish step of a pull-request read: marks an unmatched scope or
//! literal, attaches the continuations and context reads, trims pagination,
//! and builds the metadata row.
use super::patch::hunks::match_context;
use super::pr_menu::{fresh_pr_query, remove_key, without_match_string};
use super::promotion::promote_pr_continuations;
use super::pull_request::{FileOutcome, PrPlan, ShapedPr};
use super::util::str_at;
use super::{HistoryItemRequest, Sections};
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::sections::Section;
use octocode_contract_types::{
    GhGetFileContentQuery, GhGetHistoryItemChannel, GhGetHistoryItemQuery,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::paging::{mark_partial, push_warnings};
use octocode_runtime_base::response::result::wire_query;
use octocode_runtime_base::response::result::{Continuation, ToolData, insert_next};
use octocode_tools_shared::gh_shared::ensure_calls;
use serde_json::{Map, Value, json};

mod metadata;
#[cfg(test)]
mod tests;

pub(super) use metadata::{BOT_ENTRIES_KEY, WHOLE_COUNT_KEY, body_preview, pr_metadata};

/// A scope or literal that matched nothing: an empty row whose leads run the
/// recovery ([`unmatched_leads`]), or, when other sections hold evidence, a
/// warning. A literal that hit none of the files in scope says so in the
/// warning a commit or comparison read gives.
fn mark_unmatched(
    out: &mut Value,
    query: &HistoryItemRequest,
    plan: &PrPlan,
    files: &FileOutcome,
    other_evidence: bool,
) -> Option<String> {
    if !files.scope_unmatched || !files.unsearched.is_empty() {
        return None;
    }
    let literal = gh_get_history_item_query::match_strings(query).filter(|_| plan.needle.is_some());
    let literal_warning = literal
        .filter(|_| files.in_scope > 0)
        .map(|_| super::filter::match_removed_all_warning(files.in_scope));
    if literal.is_some() && plan.scope.is_none() && other_evidence {
        return literal_warning;
    }
    push_warnings(out, literal_warning);
    push_warnings(out, files.scope_warning.clone());
    out["status"] = json!("empty");
    ensure_calls(out);
    if let Some(next) = out["calls"].as_object_mut() {
        next.extend(unmatched_leads(query, literal.is_some()));
    }
    None
}

/// The same pull-request read without its `matchString`: the recovery of a
/// literal that dropped entries.
fn without_match_lead(query: &HistoryItemRequest) -> Value {
    let mut same = fresh_pr_query(query);
    query.sections().set_on(&mut same);
    without_match_string(same)
}

/// The reads that recover a pull-request read whose literal or file scope
/// matched no changed file, in rank order: the same read without the
/// `matchString` (when the literal missed), then, when an `include` or
/// `fileStatus` scope narrowed the files, the whole file inventory (the scope
/// dropped) to copy a path from. Without a scope that inventory is the first
/// read less its patches.
fn unmatched_leads(query: &HistoryItemRequest, literal_missed: bool) -> Map<String, Value> {
    let mut leads = Map::new();
    if literal_missed {
        leads.insert("withoutMatchString".into(), without_match_lead(query));
    }
    if !gh_get_history_item_query::has_file_filter(query) {
        return leads;
    }
    let mut inventory = fresh_pr_query(query);
    for key in ["include", "fileStatus", "contextLines"] {
        remove_key(&mut inventory, key);
    }
    Sections::of([Section::Files]).set_on(&mut inventory);
    if let Some(read) = wire_query::<GhGetHistoryItemQuery>(inventory) {
        leads.insert(
            "readFiles".into(),
            Continuation::related_query(read).build(),
        );
    }
    leads
}

pub(super) fn finish_pr(
    query: &HistoryItemRequest,
    wants: &Sections,
    plan: &PrPlan,
    shaped: ShapedPr,
) -> ToolData {
    let ShapedPr {
        row,
        files,
        menu,
        comment_reads,
        commit_file_pages,
        body_drops,
        notes,
    } = shaped;
    // A `matchString` miss in the files is an empty row only when no other
    // section of the read (body, comments, reviews) holds evidence;
    // otherwise the row says so in a warning.
    let other_evidence = wants.has(Section::Body)
        || ["comments", "reviewComments", "reviews"].iter().any(|key| {
            row.get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        });
    let mut out = json!({"pullRequests":[row]});
    lift_terminal_limits(&mut out);
    let needle_missed = mark_unmatched(&mut out, query, plan, &files, other_evidence);
    if !files.unsearched.is_empty() {
        out["pullRequests"][0]["unsearchedFiles"] = json!(files.unsearched);
    }
    if let Some(warning) = needle_missed {
        push_warnings(&mut out, [warning]);
    }
    if let Some((dropped, listed)) = files.match_dropped {
        push_warnings(
            &mut out,
            [super::filter::match_dropped_warning(
                dropped,
                listed,
                "changed files",
                "patch line",
            )],
        );
    }
    push_warnings(&mut out, notes);
    // What the literal dropped from the body and discussion is counted; the
    // row is evidence, so it offers no read without the literal.
    push_warnings(&mut out, body_drops);
    promote_pr_continuations(&mut out, query);
    if !commit_file_pages.is_empty() {
        octocode_runtime_base::response::paging::set_partial(&mut out, true);
        ensure_calls(&mut out);
        if let Some(next) = out["calls"].as_object_mut() {
            next.extend(commit_file_pages);
        }
    }
    if out.pointer("/providerLimit/reason").and_then(Value::as_str)
        == Some("providerCommitListLimit")
    {
        push_warnings(&mut out, [
            "GitHub ended this PR's commit list before commitCount; inspect a local checkout with sufficient history.".to_owned()
        ]);
    }
    if !files.leads_unsearched.is_empty() {
        attach_unsearched_reads(&mut out, query, &files.leads_unsearched);
    }
    attach_context_reads(&mut out, query, &files.clipped);
    // A patch read's next step is the code at either side of its numbered
    // diff: those reads rank ahead of the section menu.
    // A miss already leads to its recovery reads; the menu would offer the
    // whole-PR patches the miss was narrowing away from.
    let menu = menu.filter(|_| out["status"] != "empty");
    attach_leads(&mut out, files.side_reads, menu);
    lead_with_comment_reads(&mut out, comment_reads);
    ToolData::from(out)
}

/// The row's next steps ride the response's leads (one capped list), never
/// a second list nested in the row; the contract's lead priority keeps every
/// lead under the lead cap. The side reads of the numbered diff come first,
/// then the section menu; a page continuation
/// already under a name keeps it.
fn attach_leads(out: &mut Value, side_reads: Vec<(String, Value)>, menu: Option<Value>) {
    let menu = match menu {
        Some(Value::Object(menu)) => menu,
        _ => Map::new(),
    };
    let leads = side_reads.into_iter().chain(menu);
    for (name, lead) in leads {
        ensure_calls(out);
        if let Some(next) = out["calls"].as_object_mut() {
            next.entry(name).or_insert(lead);
        }
    }
}

/// Optional source reads for review anchors precede the section menu,
/// ten anchor ranges each.
fn lead_with_comment_reads(out: &mut Value, comment_reads: Vec<Value>) {
    if comment_reads.is_empty() {
        return;
    }
    ensure_calls(out);
    if let Some(next) = out["calls"].as_object_mut() {
        for (index, read) in comment_reads.into_iter().enumerate().rev() {
            next.shift_insert(
                0,
                octocode_runtime_base::id::numbered("readCommentCode", index),
                read,
            );
        }
    }
}

/// A `matchString` search that skipped patchless files covers only part of
/// the diff (a pull request's, a commit's or a comparison's): mark it
/// partial and offer the literal search of every skipped file's source
/// at its sha (the base for a removed file, else the head), as many calls
/// as the queries limit needs.
pub(super) fn attach_unsearched_reads(
    out: &mut Value,
    query: &HistoryItemRequest,
    files: &[(String, Option<String>)],
) {
    mark_partial(
        out,
        octocode_runtime_base::response::paging::PartialReason::PatchUnavailable,
        &[],
    );
    let Some(needle) = gh_get_history_item_query::match_strings(query) else {
        return;
    };
    let reads = files
        .iter()
        .filter_map(|(path, sha)| {
            let sha = sha.as_deref()?;
            wire_query::<GhGetFileContentQuery>(json!({
                "owner": gh_get_history_item_query::owner(query),
                "repo": gh_get_history_item_query::repo(query),
                "path": path,
                "ref": sha,
                "matchString": needle,
                "regexEngine":gh_get_history_item_query::match_options(query).map(|o|o.1)}))
        })
        .collect::<Vec<_>>();
    if reads.is_empty() {
        return;
    }
    ensure_calls(out);
    let per_call =
        octocode_runtime_base::response::result::rows_per_call(ToolId::GhGetFileContent).max(1);
    let channel = GhGetHistoryItemChannel::SearchUnpatchedFile;
    for (index, chunk) in reads.chunks(per_call).enumerate() {
        let rows = chunk
            .iter()
            .filter_map(|read| serde_json::to_value(read).ok())
            .collect::<Vec<_>>();
        let call =
            Continuation::continued_input(ToolId::GhGetFileContent, json!({"queries": rows}));
        if index == 0 {
            insert_next(&mut out["calls"], channel, call);
        } else {
            out["calls"][octocode_runtime_base::id::numbered(channel.key(), index)] = call.build();
        }
    }
}

/// `matchString` context the hunks cut short and the head text did not
/// fill: the row is partial, and `next.expandContext` reads each file's
/// wanted windows at the PR head (`headCommitSha`).
fn attach_context_reads(
    out: &mut Value,
    query: &HistoryItemRequest,
    clipped: &[(String, Vec<(usize, usize)>)],
) {
    let Some(sha) = str_at(out, "/pullRequests/0/headCommitSha").map(str::to_owned) else {
        return;
    };
    if clipped.is_empty() {
        return;
    }
    mark_partial(
        out,
        octocode_runtime_base::response::paging::PartialReason::ContextClipped,
        &[format!(
            "contextLines {:?} reaches past the diff hunks of {} file(s) (contextClipped).",
            match_context(query),
            clipped.len()
        )],
    );
    ensure_calls(out);
    for (index, (path, ranges)) in clipped.iter().enumerate() {
        let Some(read) = wire_query::<GhGetFileContentQuery>(json!({
            "owner": gh_get_history_item_query::owner(query),
            "repo": gh_get_history_item_query::repo(query),
            "path": path,
            "ref": sha,
            "ranges": ranges.iter().map(|(start, end)| json!({"startLine":start,"endLine":end})).collect::<Vec<_>>()}))
        else {
            continue;
        };
        out["calls"][octocode_runtime_base::id::numbered("expandContext", index)] =
            Continuation::continued_query(read).build();
    }
}

/// A page's terminal limit is restated on the row (`terminalLimit`,
/// `partialReasons`); the page schema does not declare it.
fn lift_terminal_limits(out: &mut Value) {
    let Some(row) = out.pointer_mut("/pullRequests/0") else {
        return;
    };
    let limited: Vec<Value> = row
        .get_mut("sectionPagination")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|pages| pages.values_mut())
        .filter_map(Value::as_object_mut)
        .filter_map(|page| {
            (page.remove("terminalLimit") == Some(Value::Bool(true)))
                .then(|| json!({"terminalLimit": true, "providerLimit": page.get("providerLimit")}))
        })
        .collect();
    for mut page in limited {
        super::window::lift_page_limits(row, &mut page);
    }
}
