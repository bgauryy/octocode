//! One listing page: the pinned ref, the page window, the entries and
//! their continuations.
use super::*;
use crate::tools::root_read::{RANK_ANY, entry_text, read_rank};

/// Pin the listing to one commit: an explicit ref that does not resolve is
/// an error (like ghGetFileContent), never a silent default-branch listing,
/// and every page of one listing reads the same tree. The recursive tree is
/// fetched by the resolved commit SHA, never by the ref name.
pub(super) async fn resolve_listing_ref(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
) -> Result<(String, String), ProviderError> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    match query.ref_.as_deref() {
        Some(branch) => {
            let sha = provider
                .resolve_reference(owner, repo, Some(branch), context)
                .await
                .map_err(|error| missing_ref(error, owner, repo, branch))?;
            Ok((branch.to_owned(), sha))
        }
        None => Ok(tokio::try_join!(
            default_branch(provider, owner, repo, context),
            provider.resolve_reference(owner, repo, None, context),
        )?),
    }
}

/// One page of a sorted listing.
pub(super) struct ListingPage<'a> {
    pub(super) current: usize,
    pub(super) per_page: usize,
    pub(super) total_entries: usize,
    pub(super) total_pages: usize,
    pub(super) entries: &'a [TreeEntry],
}

impl<'a> ListingPage<'a> {
    pub(super) fn of(query: &GhStructureQueryTree, all: &'a [TreeEntry]) -> Self {
        let current = query
            .page
            .map_or(1, |page| octocode_engine::num::saturating_usize(page.get()));
        // Entries per page: the query's `limit` (its contract default when
        // omitted); each page reads the tree afresh at the pinned commit, so
        // any size pages it losslessly.
        let per_page = octocode_engine::num::saturating_usize(query.limit.get());
        let total_entries = all.len();
        let total_pages = crate::response::page_windows::count_pages(total_entries, per_page);
        let start = current
            .saturating_sub(1)
            .saturating_mul(per_page)
            .min(total_entries);
        let end = start.saturating_add(per_page).min(total_entries);
        Self {
            current,
            per_page,
            total_entries,
            total_pages,
            entries: &all[start..end],
        }
    }

    pub(super) fn has_more(&self) -> bool {
        self.current < self.total_pages
    }

    /// The requested page lies past the last page of the listing.
    pub(super) fn past_end(&self) -> bool {
        self.current > self.total_pages
    }

    /// The `pagination` block: shown when the listing has more than one page,
    /// and on a page past the end of any listing (the response layer words
    /// the page count and leads back to page 1).
    pub(super) fn paginate(&self, value: &mut Value) {
        if self.total_pages > 1 || self.past_end() {
            value["pagination"] = crate::response::pages::PageFacts::nth(
                self.current,
                self.per_page,
                self.total_entries,
            )
            .to_value();
        }
    }
}

/// File and folder counts of the whole listing (never one page's share), and
/// the bytes of the files whose size the provider states.
pub(super) fn summary(entries: &[TreeEntry]) -> Value {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .count();
    let mut summary = json!({"fileCount": files, "dirCount": entries.len() - files});
    let bytes = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.size)
        .collect::<Option<Vec<_>>>();
    if let Some(sizes) = bytes {
        summary["bytes"] = json!(sizes.iter().sum::<u64>());
    }
    summary
}

/// Provider limits and skipped entries of the walk, stated on the page. A
/// limit's `terminalLimitReached` code is the shared coverage code
/// ([`crate::response::paging::terminal_limit`]), never restated here.
pub(super) fn disclose_limits(
    traversal: &Traversal,
    page: &ListingPage<'_>,
    value: &mut Value,
    output: &mut ToolData,
) {
    use crate::response::paging::{mark_partial, terminal_limit};
    if traversal.contents_limit {
        terminal_limit(
            value,
            crate::response::paging::PartialReason::ProviderContentsLimit,
            &[],
        );
        value["providerLimit"] = json!({
            "reason": "providerContentsLimit",
            "maxEntriesPerDirectory": CONTENTS_LIMIT
        });
    }
    if !traversal.omitted.is_empty() && page.current == 1 {
        // Symlinks and submodules have no listable content: name each
        // instead of silently dropping it, once per listing.
        value["omitted"] = json!(traversal.omitted);
    }
    if traversal.failed_subtrees > 0 {
        mark_partial(
            value,
            crate::response::paging::PartialReason::PartialTreeFailures,
            &[],
        );
        output.diagnostics.hint(
            "One or more repository subdirectories could not be read; retry the same page or narrow the path/depth.",
        );
    }
}

/// A directory past GitHub's Contents listing limit is complete only in a
/// checkout: the clone of the listed path at the listed commit.
pub(super) fn clone_lead(query: &GhStructureQueryTree, scope: &Scope, commit_sha: &str) -> Value {
    crate::tools::gh_shared::RepoPath {
        owner: query.owner.as_str(),
        repo: query.repo.as_str(),
        path: &scope.path,
        reference: Some(commit_sha),
    }
    .lead(ToolId::GhCloneRepo)
    .build()
}

