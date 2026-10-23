//! `operation: "compare"`: a comparison's files (past GitHub's cap from the
//! two trees), its commit page, and the range's identity.
use super::axis::Axis;
use super::file_page::{file_page_note, first_page, lead_first};
use super::patch_hop::{DiffCursors, attach_diff_continuations};
use super::pr_menu::{base_public_query, remove_key};
use super::util::{str_at, usize_at};
use super::window::COMPARE_FILE_LIMIT;
use super::{HistoryItemRequest, ItemOperation, Section, fetch, validation};
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, ToolData};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use octocode_github::{GitHubProvider, GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};

mod identity;
mod leads;
mod pages;
#[cfg(test)]
mod tests;
mod trees;

use identity::compare_identity;
use leads::capped_file_leads;
use pages::{CommitPage, attach_commit_page, attach_file_page};
use trees::tree_changed_files;

pub(super) async fn compare(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    let page = gh_get_history_item_query::page(query).unwrap_or(1);
    let per = gh_get_history_item_query::collection_page_size(query);
    // Omitted sections read both lists; named sections read what they name.
    // A patch window continues one file page's patches only.
    let window = gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    let sections = query.sections();
    let named = gh_get_history_item_query::has_sections(query);
    // `include` or `matchString` asks about files: the range's commits
    // (which neither scopes) are paged apart, from their first page.
    let files_asked = !named
        && (!gh_get_history_item_query::include_patterns(query).is_empty()
            || gh_get_history_item_query::match_string(query).is_some());
    let wants_commits = !window
        && if named {
            sections.has(Section::Commits)
        } else {
            !files_asked
        };
    let wants_files = !named || sections.lists_files();
    let Comparison {
        mut raw,
        total,
        more,
        base_sha,
        head_sha,
    } = read_comparison(provider, query, context, wants_commits).await?;
    let capped = raw
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| files.len() >= COMPARE_FILE_LIMIT);
    // Past GitHub's compare file cap the two commits' trees name every
    // other changed path (two reads); the cap stays only when a tree is
    // truncated or unreadable.
    let tree_files = if wants_files && capped {
        tree_changed_files(transport, query, &raw, &head_sha, context).await
    } else {
        None
    };
    let file_limit = capped && tree_files.is_none();
    let tree_listed = tree_files.as_ref().is_some_and(|extra| !extra.is_empty());
    if let Some(extra) = tree_files.filter(|extra| !extra.is_empty())
        && let Some(files) = raw.get_mut("files").and_then(Value::as_array_mut)
    {
        files.extend(extra);
    }
    // The file list moves into its page; whether it was empty is kept.
    let no_files = raw
        .get("files")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    let listed_files = raw.get_mut("files").map(Value::take);
    let mut out = json!({"baseSha":base_sha,"headSha":head_sha,
        "aheadBy":usize_at(&raw,"/ahead_by"),"behindBy":usize_at(&raw,"/behind_by")});
    if more {
        out["isPartial"] = json!(true);
    }
    // The changed files lead the row (what the comparison changed), the
    // commit page follows (why): each is paged apart by its own continuation.
    // One marker: the file list stops at GitHub's cap and the trees could
    // not extend it. Its lead below reads the range's commits instead.
    if wants_files && file_limit {
        crate::response::paging::mark_partial(&mut out, "providerFileLimit", &[]);
    }
    // Continuations read the same two commits.
    let pinned = query.with_compare_refs(&base_sha, &head_sha)?;
    let file_page = wants_files
        .then(|| {
            let merge_base = str_at(&raw, "/merge_base_commit/sha");
            attach_file_page(&mut out, listed_files, merge_base, &pinned, file_limit)
        })
        .transpose()?;
    let mut cursors = file_page
        .as_ref()
        .map_or_else(DiffCursors::default, |page| page.cursors);
    // An identical range (the head is the base, or behind it) changes
    // nothing: an empty answer, as a commit or PR with nothing in scope is.
    if total == 0 && usize_at(&raw, "/ahead_by") == 0 && no_files {
        out["status"] = json!("empty");
    }
    let commits = wants_commits.then(|| {
        attach_commit_page(
            &mut out,
            &raw,
            query,
            CommitPage {
                page,
                per,
                total,
                more,
            },
        )
    });
    cursors.commit_page = more
        .then(|| {
            crate::response::paging::next_cursor(
                &mut out,
                crate::id::ToolId::GhGetHistoryItem,
                None,
                "page",
                page + 1,
            )
        })
        .flatten()
        .map(|next| u64::try_from(next).unwrap_or(u64::MAX));
    // A file answer leaves the range's commits to their own pages: the
    // section states their count with none shown, and its page lists them
    // from the first. An empty file answer is the whole answer.
    if files_asked && !window && first_page(query) && total > 0 && out["status"] != "empty" {
        let mut unshown = crate::response::pages::PageFacts::counted(1, per, total).to_value();
        unshown["currentPage"] = json!(0);
        unshown["hasMore"] = json!(true);
        out["contentPagination"][Axis::Commits.key()] = unshown;
        cursors.commit_page = Some(1);
    }
    attach_diff_continuations(&mut out, &pinned, ItemOperation::Compare, None, cursors);
    // A literal that hit nothing leads with its recovery, as on a commit.
    let (without_match, side_reads) = file_page
        .map(|page| (page.without_match, page.side_reads))
        .unwrap_or_default();
    let mut leads = Vec::from_iter(without_match);
    // A head behind its base (refs given in reverse) changes nothing this
    // way round: the swapped comparison reads the `behindBy` commits.
    if out["status"] == "empty" && usize_at(&raw, "/behind_by") > 0 {
        leads.push(("swapRefs", swapped_refs(query)));
    }
    leads.extend(capped_file_leads(
        query,
        &mut out,
        &raw,
        file_limit,
        wants_files,
        commits.as_ref(),
    ));
    leads.extend(side_reads);
    // A literal that emptied the file page answers with its recovery alone.
    if leads
        .first()
        .is_none_or(|(name, _)| *name != "withoutMatchString")
    {
        leads.extend(
            commits
                .and_then(|commits| commits.read)
                .map(|read| ("readCommit", read)),
        );
    }
    // The tree source is a static fact of the comparison: stated once, on
    // its first page.
    if first_page(query) && tree_listed {
        push_warnings(
            &mut out,
            [format!(
                "GitHub's compare lists {COMPARE_FILE_LIMIT} changed files; the rest come from the two commits' trees: no line counts or patches (each reads at headSha), and a rename shows as a removal plus an addition."
            )],
        );
    }
    lead_first(&mut out, leads);
    Ok(ToolData {
        remaining: file_page_note(&out),
        ..ToolData::from(out)
    })
}

