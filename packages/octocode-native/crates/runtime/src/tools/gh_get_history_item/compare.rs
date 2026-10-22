//! `operation: "compare"`: a comparison's files (past GitHub's cap from the
//! two trees), its commit page, and the range's identity.
//! `operation: "commit"` and `operation: "compare"`: commit metadata or a
//! ref comparison, each with one page of changed files.
use super::axis::Axis;
use super::file_page::{
    DiffSides, FilePage, Lead, empty_scope_warning, file_page_note, first_page, lead_first,
    literal_prefix, patch_needle, read_commit_lead, shape_file_page,
};
use super::filter::{FileFilter, FileScope};
use super::patch_hop::{DiffCursors, attach_diff_continuations, files_only};
use super::pr_menu::{base_public_query, remove_key};
use super::util::{array, str_at, usize_at};
use super::window::{COMPARE_FILE_LIMIT, WindowState, page_starting_at};
use super::{HistoryItemRequest, ItemOperation, Section, fetch, validation};
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, ToolData};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::commit::{commit_date, utc_date};
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};

pub(super) async fn compare(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let page = gh_get_history_item_query::page(query).unwrap_or(1);
    let per = gh_get_history_item_query::collection_page_size(query);
    // Omitted sections read both lists; named sections read what they name.
    // A patch window continues one file page's patches only.
    let window = gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    let sections = query.sections();
    let wants_commits = !window
        && (!gh_get_history_item_query::has_sections(query) || sections.has(Section::Commits));
    let wants_files = !gh_get_history_item_query::has_sections(query) || sections.lists_files();
    let Comparison {
        mut raw,
        total,
        more,
        base_sha,
        head_sha,
    } = read_comparison(transport, query, context, wants_commits).await?;
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

/// The comparison's changed paths past GitHub's compare file list, from the
/// merge base's and the head's recursive trees: every path whose blob (or
/// submodule commit) differs, as provider-shaped entries without patch or
/// line counts, skipping the paths the compare list already holds. `None`
/// when either tree is truncated or unreadable (the cap then stands).
async fn tree_changed_files(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    raw: &Value,
    head_sha: &str,
    context: &RequestContext,
) -> Option<Vec<Value>> {
    let merge_base = str_at(raw, "/merge_base_commit/sha")?;
    let request = |reference: &str| octocode_github::TreeRequest {
        owner: gh_get_history_item_query::owner(query).to_owned(),
        repo: gh_get_history_item_query::repo(query).to_owned(),
        reference: reference.to_owned(),
        recursive: true,
    };
    let (base_tree, head_tree) = (request(merge_base), request(head_sha));
    let (base, head) = tokio::join!(
        transport.get_tree(&base_tree, context),
        transport.get_tree(&head_tree, context)
    );
    let (base, head) = (base.ok()?, head.ok()?);
    if base.truncated || head.truncated {
        return None;
    }
    Some(tree_diff(&base.tree, &head.tree, raw))
}

/// The changed entries between two recursive trees that `raw`'s compare
/// file list does not name (by path, or a rename's previous path).
fn tree_diff(
    base: &[octocode_github::TreeEntry],
    head: &[octocode_github::TreeEntry],
    raw: &Value,
) -> Vec<Value> {
    use std::collections::{BTreeMap, HashSet};
    let listed: HashSet<&str> = raw
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|file| {
            [
                str_at(file, "/filename"),
                str_at(file, "/previous_filename"),
            ]
        })
        .flatten()
        .collect();
    let content = |entries: &[octocode_github::TreeEntry]| {
        entries
            .iter()
            .filter(|entry| entry.kind == "blob" || entry.kind == "commit")
            .map(|entry| {
                (
                    entry.path.clone(),
                    (entry.sha.clone().unwrap_or_default(), entry.mode.clone()),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let (base, head) = (content(base), content(head));
    let mut changed = Vec::new();
    for (path, side) in &head {
        let status = match base.get(path) {
            None => "added",
            Some(old) if old != side => "modified",
            Some(_) => continue,
        };
        changed.push((path, status, &side.0));
    }
    for (path, side) in &base {
        if !head.contains_key(path) {
            changed.push((path, "removed", &side.0));
        }
    }
    changed.sort_by(|a, b| a.0.cmp(b.0));
    changed
        .into_iter()
        .filter(|(path, _, _)| !listed.contains(path.as_str()))
        .map(|(path, status, sha)| json!({"filename": path, "status": status, "sha": sha}))
        .collect()
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
    transport: &GitHubTransport,
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
    // row and every continuation name one comparison. A cross-repository
    // `owner:ref` head is pinned below, from the comparison's own commits.
    let head = if head.contains(':') || octocode_github::is_full_sha(head) {
        head.to_owned()
    } else {
        transport
            .commit_sha(
                gh_get_history_item_query::owner(query),
                gh_get_history_item_query::repo(query),
                head,
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

/// The comparison's commit page a read asked for.
struct CommitPage {
    page: usize,
    per: usize,
    /// GitHub's count of the range's commits (`total_commits`).
    total: usize,
    more: bool,
}

/// What a listed commit page leaves to the row's leads.
struct ListedCommits {
    /// The page's commit SHAs, in order.
    shas: Vec<String>,
    /// How many of the range's commits this page and the pages before it
    /// show, when more follow; `None` when it shows the last one.
    seen: Option<usize>,
    /// The whole-message read of the first commit whose message has more
    /// than its headline.
    read: Option<Value>,
}

/// The commit page: each commit by its SHA and headline (any row's
/// `commitSha` reads its whole message). A scoped comparison lists every
/// commit of the range, not only the scope's: the `narrowScope` lead lists
/// the scope's own commits.
fn attach_commit_page(
    out: &mut Value,
    raw: &Value,
    query: &HistoryItemRequest,
    at: CommitPage,
) -> ListedCommits {
    let commits = array(raw.get("commits").cloned().unwrap_or(json!([])));
    let shas = commits
        .iter()
        .filter_map(|v| str_at(v, "/sha"))
        .map(str::to_owned)
        .collect();
    let read = commits
        .iter()
        .find(|v| {
            let message = str_at(v, "/commit/message").unwrap_or("").trim_end();
            message.lines().nth(1).is_some()
        })
        .and_then(|v| str_at(v, "/sha"))
        .map(|sha| read_commit_lead(query, sha));
    out["commits"] = json!(
        commits
            .into_iter()
            .map(|v| {
                let message = str_at(&v, "/commit/message").unwrap_or("");
                json!({
                    "commitSha":v["sha"],
                    "messageHeadline":message.lines().next().unwrap_or(message),
                    "author":super::pr_sections::commit_person(&v),
                    "date":commit_date(&v)
                })
            })
            .collect::<Vec<_>>()
    );
    // Every commit page states where it stands, the last one too. GitHub
    // counts the range (`total_commits`): a count-cut page states that
    // total unless the provider's links say otherwise.
    let counted = at.total > 0 && (at.page.saturating_mul(at.per) < at.total) == at.more;
    let facts = if counted {
        crate::response::pages::PageFacts::counted(at.page, at.per, at.total)
    } else {
        crate::response::pages::PageFacts::open(at.page, Some(at.per), at.more)
    };
    // A commit page past the last is the shared out-of-range outcome (empty,
    // `page N of M`, a restart), unless the same page still lists files.
    let files_listed = out
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| !files.is_empty());
    if files_listed
        || !crate::response::paging::out_of_range(
            out,
            &["contentPagination", Axis::Commits.key()],
            facts,
        )
    {
        // The commit page continuation (`nextCommitPage`) is the cursor.
        out["contentPagination"][Axis::Commits.key()] = facts.to_value();
    }
    ListedCommits {
        shas,
        seen: at.more.then(|| at.page.saturating_mul(at.per)),
        read,
    }
}

/// The comparison's changed-file page (paths and line stats; patches with
/// the patches section or a `matchString`): GitHub lists the files whole
/// with any commit page, so the page is a window of that list, then the
/// shared file-page shaping. `query` names the comparison by its pinned
/// commits.
fn attach_file_page(
    out: &mut Value,
    files: Option<Value>,
    merge_base: Option<&str>,
    query: &HistoryItemRequest,
    file_limit: bool,
) -> Result<FilePage, ProviderError> {
    let all_files = array(files.unwrap_or(json!([])));
    let all_count = all_files.len();
    let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
    let first_window =
        gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0);
    let unmatched = scope
        .as_ref()
        .filter(|_| first_window)
        .and_then(|scope| empty_scope_warning(&all_files, Some(scope), query, !file_limit));
    let include_diff = query.include_diff();
    let needle = patch_needle(query);
    let mut files = FileFilter {
        needle: needle.as_deref(),
        scope: scope.as_ref(),
    }
    .page(all_files, WindowState::COMPLETE, query, include_diff);
    // The whole comparison's count, when the listed files are not all of
    // it (a scope or a later page). At GitHub's cap the count is only a
    // floor: the file page's `countScope` says so and no count is claimed.
    if !include_diff && !file_limit && files.files.len() < all_count {
        out["changedFilesCount"] = json!(all_count);
    }
    // The page's own count fact: at the cap its total is a floor.
    if file_limit {
        files.page["countScope"] = json!("partial");
    }
    let head = out["headSha"].as_str().map(str::to_owned);
    let page = shape_file_page(
        out,
        files,
        query,
        DiffSides {
            new: head.as_deref(),
            // The old side of a three-dot comparison is the merge base.
            old: merge_base,
        },
        || {
            let mut same = base_public_query(query, ItemOperation::Compare);
            remove_key(&mut same, "offset");
            files_only(&mut same, ItemOperation::Compare);
            same
        },
    );
    if let Some(warning) = unmatched {
        push_warnings(out, [warning]);
        // A scope that left none of a complete file list is an empty answer,
        // as it is on a commit; a capped list may hide the scope's files.
        if !file_limit {
            out["status"] = json!("empty");
        }
    }
    Ok(page)
}

/// A scoped path's own history lists the commits of the range that touched
/// it: `ghSearchHistory` at the pinned head, from the range's start (its
/// merge base: the base commit's own date is the first it can list, never an
/// older commit). When GitHub's compare file cap stands (the trees could
/// not extend it) the scoped path may look unchanged when it is not; a file
/// read warns and leads to the range's commits, whose touched files cover
/// the net change (and more: a file changed and then reverted is touched).
fn capped_file_leads(
    query: &HistoryItemRequest,
    out: &mut Value,
    raw: &Value,
    file_limit: bool,
    wants_files: bool,
    commits: Option<&ListedCommits>,
) -> Vec<Lead> {
    let mut leads = Vec::new();
    // The scope's literal prefix: a glob's directory, or the exact path.
    if let Some(path) = gh_get_history_item_query::include_patterns(query)
        .into_iter()
        .find_map(literal_prefix)
    {
        let mut history = json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"path":path,"ref":out["headSha"]});
        if let Some(date) = str_at(raw, "/merge_base_commit/commit/committer/date")
            .or_else(|| str_at(raw, "/base_commit/commit/committer/date"))
        {
            history["since"] = json!(utc_date(Some(date)));
        }
        leads.push((
            "narrowScope",
            Continuation::new(ToolId::GhSearchHistory, history)
                .confidence(Confidence::High)
                .build(),
        ));
    }
    if !file_limit {
        return leads;
    }
    if wants_files && first_page(query) {
        // The cap is stated once, on the comparison's first page, with what
        // it may hide.
        let include = gh_get_history_item_query::include_patterns(query);
        let hidden = if include.is_empty() {
            "other changed files".to_owned()
        } else {
            format!("changes to {}", include.join(", "))
        };
        push_warnings(
            out,
            [format!(
                "GitHub lists at most {COMPARE_FILE_LIMIT} changed files, so {hidden} may be missing; the range's commits reach them, as files each commit touched (a superset of the net change)."
            )],
        );
    }
    if wants_files {
        leads.extend(capped_files_reach(query, out, commits));
    }
    leads
}

/// The lead that reaches the changed files past GitHub's compare cap through
/// the range's commits: this page's commits as one batch of commit reads when
/// the page lists no more commits than a batch takes, else the commits the
/// page does not show, in batch-sized pages (each carries that batch read;
/// [`page_starting_at`] aligns them). A wider page that ends the range has
/// no unseen commit to list: its rows' `commitSha` are the reads.
fn capped_files_reach(
    query: &HistoryItemRequest,
    out: &Value,
    commits: Option<&ListedCommits>,
) -> Option<(&'static str, Value)> {
    let batch = crate::response::result::rows_per_call(ToolId::GhGetHistoryItem);
    let Some(listed) = commits else {
        return Some(commit_list_lead(query, out, batch, 1));
    };
    if !listed.shas.is_empty() && listed.shas.len() <= batch {
        let rows = listed
            .shas
            .iter()
            .map(|sha| {
                json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"ref":sha})
            })
            .collect::<Vec<_>>();
        return Some((
            "readCommitFiles",
            Continuation::input(ToolId::GhGetHistoryItem, json!({"queries": rows}))
                .confidence(Confidence::High)
                .build(),
        ));
    }
    let (page, size) = page_starting_at(listed.seen?, batch);
    Some(commit_list_lead(query, out, size, page))
}

fn commit_list_lead(
    query: &HistoryItemRequest,
    out: &Value,
    size: usize,
    page: usize,
) -> (&'static str, Value) {
    let mut list = json!({"operation":"compare","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"base":out["baseSha"],"head":out["headSha"],"sections":["commits"],"pageSize":size});
    if page > 1 {
        list["page"] = json!(page);
    }
    (
        "listCommits",
        Continuation::new(ToolId::GhGetHistoryItem, list)
            .confidence(Confidence::High)
            .build(),
    )
}

pub(super) fn compare_identity(
    raw: &Value,
    requested_base: &str,
    requested_head: &str,
) -> (String, String) {
    let permalink = raw
        .get("permalink_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let pair = permalink
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split_once("..."));
    // The permalink spells both sides `owner:abbrev`; the echo keeps the
    // requested form (an `owner:` prefix only when the caller wrote one) and
    // expands an abbreviation to the full SHA it uniquely names.
    let expand = |requested: &str, parsed: Option<&str>| {
        let candidate = parsed.unwrap_or(requested);
        let abbrev = candidate.rsplit(':').next().unwrap_or(candidate);
        let prefix = requested
            .rsplit_once(':')
            .map_or(String::new(), |(owner, _)| format!("{owner}:"));
        if abbrev.len() == 40 {
            return format!("{prefix}{abbrev}");
        }
        let mut known = Vec::new();
        if let Some(sha) = raw.pointer("/base_commit/sha").and_then(Value::as_str) {
            known.push(sha);
        }
        if let Some(sha) = raw
            .pointer("/merge_base_commit/sha")
            .and_then(Value::as_str)
        {
            known.push(sha);
        }
        for commit in raw
            .get("commits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(sha) = commit.get("sha").and_then(Value::as_str) {
                known.push(sha);
            }
        }
        // base_commit and merge_base_commit are often the same commit.
        known.sort_unstable();
        known.dedup();
        let matches: Vec<_> = known
            .into_iter()
            .filter(|sha| {
                sha.to_ascii_lowercase()
                    .starts_with(&abbrev.to_ascii_lowercase())
            })
            .collect();
        if matches.len() == 1 {
            format!("{prefix}{}", matches[0])
        } else {
            requested.to_owned()
        }
    };
    (
        expand(requested_base, pair.map(|(base, _)| base)),
        expand(requested_head, pair.map(|(_, head)| head)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_identity_expands_permalink_abbreviations() {
        let raw = json!({
            "permalink_url": "https://github.com/a/b/compare/abc1234...def5678",
            "base_commit": {"sha": "abc1234aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "commits": [{"sha": "def5678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]
        });
        let (base, head) = compare_identity(&raw, "main", "feature");
        assert!(base.starts_with("abc1234"));
        assert!(head.starts_with("def5678"));
    }

    /// GitHub's permalink spells both sides `owner:abbrev`; the echo
    /// must keep the caller's bare-SHA form on both sides, and a base that is
    /// both base_commit and merge_base_commit still expands.
    #[test]
    fn compare_identity_echoes_the_requested_form_without_owner_prefix() {
        let base_sha = "af20f667fd2536c9502f69d99fe6bdedfcc839cb";
        let head_sha = "c265e3f9413161c900cbf4aa70d451b8e6b3920a";
        let raw = json!({
            "permalink_url": "https://github.com/o/r/compare/o:af20f66...o:c265e3f",
            "base_commit": {"sha": base_sha},
            "merge_base_commit": {"sha": base_sha},
            "commits": [{"sha": head_sha}]
        });
        assert_eq!(
            compare_identity(&raw, base_sha, head_sha),
            (base_sha.to_owned(), head_sha.to_owned())
        );
        assert_eq!(
            compare_identity(&raw, "af20f66", "c265e3f"),
            (base_sha.to_owned(), head_sha.to_owned())
        );
        // A caller-written fork prefix is kept.
        assert_eq!(
            compare_identity(&raw, base_sha, "fork:c265e3f").1,
            format!("fork:{head_sha}")
        );
    }

    /// A list lead starts right after the shown items and every later item
    /// falls on exactly one of its pages: the comparison's unseen commits
    /// (batch-sized pages) and an issue's unshown comments (one provider
    /// batch at most) alike.
    #[test]
    fn a_page_starting_at_an_item_reaches_every_later_item_once() {
        for (seen, batch) in [(100, 5), (100, 3), (97, 5), (30, 4), (7, 1), (5, 5)] {
            let (page, size) = page_starting_at(seen, batch);
            assert!((1..=batch).contains(&size), "{seen}/{batch}: {size}");
            assert_eq!(size * (page - 1), seen, "{seen}/{batch}: {size} {page}");
        }
        assert_eq!(page_starting_at(100, 5), (21, 5));
        assert_eq!(page_starting_at(12, 100), (2, 12));
        assert_eq!(page_starting_at(97, 100), (2, 97));
        assert_eq!(page_starting_at(190, 100), (3, 95));
        assert_eq!(page_starting_at(200, 100), (3, 100));
    }
}
