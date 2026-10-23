//! The finish step of a pull-request read: marks an unmatched scope or
//! literal, attaches the continuations and context reads, trims pagination,
//! and builds the metadata row.
use super::patch::hunks::match_context;
use super::patch_hop::attach_full_patch_continuation;
use super::pr_menu::{continuation, fresh_pr_query, remove_key, without_match_string};
use super::promotion::promote_pr_continuations;
use super::pull_request::{FileOutcome, PrPlan, ShapedPr};
use super::util::{nonzero, str_at, string};
use super::window::clamp_warning;
use super::{HistoryItemRequest, Section, Sections};
use crate::id::ToolId;
use crate::response::paging::{mark_partial, push_warnings};
use crate::response::result::Confidence;
use crate::response::result::{ToolData, remove_nulls};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::dates::{utc_date, utc_value};
use crate::tools::gh_shared::ensure_next;
use octocode_contract_types::ExtraKind;
use serde_json::{Map, Value, json};

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
    .confidence(Confidence::High)
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
            .confidence(Confidence::Exact)
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

/// A section read (or a later page) is read for its sections; the summary
/// read owns the rest of the header. It keeps what names the PR and the
/// commits code is read at: `number`, `title`, `author`, `state`, `headSha`
/// and a merged PR's `mergeCommitSha`.
/// The first page of a file inventory also keeps `changedFilesCount`: the
/// denominator of the list it opens, which a one-page list would not state.
pub(super) fn slim_row(row: &mut Value, query: &HistoryItemRequest) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let wants = query.sections();
    let inventory = wants.lists_files()
        && !wants.has(Section::Patches)
        && !gh_get_history_item_query::later_page(query);
    fields.retain(|key, _| {
        matches!(
            key.as_str(),
            "number" | "title" | "author" | "state" | "headSha" | "mergeCommitSha"
        ) || (inventory && key == "changedFilesCount")
    });
}

/// Characters a PR summary's `bodyPreview` shows.
const BODY_PREVIEW_CHARS: usize = 300;

/// The opening of a PR body, whitespace runs collapsed; a cut preview ends
/// with `…` (`readBody` reads it whole). `None` for an empty body.
fn body_preview(body: &str) -> Option<String> {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= BODY_PREVIEW_CHARS {
        return Some(flat);
    }
    let mut preview = flat.chars().take(BODY_PREVIEW_CHARS).collect::<String>();
    preview.push('…');
    Some(preview)
}

pub(super) fn pr_metadata(raw: &Value, query: &HistoryItemRequest) -> Value {
    let merged = raw.get("merged_at").is_some_and(|v| !v.is_null());
    let open = !merged && str_at(raw, "/state").unwrap_or("open") == "open";
    let labels = raw
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| str_at(v, "/name").map(str::to_owned))
        })
        .collect::<Vec<_>>();
    // The body is opt-in (`include:["body"]`, offered by the menu): the
    // summary names the change; a merged PR's close time is its merge time,
    // and only an open PR's last update says whether it is still moving.
    let mut row = json!({
        "number": raw["number"],
        "title": string(raw.get("title")),
        "state": if merged {"merged"} else {str_at(raw,"/state").unwrap_or("open")},
        "draft": raw.get("draft").and_then(Value::as_bool).filter(|v|*v),
        "author": str_at(raw,"/user/login").unwrap_or(""),
        "labels": (!labels.is_empty()).then_some(labels),
        // GraphQL reads the first 20 labels; say when more exist.
        "labelsTruncated": raw.get("labels_truncated").and_then(Value::as_bool).filter(|v| *v),
        "baseBranch": str_at(raw,"/base/ref").filter(|v|!v.is_empty()),
        "headBranch": str_at(raw,"/head/ref").filter(|v|!v.is_empty()),
        "headSha": str_at(raw,"/head/sha").filter(|v|!v.is_empty()),
        // The target-branch tip GitHub recorded for the PR: not the diff's
        // old side (that is the merge base, which `readParent` reads).
        "baseSha": str_at(raw,"/base/sha").filter(|v|!v.is_empty()),
        "createdAt": utc_date(str_at(raw, "/created_at")),
        "updatedAt": open.then(|| utc_date(str_at(raw, "/updated_at"))).filter(|v| !v.is_empty()),
        "closedAt": (!merged).then(|| utc_value(raw.get("closed_at"))),
        "mergedAt": utc_value(raw.get("merged_at")),
        // Only a merged PR has a real merge commit (open PRs expose GitHub's
        // test-merge SHA, which is not on the base branch).
        "mergeCommitSha": merged.then(|| str_at(raw,"/merge_commit_sha")).flatten().filter(|v|!v.is_empty()),
        "commentsCount": nonzero(raw.get("comments")),
        "commitsCount": nonzero(raw.get("commits")),
        // Review threads (GraphQL only): REST counts review comments, a
        // different unit, so a REST read leaves this out.
        "reviewThreadsCount": nonzero(raw.get("review_threads")),
        "changedFilesCount": nonzero(raw.get("changed_files")),
        "additions": nonzero(raw.get("additions")),
        "deletions": nonzero(raw.get("deletions")),
    });
    // A summary previews what the PR says; the menu's body read (`readFiles`,
    // `readPatches` or `readBody`) holds the whole body.
    if query.sections().is_empty()
        && let Some(preview) = str_at(raw, "/body").and_then(body_preview)
    {
        row["bodyPreview"] = json!(preview);
    }
    if gh_get_history_item_query::wants(query, ExtraKind::Details) {
        // Details keep the provider's timestamps whole.
        if let Some(updated) = raw.get("updated_at").filter(|v| v.is_string()) {
            row["updatedAt"] = utc_value(Some(updated));
        }
        if let Some(closed) = raw.get("closed_at").filter(|v| !v.is_null()) {
            row["closedAt"] = utc_value(Some(closed));
        }
    }
    remove_nulls(&mut row);
    row
}

