//! The finish step of a pull-request read: marks an unmatched scope or
//! literal, attaches the continuations and context reads, trims pagination,
//! and builds the metadata row.
use super::patch::hunks::match_context;
use super::patch_hop::attach_full_patch_continuation;
use super::pr_menu::{continuation, fresh_pr_query, remove_key, without_match_string};
use super::promotion::promote_pr_continuations;
use super::pull_request::{FileOutcome, PrPlan, ShapedPr};
use super::util::str_at;
use super::window::clamp_warning;
use super::{HistoryItemRequest, Section, Sections};
use crate::id::ToolId;
use crate::response::paging::{mark_partial, push_warnings};
use crate::response::result::ToolData;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::ensure_next;
use octocode_contract_types::ExtraKind;
use serde_json::{Map, Value, json};

mod metadata;
#[cfg(test)]
mod tests;

pub(super) use metadata::{pr_metadata, slim_row};

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
    let literal = gh_get_history_item_query::match_string(query).filter(|_| plan.needle.is_some());
    let literal_warning = literal
        .filter(|_| files.in_scope > 0)
        .map(|literal| super::filter::match_removed_all_warning(literal, files.in_scope));
    if literal.is_some() && plan.scope.is_none() && other_evidence {
        return literal_warning;
    }
    push_warnings(out, literal_warning);
    out["status"] = json!("empty");
    ensure_next(out);
    if let Some(next) = out["next"].as_object_mut() {
        next.extend(unmatched_leads(query, literal.is_some()));
    }
    None
}

/// The reads that recover a pull-request read whose literal or file scope
/// matched no changed file, in rank order: the same read without the
/// `matchString` (when the literal missed), then the whole file inventory
/// (`include`/`status`/`minChanges` dropped) to copy a path from.
fn unmatched_leads(query: &HistoryItemRequest, literal_missed: bool) -> Map<String, Value> {
    let target = fresh_pr_query(query);
    let mut leads = Map::new();
    if literal_missed {
        let mut same = target.clone();
        query.sections().set_on(&mut same);
        leads.insert("withoutMatchString".into(), without_match_string(same));
    }
    let mut inventory = target;
    for key in ["include", "status", "minChanges", "contextLines"] {
        remove_key(&mut inventory, key);
    }
    Sections::of([Section::Files]).set_on(&mut inventory);
    leads.insert("readFiles".into(), continuation(inventory));
    leads
}

pub(super) fn finish_pr(
    query: &HistoryItemRequest,
    wants: &Sections,
    plan: &PrPlan,
    shaped: ShapedPr,
    graphql_fallback: Option<String>,
) -> ToolData {
    let ShapedPr {
        row,
        files,
        menu,
        comment_reads,
        remainder,
        commit_file_pages,
    } = shaped;
    let patches = wants.has(Section::Patches);
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
    let needle_missed = mark_unmatched(&mut out, query, plan, &files, other_evidence);
    if !files.unsearched.is_empty() {
        out["pullRequests"][0]["unsearchedFiles"] = json!(files.unsearched);
    }
    if (patches || plan.needle.is_some())
        && let Some(warning) = clamp_warning(query)
    {
        push_warnings(&mut out, [warning]);
    }
    if let Some(warning) = needle_missed {
        push_warnings(&mut out, [warning]);
    }
    if let Some((dropped, listed)) = files.match_dropped {
        push_warnings(
            &mut out,
            [super::filter::match_dropped_warning(dropped, listed)],
        );
    }
    let remaining = promote_pr_continuations(&mut out, query, &remainder);
    if !commit_file_pages.is_empty() {
        out["isPartial"] = json!(true);
        ensure_next(&mut out);
        if let Some(next) = out["next"].as_object_mut() {
            next.extend(commit_file_pages);
        }
    }
    if out.pointer("/providerLimit/reason").and_then(Value::as_str)
        == Some("providerCommitListLimit")
    {
        push_warnings(&mut out, [
            "GitHub ended this PR's commit list before commitsCount; inspect a local checkout with sufficient history.".to_owned()
        ]);
    }
    attach_full_patch_continuation(&mut out, query);
    if let Some((path, sha)) = &files.first_unsearched {
        attach_unsearched_read(&mut out, query, path, sha.as_deref());
    }
    attach_context_reads(&mut out, query, &files.clipped);
    // A patch read's next step is the code at either side of its numbered
    // diff: those reads rank ahead of the section menu.
    let merge_read = files.merge_read.filter(|_| patches);
    attach_leads(&mut out, files.side_reads, menu, merge_read);
    lead_with_comment_reads(&mut out, comment_reads);
    if !gh_get_history_item_query::wants(query, ExtraKind::Details) {
        trim_content_pagination(&mut out);
    }
    if gh_get_history_item_query::wants(query, ExtraKind::Diagnostics)
        && let Some(reason) = graphql_fallback
    {
        out["graphqlFallback"] = json!(reason);
    }
    ToolData {
        remaining,
        ..ToolData::from(out)
    }
}

