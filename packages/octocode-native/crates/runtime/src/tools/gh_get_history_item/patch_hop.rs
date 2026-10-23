//! Patch hops: the whole-patch re-read, selected-patch continuations, and
//! commit/comparison file-page cursors.
use super::inventory::file_page_size;
use super::pr_menu::*;
use super::{HistoryItemRequest, ItemOperation, Section, Sections};
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, query_row};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::commit::commit_pull_request;
use crate::tools::gh_shared::ensure_next;
use serde_json::{Map, Value, json};

/// The whole-patch re-read of every patch row a `matchString` view narrowed
/// (hit lines, clipped long lines; rows marked `fullPatchChars`, kept on
/// rows only with `extras: ["details"]`): the opt-in lead `readFullPatches`
/// reads the selected files whole; more files than one selection holds
/// continue in `readFullPatches2`, `readFullPatches3`, …. The grep showed
/// what was asked, so the lead yields to code reads (`confidence: "low"`).
pub(super) fn attach_full_patch_continuation(out: &mut Value, q: &HistoryItemRequest) {
    let narrowed = take_reshaped_paths(
        out.pointer_mut("/pullRequests/0/files"),
        gh_get_history_item_query::wants(q, octocode_contract_types::ExtraKind::Details),
    );
    if narrowed.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in ["offset", "length", "page"] {
        remove_key(&mut nq, key);
    }
    ensure_next(out);
    remove_key(&mut nq, "matchString");
    remove_key(&mut nq, "contextLines");
    // Each read names its files in the public `include` beside `patches`.
    for (index, files) in narrowed.chunks(selected_patch_files()).enumerate() {
        let read = super::util::merge(
            nq.clone(),
            json!({"include": files, "sections": ["patches"]}),
        );
        let mut lead = continuation(read);
        lead["confidence"] = json!("low");
        out["next"][crate::id::numbered("readFullPatches", index)] = lead;
    }
}

/// Paths of the rows marked `fullPatchChars` (a view that is not the raw
/// patch). The marker stays on rows only with `debug`.
pub(super) fn take_reshaped_paths(rows: Option<&mut Value>, debug: bool) -> Vec<String> {
    let mut paths = Vec::new();
    for file in rows.and_then(Value::as_array_mut).into_iter().flatten() {
        let Some(fields) = file.as_object_mut() else {
            continue;
        };
        let marked = if debug {
            fields.contains_key("fullPatchChars")
        } else {
            fields.remove("fullPatchChars").is_some()
        };
        if marked && let Some(path) = fields.get("path").and_then(Value::as_str) {
            paths.push(path.to_owned());
        }
    }
    paths
}

/// Paths one selected-patch read names: the contract's pull-request
/// `include` `maxItems`.
#[expect(
    clippy::panic,
    reason = "a missing bound is a contract defect caught by the contract tests, never a request-time failure"
)]
pub(super) fn selected_patch_files() -> usize {
    crate::contracts::query_schema_number(
        ToolId::GhGetHistoryItem,
        Some("pullRequest"),
        "include",
        "maxItems",
    )
    .and_then(|maximum| usize::try_from(maximum).ok())
    .unwrap_or_else(|| panic!("contract declares no maxItems for ghGetHistoryItem include"))
}

/// The patch continuation of a pull-request page: the same query (file
/// page, filters, selection) at the page-stream cursor, narrowed to the
/// patch surface.
pub(super) fn patch_continuation(mut nq: Value, entry: &Value, q: &HistoryItemRequest) -> Value {
    let read = q.sections();
    let patch_read = Sections::of([Section::Patches]);
    if read.has(Section::Files) {
        patch_read.with(Section::Files)
    } else {
        patch_read
    }
    .set_on(&mut nq);
    if let Some(cursor) = entry.get("nextOffset") {
        nq["offset"] = cursor.clone();
    }
    clamp_hop_length(&mut nq, q);
    continuation(nq)
}

/// A file-page continuation carries the page size its grid was cut on when
/// the read sized it from the response budget
/// ([`super::inventory::pinned_file_page_size`]), so it continues the same
/// file list in any batch.
pub(super) fn pin_file_page_size(nq: &mut Value, q: &HistoryItemRequest, patches: bool) {
    if let Some(size) = super::inventory::pinned_file_page_size(q, patches) {
        nq["pageSize"] = json!(size);
    }
}