#[cfg(test)]
mod tests {
    use super::super::util::merge;
    use super::*;

    fn summary(raw: Value, details: bool) -> Value {
        let mut row = json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "number":1
        });
        if details {
            row["extras"] = json!(["details"]);
        }
        let query = HistoryItemRequest::from_row(row).expect("query");
        pr_metadata(&raw, &query)
    }

    #[test]
    fn summary_rows_state_each_time_once_and_preview_the_body() {
        let base = json!({"number":1,"title":"t","user":{"login":"a"},"body":"Long description",
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-05T00:00:00Z"});
        let merged = summary(
            merge(
                base.clone(),
                json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
                "closed_at":"2026-01-04T00:00:01Z","merge_commit_sha":"abc"}),
            ),
            false,
        );
        assert_eq!(merged["mergedAt"], "2026-01-04T00:00:00Z", "{merged}");
        for absent in ["closedAt", "updatedAt", "body"] {
            assert!(merged.get(absent).is_none(), "{absent}: {merged}");
        }
        // The summary previews the body; the menu's body read holds it whole.
        assert_eq!(merged["bodyPreview"], "Long description", "{merged}");
        let closed = summary(
            merge(
                base.clone(),
                json!({"state":"closed","merged_at":null,
                "closed_at":"2026-01-04T00:00:00Z"}),
            ),
            false,
        );
        assert_eq!(closed["closedAt"], "2026-01-04T00:00:00Z", "{closed}");
        assert!(closed.get("updatedAt").is_none(), "{closed}");
        let open = summary(merge(base.clone(), json!({"state":"open"})), false);
        assert_eq!(open["updatedAt"], "2026-01-05T00:00:00Z", "{open}");
        // details keep the provider's timestamps whole.
        let details = summary(
            merge(
                base,
                json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
                "closed_at":"2026-01-04T00:00:01Z"}),
            ),
            true,
        );
        assert_eq!(details["closedAt"], "2026-01-04T00:00:01Z", "{details}");
        assert_eq!(details["updatedAt"], "2026-01-05T00:00:00Z", "{details}");
    }

    /// The summary names the change's three commits: the head
    /// (`headSha`), the recorded target-branch tip (`baseSha`) and a
    /// merged PR's merge commit. A section read keeps what names the PR
    /// (number, title, author) and the commits code is read at (`headSha`,
    /// `mergeCommitSha`), nothing else.
    #[test]
    fn every_pr_view_keeps_source_target_and_merge_commits() {
        let raw = json!({"number":1,"title":"t","user":{"login":"a"},"state":"closed",
            "merged_at":"2026-01-04T00:00:00Z","merge_commit_sha":"m1",
            "head":{"sha":"h1","ref":"feat"},"base":{"sha":"b1","ref":"main"},
            "created_at":"2026-01-01T00:00:00Z","changed_files":3,"additions":5,"deletions":1,
            "commits":4,"review_threads":2});
        let row = summary(raw.clone(), false);
        assert_eq!(
            (&row["headSha"], &row["baseSha"], &row["mergeCommitSha"]),
            (&json!("h1"), &json!("b1"), &json!("m1")),
            "{row}"
        );
        assert_eq!(row["commitsCount"], 4, "{row}");
        assert_eq!(row["reviewThreadsCount"], 2, "{row}");
        for fields in [
            json!({"sections":["patches"]}),
            json!({"sections":["files"]}),
            json!({"sections":["patches"],"matchString":"x"}),
            json!({"sections":["patches"],"page":2}),
        ] {
            let query = HistoryItemRequest::from_row(merge(
                json!({"operation":"pullRequest","owner":"o","repo":"r","number":1}),
                fields.clone(),
            ))
            .expect("query");
            let mut row = pr_metadata(&raw, &query);
            slim_row(&mut row, &query);
            let mut expected = json!({"number":1,"title":"t","author":"a","state":"merged",
                "headSha":"h1","mergeCommitSha":"m1"});
            // Only the first page of a file inventory keeps the count the
            // list it opens is measured against.
            if fields == json!({"sections":["files"]}) {
                expected["changedFilesCount"] = json!(3);
            }
            assert_eq!(row, expected, "{fields}");
        }
        // An open PR has no merge commit; REST has no review-thread total.
        let open = summary(
            json!({"number":1,"state":"open","head":{"sha":"h1"},"base":{"sha":"b1"},
                "merge_commit_sha":"test-merge","review_comments":9}),
            false,
        );
        assert!(open.get("mergeCommitSha").is_none(), "{open}");
        assert!(open.get("reviewThreadsCount").is_none(), "{open}");
        assert_eq!(open["baseSha"], "b1", "{open}");
    }
}