/// The row's next steps ride the response's leads (one capped list), never
/// a second list nested in the row; the contract's lead priority keeps every
/// lead under the lead cap. The side reads of the numbered diff come first,
/// then the section menu, then the merge-commit read; a page continuation
/// already under a name keeps it.
fn attach_leads(
    out: &mut Value,
    side_reads: Vec<(&'static str, Value)>,
    menu: Option<Value>,
    merge_read: Option<Value>,
) {
    let menu = match menu {
        Some(Value::Object(menu)) => menu,
        _ => Map::new(),
    };
    let leads = side_reads
        .into_iter()
        .map(|(name, read)| (name.to_owned(), read))
        .chain(menu)
        .chain(merge_read.map(|read| ("readAtMerge".to_owned(), read)));
    for (name, lead) in leads {
        ensure_next(out);
        if let Some(next) = out["next"].as_object_mut() {
            next.entry(name).or_insert(lead);
        }
    }
}

/// The review comments' own leads go before the menu's: they are the reads
/// the comment page asked for, ten anchor ranges each.
fn lead_with_comment_reads(out: &mut Value, comment_reads: Vec<Value>) {
    if comment_reads.is_empty() {
        return;
    }
    ensure_next(out);
    if let Some(next) = out["next"].as_object_mut() {
        for (index, read) in comment_reads.into_iter().enumerate().rev() {
            next.shift_insert(0, crate::id::numbered("readCommentCode", index), read);
        }
    }
}

/// A `matchString` search that skipped patchless files covers only part of
/// the diff (a pull request's, a commit's or a comparison's): mark it
/// partial and offer the literal search of the first skipped file's source
/// at `sha` (the base for a removed file, else the head; a template for the
/// rest).
pub(super) fn attach_unsearched_read(
    out: &mut Value,
    query: &HistoryItemRequest,
    path: &str,
    sha: Option<&str>,
) {
    mark_partial(out, "patchUnavailable", &[]);
    let (Some(needle), Some(sha)) = (gh_get_history_item_query::match_string(query), sha) else {
        return;
    };
    ensure_next(out);
    out["next"]["searchUnpatchedFile"] = crate::response::result::Continuation::new(
        ToolId::GhGetFileContent,
        json!({
            "owner": gh_get_history_item_query::owner(query),
            "repo": gh_get_history_item_query::repo(query),
            "path": path,
            "ref": sha,
            "matchString": needle,
        }),
    )
    .build();
}

/// `matchString` context the hunks cut short and the head text did not
/// fill: the row is partial, and `next.expandContext` reads each file's
/// wanted windows at the PR head (`headSha`).
fn attach_context_reads(
    out: &mut Value,
    query: &HistoryItemRequest,
    clipped: &[(String, Vec<String>)],
) {
    let Some(sha) = str_at(out, "/pullRequests/0/headSha").map(str::to_owned) else {
        return;
    };
    if clipped.is_empty() {
        return;
    }
    mark_partial(
        out,
        "contextClipped",
        &[format!(
            "contextLines {} reaches past the diff hunks of {} file(s) (contextClipped).",
            match_context(query),
            clipped.len()
        )],
    );
    ensure_next(out);
    for (index, (path, ranges)) in clipped.iter().enumerate() {
        out["next"][crate::id::numbered("expandContext", index)] =
            crate::response::result::Continuation::new(
                ToolId::GhGetFileContent,
                json!({
                    "owner": gh_get_history_item_query::owner(query),
                    "repo": gh_get_history_item_query::repo(query),
                    "path": path,
                    "ref": sha,
                    "ranges": ranges,
                }),
            )
            .build();
    }
}

/// Default responses drop pagination that adds nothing once `next.*` is
/// built: a finished single-page list or whole text window, and the patch
/// cursor, which the cut file's own `patchPagination` already carries.
/// Provider limits stay.
fn trim_content_pagination(out: &mut Value) {
    let Some(row) = out
        .pointer_mut("/pullRequests/0")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(pages) = row
        .get_mut("contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    pages.retain(|_, page| {
        let done =
            page.get("hasMore") == Some(&Value::Bool(false)) && page.get("terminalLimit").is_none();
        // A finished first page, or a whole text window (offset zero).
        let first = page.get("currentPage").and_then(Value::as_u64) == Some(1)
            || page.get("offset").and_then(Value::as_u64) == Some(0);
        !(done && first)
    });
    if pages
        .get("patches")
        .is_some_and(|patches| patches.get("hasMore") != Some(&Value::Bool(true)))
    {
        pages.remove("patches");
    }
    if let Some(patches) = pages.get_mut("patches").and_then(Value::as_object_mut) {
        patches.retain(|key, _| key == "hasMore");
    }
    if pages.is_empty() {
        row.remove("contentPagination");
    }
    lift_terminal_limits(out);
}

/// A page's terminal limit is restated on the row (`terminalLimit`,
/// `partialReasons`); the page schema does not declare it.
fn lift_terminal_limits(out: &mut Value) {
    let Some(row) = out.pointer_mut("/pullRequests/0") else {
        return;
    };
    let limited: Vec<Value> = row
        .get_mut("contentPagination")
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