/// The same comparison with `base` and `head` exchanged, from its first page.
fn swapped_refs(query: &HistoryItemRequest) -> Value {
    let mut swapped = base_public_query(query, ItemOperation::Compare);
    for key in ["page", "offset"] {
        remove_key(&mut swapped, key);
    }
    swapped["base"] = json!(gh_get_history_item_query::head(query));
    swapped["head"] = json!(gh_get_history_item_query::base(query));
    Continuation::new(ToolId::GhGetHistoryItem, swapped)
        .confidence(Confidence::Exact)
        .build()
}

/// One comparison page as GitHub answers it, with both ends pinned to
/// commits.
struct Comparison {
    raw: Value,
    /// GitHub's count of the range's commits (`total_commits`).
    total: usize,
    /// A later commit page exists.
    more: bool,
    base_sha: String,
    head_sha: String,
}

/// Read the comparison page the query names (its commit page when
/// `wants_commits`, else the first commit page beside the whole file list).
async fn read_comparison(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants_commits: bool,
) -> Result<Comparison, ProviderError> {
    let transport = &provider.transport;
    let page = gh_get_history_item_query::page(query).unwrap_or(1);
    let per = gh_get_history_item_query::collection_page_size(query);
    let base =
        gh_get_history_item_query::base(query).ok_or_else(|| validation("base is required"))?;
    let head =
        gh_get_history_item_query::head(query).ok_or_else(|| validation("head is required"))?;
    // The head commit is the last of the comparison, often past this commit
    // page: resolve a movable head (branch, tag) to the commit read so the
    // row and every continuation name one comparison; the resolution is the
    // memoized single flight every GitHub read of the ref shares. A
    // cross-repository `owner:ref` head is pinned below, from the
    // comparison's own commits.
    let head = if head.contains(':') || octocode_github::is_full_sha(head) {
        head.to_owned()
    } else {
        provider
            .resolve_reference(
                gh_get_history_item_query::owner(query),
                gh_get_history_item_query::repo(query),
                Some(head),
                false,
                context,
            )
            .await?
    };
    let refs = format!("{base}...{head}");
    // GitHub pages the commit list; the changed files come whole with any
    // page, so a files-only read asks for the first commit page: the URL
    // (cache key) the row's first read already fetched.
    let (commit_page, commit_per) = if wants_commits { (page, per) } else { (1, per) };
    let (raw, link_more) = fetch(
        transport,
        &[
            "repos",
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            "compare",
            &refs,
        ],
        &[
            ("page", commit_page.to_string()),
            ("per_page", commit_per.to_string()),
        ],
        context,
    )
    .await?;
    let total = usize_at(&raw, "/total_commits");
    let more = wants_commits && (link_more || page.saturating_mul(per) < total);
    let (base_sha, mut head_sha) = compare_identity(&raw, base, &head);
    if head.contains(':') {
        head_sha = pin_fork_head(
            transport,
            query,
            &refs,
            &raw,
            commit_page.saturating_mul(commit_per),
            context,
        )
        .await?;
    }
    Ok(Comparison {
        raw,
        total,
        more,
        base_sha,
        head_sha,
    })
}

/// The commit a cross-repository head (`owner:branch`) names now. A fork's
/// branch moves and the upstream repository cannot resolve it by name, so
/// every continuation needs the SHA. The range's last commit is the head;
/// when the page read (ending at commit `read_end`) does not end the range,
/// one more read of the last page
/// (one commit) names it. A head with no commits ahead is the merge base.
async fn pin_fork_head(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    refs: &str,
    raw: &Value,
    read_end: usize,
    context: &RequestContext,
) -> Result<String, ProviderError> {
    let total = usize_at(raw, "/total_commits");
    let last_of = |value: &Value| {
        value
            .get("commits")
            .and_then(Value::as_array)
            .and_then(|commits| commits.last())
            .and_then(|commit| str_at(commit, "/sha"))
            .map(str::to_owned)
    };
    if total == 0 {
        return Ok(str_at(raw, "/merge_base_commit/sha")
            .unwrap_or(refs)
            .to_owned());
    }
    if read_end >= total
        && let Some(sha) = last_of(raw)
    {
        return Ok(sha);
    }
    let (last_page, _) = fetch(
        transport,
        &[
            "repos",
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            "compare",
            refs,
        ],
        &[("page", total.to_string()), ("per_page", "1".to_owned())],
        context,
    )
    .await?;
    last_of(&last_page).ok_or_else(|| validation("compare head has no commit"))
}