/// The read a listing leads to on its first page: the best-ranked listed
/// file ([`read_rank`](crate::tools::root_read::read_rank), the ranking
/// structureSearch shares): a source file a bare-word `include` names, else
/// the shallowest code entry file, a manifest, a README, a guide, then other
/// source. A small listing falls back to its first file. Source and docs are outlined
/// (`view:"symbols"`).
pub(super) fn entry_read(
    query: &GhStructureQueryTree,
    entries: &[TreeEntry],
    commit_sha: &str,
) -> Option<Value> {
    Some(file_read(
        query,
        entry_path(query, entries.iter())?,
        commit_sha,
    ))
}

/// The read a size-ranked page leads to: its top file, the rank's answer.
pub(super) fn top_read(
    query: &GhStructureQueryTree,
    ranked: &[TreeEntry],
    commit_sha: &str,
) -> Option<Value> {
    let top = ranked.iter().find(|entry| entry.kind == EntryKind::File)?;
    Some(file_read(query, top.path.clone(), commit_sha))
}

/// Whether the listing ranks its files by size.
pub(super) fn ranks_by_size(query: &GhStructureQueryTree) -> bool {
    query.sort.by == octocode_contract_types::GhStructureTreeSortBy::SizeBytes
}

/// `path` read at `commit_sha`; source and docs outlined.
fn file_read(query: &GhStructureQueryTree, path: String, commit_sha: &str) -> Value {
    let mut read = json!({
        "owner": query.owner.as_str(),
        "repo": query.repo.as_str(),
        "path": path,
        "ref": commit_sha,
    });
    if crate::tools::outlines(&path) {
        read["view"] = json!("symbols");
    }
    crate::response::result::Continuation::related(ToolId::GhGetFileContent, read).build()
}

/// The read a materialized listing leads to: [`entry_path`] over the entries
/// the page wrote, read with localFetch from the local copy at `local`
/// (`location.path`, the listed directory), outlined like the remote read.
pub(super) fn local_entry_read<'a>(
    query: &GhStructureQueryTree,
    written: impl Iterator<Item = &'a TreeEntry>,
    scope: &Scope,
    local: &std::path::Path,
) -> Option<Value> {
    let path = entry_path(query, written)?;
    let file = local.join(relative(&path, &scope.path));
    let mut read = json!({"path": file.to_string_lossy()});
    if crate::tools::outlines(&path) {
        read["view"] = json!("symbols");
    }
    Some(crate::response::result::Continuation::related(ToolId::LocalFetch, read).build())
}

/// The best-ranked file of `entries` to read first (see [`entry_read`]).
fn entry_path<'a>(
    query: &GhStructureQueryTree,
    entries: impl Iterator<Item = &'a TreeEntry>,
) -> Option<String> {
    let files = entries
        .filter(|entry| entry.kind == EntryKind::File)
        .collect::<Vec<_>>();
    let words = bare_words(query.include.iter().map(|word| word.as_str()));
    let base_depth = query.path.as_deref().map_or(0, |path| {
        path.split('/').filter(|part| !part.is_empty()).count()
    });
    let small = files.len() <= crate::tools::OUTLINE_LEAD_MAX_FILES;
    files
        .iter()
        .filter_map(|entry| {
            let depth = entry.path.matches('/').count().saturating_sub(base_depth);
            let rank = read_rank(&entry.path, depth, &words).or(small.then_some(RANK_ANY))?;
            Some(((rank, depth, entry.path.as_str()), entry))
        })
        .min_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, entry)| entry.path.clone())
}

/// `include` entries that are bare words (no glob metacharacter or `/`),
/// lowercased: a file whose stem equals one is the file the caller named.
fn bare_words<'a>(include: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    include
        .into_iter()
        .map(str::trim)
        .filter(|word| !word.is_empty() && octocode_engine::search::glob::is_bare_word(word))
        .map(str::to_lowercase)
        .collect()
}

/// Listing rows grouped by directory, built in one pass. `dir` is
/// repo-relative (the same base as `path` and `include`), `"."` only for
/// the repository root, which leads. Each listed entry carries its facts in
/// the structureSearch entry form, `"<name> (<fields>)"` (the last `" ("`
/// opens them): a file its size in bytes; a folder is named `"<name>/"`.
/// Names sort within each row.
pub(super) fn build_structure(entries: &[TreeEntry]) -> Vec<Value> {
    let mut dirs = std::collections::BTreeMap::<&str, Vec<(&str, String)>>::new();
    for entry in entries {
        let path = entry.path.as_str();
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
        let file = entry.kind == EntryKind::File;
        let fields = file
            .then_some(entry.size)
            .flatten()
            .map(|size| size.to_string())
            .into_iter()
            .collect::<Vec<_>>();
        let text = entry_text(name, !file, &fields);
        dirs.entry(parent).or_default().push((name, text));
    }
    let listed = |mut names: Vec<(&str, String)>| {
        names.sort_by(|left, right| left.0.cmp(right.0));
        Value::Array(
            names
                .into_iter()
                .map(|(_, text)| Value::String(text))
                .collect(),
        )
    };
    let mut rows = dirs.into_iter().collect::<Vec<_>>();
    // The root leads; the other directories follow in path order.
    rows.sort_by(|(left, _), (right, _)| {
        (*left != ".").cmp(&(*right != ".")).then(left.cmp(right))
    });
    rows.into_iter()
        .map(|(dir, row)| json!({"dir": dir, "files": listed(row)}))
        .collect()
}