/// A hop carries the caller's `length` as the window it got, so a length
/// clamped to the response page is said once, on the call that asked.
pub(super) fn clamp_hop_length(nq: &mut Value, q: &HistoryItemRequest) {
    if let Some(length) = gh_get_history_item_query::char_length(q) {
        nq["length"] = json!(super::window::patch_window(Some(length), q.auto_page_chars));
    }
}

/// The page state a commit or comparison read leaves to continue, as the
/// shaping code computed it (never re-read from the shaped output).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DiffCursors {
    /// The next comparison commit page.
    pub(super) commit_page: Option<u64>,
    /// The next changed-file page.
    pub(super) file_page: Option<u64>,
    /// The page-stream cursor of an unfinished patch window.
    pub(super) patch_offset: Option<usize>,
    /// The page listed changed files.
    pub(super) files_listed: bool,
}

impl DiffCursors {
    /// The cursors of a file page: its `nextPage`, read out of the block
    /// (a cursor rides the `next` continuation, never the block), and the
    /// patch-stream cursor of its unfinished patch window. The cursor is
    /// internal state for `next.continuePatch`, never an output field.
    pub(super) fn of_file_page(
        files_page: &mut Value,
        patch_offset: Option<usize>,
        files_listed: bool,
    ) -> Self {
        Self {
            commit_page: None,
            file_page: files_page
                .as_object_mut()
                .and_then(|page| page.remove("nextPage"))
                .and_then(|page| page.as_u64()),
            patch_offset,
            files_listed,
        }
    }
}

/// The commit read a commit file page continues: the generated
/// `ghGetHistoryItem` commit query at `reference`, with the caller's
/// page, window and scope.
pub(super) fn commit_query(q: &HistoryItemRequest, reference: &str) -> Value {
    use octocode_contract_types::{
        GhGetHistoryItemQuery, GhGetHistoryItemQueryRef, GhGetHistoryItemQuerySectionsItem,
        HiInclude, HiIncludeItem, HiMatchString, HiOwner, HiRepo,
    };
    let positive = |value: usize| {
        u64::try_from(value)
            .ok()
            .and_then(std::num::NonZeroU64::new)
    };
    let typed = || -> Option<GhGetHistoryItemQuery> {
        Some(GhGetHistoryItemQuery::Commit {
            // A hop keeps the patch grep that narrowed the row it came from.
            context_lines: gh_get_history_item_query::match_context(q)
                .and_then(|lines| i64::try_from(lines).ok())
                .map(Into::into),
            extras: Vec::new(),
            // Page 1 and the contract's page size are what an omitted
            // field reads: a continuation restates neither.
            page: gh_get_history_item_query::page(q)
                .filter(|page| *page > 1)
                .and_then(positive)
                .map(Into::into),
            include: (!gh_get_history_item_query::include_patterns(q).is_empty())
                .then(|| {
                    gh_get_history_item_query::include_patterns(q)
                        .into_iter()
                        .map(|path| HiIncludeItem::try_from(path).ok())
                        .collect::<Option<Vec<_>>>()
                        .map(HiInclude)
                })
                .flatten(),
            length: gh_get_history_item_query::char_length(q)
                .and_then(positive)
                .map(Into::into),
            main_goal: None,
            match_string: gh_get_history_item_query::match_string(q)
                .map(|text| HiMatchString(text.to_owned())),
            offset: gh_get_history_item_query::char_offset(q)
                .and_then(|offset| u64::try_from(offset).ok())
                .map(Into::into),
            owner: HiOwner::try_from(gh_get_history_item_query::owner(q)).ok()?,
            page_size: gh_get_history_item_query::page_size(q).and_then(positive),
            reasoning: None,
            ref_: GhGetHistoryItemQueryRef::try_from(reference).ok()?,
            repo: HiRepo::try_from(gh_get_history_item_query::repo(q)).ok()?,
            sections: if q.include_diff() {
                vec![GhGetHistoryItemQuerySectionsItem::Patches]
            } else {
                Vec::new()
            },
        })
    };
    typed()
        .map(|query| query_row(&query, &[]))
        .unwrap_or_default()
}

