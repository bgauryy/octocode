//! `operation: "compare"`: the range's identity and counts (the summary
//! row), its files (past GitHub's cap from the two trees) and its commit
//! page.
use super::axis::Axis;
use super::file_page::{first_page, lead_first};
use super::filter::FileScope;
use super::patch_hop::{DiffCursors, attach_diff_continuations};
use super::util::{str_at, usize_at};
use super::window::COMPARE_FILE_LIMIT;
use super::{HistoryItemRequest, ItemOperation, fetch, validation};
use crate::gh_get_history_item::Source;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::sections::Section;
use octocode_engine::num::saturating_u64;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use octocode_runtime_base::response::paging::push_warnings;
use octocode_runtime_base::response::result::ToolData;
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
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let page = gh_get_history_item_query::page(query).unwrap_or(1);
    let per = gh_get_history_item_query::collection_page_size(query);
    // Named sections read what they name. Omitted sections answer with the
    // summary row: the range's identity and its counts (`aheadBy`,
    // `changedFileCount`), each list one section away (`next.commits` and
    // `next.files` read their first pages). A file
    // filter (`include`, `fileStatus`) or a `matchString`
    // asks about files: it answers with its file page. Neither scopes the
    // commits: a filtered read pages them apart, from their first page, by
    // `next.commits` or `sections: ["commits"]`. A patch window
    // continues one file page's patches only.
    let window = gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    let sections = query.sections();
    let named = gh_get_history_item_query::has_sections(query);
    let summary = sections.is_empty() && !gh_get_history_item_query::has_file_filter(query);
    let files_asked = !named && !summary;
    let wants_commits = !window && named && sections.has(Section::Commits);
    let wants_files = files_asked || sections.lists_files();
    let Comparison {
        mut raw,
        total,
        more,
        base_sha,
        head_sha,
    } = read_comparison(provider, source, query, context, wants_commits).await?;
    let capped = raw
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| files.len() >= COMPARE_FILE_LIMIT);
    // Past GitHub's compare file cap the two commits' trees name every
    // other changed path (two reads); the cap stays only when a tree is
    // truncated or unreadable.
    let tree_files = if wants_files && capped {
        tree_changed_files(source, query, &raw, &head_sha, context).await
    } else {
        None
    };
    let file_limit = capped && tree_files.is_none();
    // Whether the read's file scope keeps a tree-derived row (no line
    // counts or patch): only then is the tree source the reader's concern.
    let tree_listed = match tree_files.as_deref() {
        Some(extra) if !extra.is_empty() => {
            let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
            extra
                .iter()
                .any(|file| scope.as_ref().is_none_or(|scope| scope.matches(file)))
        }
        _ => false,
    };
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
    let mut listed_files = raw.get_mut("files").map(Value::take);
    let mut out = json!({"baseCommitSha":base_sha,"headCommitSha":head_sha,
        "mergeBaseCommitSha":str_at(&raw, "/merge_base_commit/sha"),
        "aheadBy":usize_at(&raw,"/ahead_by"),"behindBy":usize_at(&raw,"/behind_by")});
    if more {
        octocode_runtime_base::response::paging::set_partial(&mut out, true);
    }
    // The changed files lead the row (what the comparison changed), the
    // commit page follows (why): each is paged apart by its own continuation.
    // One marker: the file list stops at GitHub's cap and the trees could
    // not extend it. Its lead below reads the range's commits instead.
    if wants_files && file_limit {
        octocode_runtime_base::response::paging::mark_partial(
            &mut out,
            octocode_runtime_base::response::paging::PartialReason::ProviderFileLimit,
            &[],
        );
    }
    // Continuations read the same two commits.
    let pinned = query.with_compare_refs(&base_sha, &head_sha)?;
    let file_page = if wants_files {
        let merge_base = str_at(&raw, "/merge_base_commit/sha");
        Some(attach_file_page(
            &mut out,
            listed_files.take(),
            merge_base,
            &pinned,
            file_limit,
        )?)
    } else {
        None
    };
    let mut cursors = file_page
        .as_ref()
        .map_or_else(DiffCursors::default, |page| page.cursors);
    // Nothing ahead of the base: an identical range is an empty answer, as a
    // commit or PR with nothing in scope is. A head behind its base (refs
    // given in reverse) is an answer, stated as such.
    let behind = usize_at(&raw, "/behind_by");
    let nothing_ahead = total == 0 && usize_at(&raw, "/ahead_by") == 0 && no_files;
    if nothing_ahead && behind == 0 {
        out["status"] = json!("empty");
    }
    if nothing_ahead && behind > 0 {
        push_warnings(
            &mut out,
            [format!(
                "head is behind base by {behind}: nothing ahead; swap base and head to compare the other way."
            )],
        );
    }
    // A range with no commits lists no commit page.
    let commits = (wants_commits && (named || total > 0)).then(|| {
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
    cursors.commit_page = more.then(|| saturating_u64(page + 1));
    // A file answer and the summary leave the range's commits to their own
    // pages: `next.commits` lists them from the first. `aheadBy` counts them
    // (GitHub's `total_commits` is that count), so no empty page block
    // repeats it; only a count that differs is stated there. An empty file
    // answer is the whole answer.
    let files_first = files_asked && !window && first_page(query);
    if (files_first || summary) && total > 0 && out["status"] != "empty" {
        if out.get("aheadBy") != Some(&json!(total)) {
            out["sectionPagination"][Axis::Commits.key()] =
                octocode_runtime_base::response::pages::PageFacts::unshown(total).to_value();
        }
        cursors.commit_page = Some(1);
    }
    // The summary states the changed files the same way: `changedFileCount`
    // counts them and `next.files` lists them from the first. At GitHub's
    // cap the count is a floor: no row count claims it, and the empty files
    // block states the floor (`countScope`).
    if summary && !no_files {
        let listed = listed_files
            .as_ref()
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if capped {
            let mut files =
                octocode_runtime_base::response::pages::PageFacts::unshown(listed).to_value();
            files["countScope"] = json!("partial");
            out["sectionPagination"][Axis::Files.key()] = files;
        } else {
            out["changedFileCount"] = json!(listed);
        }
        cursors.file_page = Some(1);
    }
    attach_diff_continuations(&mut out, &pinned, ItemOperation::Compare, None, cursors);
    // A literal that hit nothing leads with its recovery, as on a commit.
    let (without_match, side_reads) = file_page
        .map(|page| (page.without_match, page.side_reads))
        .unwrap_or_default();
    let mut leads = Vec::from_iter(without_match);
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
                .map(|read| ("readCommit".to_owned(), read)),
        );
    }
    // The tree source is a static fact of the comparison: stated once, on
    // its first page, when the read's scope keeps a tree-derived row.
    if first_page(query) && tree_listed {
        push_warnings(
            &mut out,
            [format!(
                "GitHub's compare lists {COMPARE_FILE_LIMIT} changed files; the rest come from the two commits' trees: no line counts or patches (each reads at headCommitSha), and a rename shows as a removal plus an addition."
            )],
        );
    }
    lead_first(&mut out, leads);
    Ok(ToolData::from(out))
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
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants_commits: bool,
) -> Result<Comparison, ProviderError> {
    let page = gh_get_history_item_query::page(query).unwrap_or(1);
    let per = gh_get_history_item_query::collection_page_size(query);
    let base =
        gh_get_history_item_query::base(query).ok_or_else(|| validation("base is required"))?;
    let head =
        gh_get_history_item_query::head(query).ok_or_else(|| validation("head is required"))?;
    // The head commit is the last of the comparison, often past this commit
    // page: resolve a movable head (branch, tag) to the commit read so the
    // row and every continuation name one comparison; the resolution is the
    // current resolution, carried by every continuation. A
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
                context,
            )
            .await?
    };
    let refs = format!("{base}...{head}");
    // GitHub pages the commit list; the changed files come whole with any
    // page, so a files-only read asks for the first commit page.
    let (commit_page, commit_per) = if wants_commits { (page, per) } else { (1, per) };
    let (raw, link_more) = fetch(
        source,
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
            source,
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
    source: &Source<'_>,
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
        source,
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