/// Rank a path-ordered listing by `sort`; `false` when it asks for the path
/// order the listing already has. `sizeBytes` ranks the files whose size the
/// provider states, the rest (folders) after them; `path` descending is the
/// listing reversed. Equal sizes keep path order, so every page of one commit
/// cuts the same rank.
pub(super) fn rank(
    entries: &mut [TreeEntry],
    sort: &octocode_contract_types::GhStructureTreeSort,
) -> bool {
    use octocode_contract_types::{GhStructureTreeSortBy as By, SortOrder};
    let size = |entry: &TreeEntry| entry.size.filter(|_| entry.kind == EntryKind::File);
    match (sort.by, sort.order) {
        (By::Path, SortOrder::Asc) => return false,
        (By::Path, SortOrder::Desc) => entries.reverse(),
        (By::SizeBytes, order) => entries.sort_by(|left, right| {
            let (left_size, right_size) = (size(left), size(right));
            let sized = match order {
                SortOrder::Asc => left_size.cmp(&right_size),
                SortOrder::Desc => right_size.cmp(&left_size),
            };
            left_size
                .is_none()
                .cmp(&right_size.is_none())
                .then(sized)
                .then_with(|| left.path.cmp(&right.path))
        }),
    }
    true
}

/// Rows of a ranked listing ([`rank`]), in rank order: each run of adjacent
/// entries of one directory is a `{dir, files}` group, and a directory the
/// rank returns to opens a new one, as structureSearch lists a metric rank.
/// Entries keep the [`build_structure`] text.
pub(super) fn ranked_structure(entries: &[TreeEntry]) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    for entry in entries {
        let path = entry.path.as_str();
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
        let file = entry.kind == EntryKind::File;
        let fields = file
            .then_some(entry.size)
            .flatten()
            .map(|size| size.to_string())
            .into_iter()
            .collect::<Vec<_>>();
        let text = Value::String(entry_text(name, !file, &fields));
        match rows.last_mut().filter(|row| row["dir"] == parent) {
            Some(row) => {
                if let Some(files) = row["files"].as_array_mut() {
                    files.push(text);
                }
            }
            None => rows.push(json!({"dir": parent, "files": [text]})),
        }
    }
    rows
}

pub(super) fn relative(path: &str, root: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        path.strip_prefix(root)
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_owned()
    }
}

pub(super) fn attach_continuations(
    value: &mut Value,
    query: &GhStructureQueryTree,
    scope: &Scope,
    page: &ListingPage<'_>,
    retry_tree: bool,
    materialize_resume: Option<MaterializeResume>,
) -> Result<(), ProviderError> {
    let (current, has_more) = (page.current, page.has_more());
    if let Some(resume) = materialize_resume {
        let mut next_query = query_row(query, &["page"]);
        next_query["materialize"] = json!(true);
        next_query["materializeOffset"] = json!(resume.offset);
        value["calls"]["continueMaterialize"] = continuation(next_query);
        mark_materialize_partial(value);
    }
    // The rest of the listing itself: reading it writes nothing more.
    if has_more {
        let mut next_query = query_row(query, &[]);
        if query.materialize == Some(true)
            && let Some(row) = next_query.as_object_mut()
        {
            row.remove("materialize");
            row.remove("materializeOffset");
            // `materialize` chose this listing's depth when no `maxDepth` did:
            // without it, the next page would cut a shallower listing.
            row.insert("maxDepth".to_owned(), json!(scope.depth));
        }
        if let Some(call) = crate::response::paging::next_page_call(
            ToolId::GhStructure,
            next_query,
            "page",
            current + 1,
            &[],
        ) {
            value["calls"]["nextPage"] = call;
        }
    }
    if retry_tree
        && let Some(call) = crate::response::paging::next_page_call(
            ToolId::GhStructure,
            query_row(query, &[]),
            "page",
            current,
            &[],
        )
    {
        value["calls"]["retry"] = call;
    }
    Ok(())
}

/// An exact `ghStructure` call of `query` (a page, retry or ref listing).
pub(super) fn continuation(query: Value) -> Value {
    crate::response::result::Continuation::continued(ToolId::GhStructure, query).build()
}

pub(super) fn structure_is_empty(value: &Value) -> bool {
    value
        .get("files")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}

#[cfg(test)]
mod read_lead_tests;