/// `next.*` for a commit or comparison file page, from the cursors the page
/// computed.
pub(super) fn attach_diff_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    cursors: DiffCursors,
) {
    let mut next = Map::new();
    let base = match (
        operation,
        resolved_ref.or(gh_get_history_item_query::reference(q)),
    ) {
        (ItemOperation::Commit, Some(reference)) => commit_query(q, reference),
        _ => base_public_query(q, operation),
    };
    if let Some(page) = cursors.commit_page {
        let mut nq = base.clone();
        // The first commit page is the page a read without `page` lists.
        if page > 1 {
            nq["page"] = json!(page);
        } else {
            remove_key(&mut nq, "page");
        }
        // A commit page carries commits only; patch cursors would be inert.
        nq["sections"] = json!(["commits"]);
        for key in ["offset", "length", "matchString", "contextLines"] {
            remove_key(&mut nq, key);
        }
        next.insert("nextCommitPage".into(), continuation(nq));
    }
    // A file page with unread patches continues them first: its window
    // that finishes the patches offers the next file page.
    if let Some(page) = cursors.file_page.filter(|_| cursors.patch_offset.is_none()) {
        let mut nq = base.clone();
        nq["page"] = json!(page);
        remove_key(&mut nq, "offset");
        files_only(&mut nq, operation);
        pin_file_page_size(&mut nq, q, q.include_diff());
        next.insert("nextFilePage".into(), continuation(nq));
    }
    // Files were listed without patches: offer their diffs, from the patch
    // page that holds this page's first file (patch pages are cut on their
    // own, smaller grid).
    if !q.include_diff() && base.get("sections").is_none() && cursors.files_listed {
        let mut nq = base.clone();
        nq["sections"] = json!(["patches"]);
        remove_key(&mut nq, "offset");
        let listed_from = gh_get_history_item_query::page(q)
            .unwrap_or(1)
            .saturating_sub(1)
            * file_page_size(q, false);
        match listed_from / file_page_size(q, true) + 1 {
            1 => remove_key(&mut nq, "page"),
            page => nq["page"] = json!(page),
        }
        next.insert("readPatches".into(), continuation(nq));
    }
    // The page-stream cursor, not a file's own `patchPagination` offset.
    if let Some(offset) = cursors.patch_offset {
        let mut nq = base;
        nq["offset"] = json!(offset);
        files_only(&mut nq, operation);
        clamp_hop_length(&mut nq, q);
        next.insert("continuePatch".into(), continuation(nq));
    }
    // The page's own continuations join the reads the page already offers
    // (an unpatched file's search), never replace them.
    if !next.is_empty() {
        crate::tools::gh_shared::ensure_next(out);
        if let Some(pages) = out["next"].as_object_mut() {
            pages.extend(next);
        }
    }
}

/// A comparison file or patch continuation reads files alone: the commit
/// list was delivered with the page it continues.
pub(super) fn files_only(query: &mut Value, operation: ItemOperation) {
    if !matches!(operation, ItemOperation::Compare) {
        return;
    }
    let sections = query
        .get("sections")
        .and_then(Value::as_array)
        .map(|sections| {
            sections
                .iter()
                .filter(|section| section.as_str() != Some("commits"))
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|sections| !sections.is_empty())
        // A `matchString` with no section named reads patches.
        .unwrap_or_else(|| {
            let section = if query.get("matchString").is_some() {
                "patches"
            } else {
                "files"
            };
            vec![json!(section)]
        });
    query["sections"] = Value::Array(sections);
}

/// Commit → pull request: the PR the message names (a squash headline's
/// `(#8506)`, a merge commit, or a `PR-URL:` trailer); otherwise issue search
/// matches PRs by commit SHA. `message` is the whole commit message, which a
/// patch read does not carry.
pub(super) fn commit_pull_request_lead(
    message: &str,
    sha: &str,
    q: &HistoryItemRequest,
) -> (&'static str, Value) {
    if let Some(number) = commit_pull_request(
        message,
        gh_get_history_item_query::owner(q),
        gh_get_history_item_query::repo(q),
    ) {
        return (
            "readPullRequest",
            Continuation::new(
                ToolId::GhGetHistoryItem,
                json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(q),"repo":gh_get_history_item_query::repo(q),"number":number}),
            )
            .confidence(Confidence::Exact)
            .build(),
        );
    }
    (
        "findPullRequest",
        Continuation::new(
            ToolId::GhSearchHistory,
            json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(q),"repo":gh_get_history_item_query::repo(q),"keywords":[sha]}),
        )
        .confidence(Confidence::High)
        .build(),
    )
}
