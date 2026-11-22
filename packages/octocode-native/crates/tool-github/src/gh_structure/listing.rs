//! One listing page: the pinned ref, the page window, the entries and
//! their continuations.
use super::*;
use octocode_tools_shared::root_read::entry_text;

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
        let total_pages =
            octocode_runtime_base::response::page_windows::count_pages(total_entries, per_page);
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
            value["pagination"] = octocode_runtime_base::response::pages::PageFacts::nth(
                self.current,
                self.per_page,
                self.total_entries,
            )
            .to_value();
        }
    }
}

/// File and folder counts of the whole listing (never one page's share), and
/// the bytes of the files whose size the provider states. A symlink counts
/// as a file, as in every listing of this tool.
pub(super) fn summary(entries: &[TreeEntry]) -> Value {
    let files = entries
        .iter()
        .filter(|entry| entry.kind != EntryKind::Dir)
        .count();
    let mut summary = json!({"fileCount": files, "dirCount": entries.len() - files});
    let bytes = entries
        .iter()
        .filter(|entry| entry.kind != EntryKind::Dir)
        .map(|entry| entry.size)
        .collect::<Option<Vec<_>>>();
    if let Some(sizes) = bytes {
        summary["bytes"] = json!(sizes.iter().sum::<u64>());
    }
    summary
}

/// Provider limits and skipped entries of the walk, stated on the page. A
/// limit's `terminalLimitReached` code is the shared coverage code
/// ([`octocode_runtime_base::response::paging::terminal_limit`]), never restated here.
pub(super) fn disclose_limits(traversal: &Traversal, page: &ListingPage<'_>, value: &mut Value) {
    use octocode_runtime_base::response::paging::{mark_partial, terminal_limit};
    if traversal.contents_limit {
        terminal_limit(
            value,
            octocode_runtime_base::response::paging::PartialReason::ProviderContentsLimit,
            &[],
        );
        value["providerLimit"] = json!({
            "reason": "providerContentsLimit",
            "maxEntriesPerDirectory": CONTENTS_LIMIT
        });
    }
    if !traversal.omitted.is_empty() && page.current == 1 {
        // A submodule has no listable content: name it instead of silently
        // dropping it, once per listing.
        value["omitted"] = json!(traversal.omitted);
    }
    if traversal.missed() {
        mark_partial(
            value,
            octocode_runtime_base::response::paging::PartialReason::PartialTreeFailures,
            &[],
        );
    }
    if traversal.failed_subtrees > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            ["One or more repository subdirectories could not be read; retry the same page or narrow the path/depth.".to_owned()],
        );
    }
    if traversal.unchecked > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            value,
            [format!(
                "{} file(s) were left out: no git tree of their folder answered, so a symlink among them could not be told from a file; retry the same page.",
                traversal.unchecked
            )],
        );
    }
}

/// A directory past GitHub's Contents listing limit is complete only in a
/// checkout: the clone of the listed path at the listed commit.
pub(super) fn clone_lead(query: &GhStructureQueryTree, scope: &Scope, commit_sha: &str) -> Value {
    octocode_tools_shared::gh_shared::RepoPath {
        owner: query.owner.as_str(),
        repo: query.repo.as_str(),
        path: &scope.path,
        reference: Some(commit_sha),
    }
    .lead(ToolId::GhCloneRepo)
    .build()
}

/// Whether the listing ranks its files by size. Ranking still determines
/// traversal depth even when the result offers no optional read advice.
pub(super) fn ranks_by_size(query: &GhStructureQueryTree) -> bool {
    sort_key(&query.sort).0 == SortKey::Bytes
}

/// What a tree listing ranks by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SortKey {
    Path,
    Bytes,
}

/// The key and order of `sort`, whichever form states them: each form holds
/// its keys and the order the contract gives an omitted `order` for them.
pub(super) fn sort_key(
    sort: &octocode_contract_types::GhStructureTreeSort,
) -> (SortKey, octocode_contract_types::SortOrder) {
    use octocode_contract_types::GhStructureTreeSort as Sort;
    match sort {
        Sort::Ascending(path) => (SortKey::Path, path.order),
        Sort::Descending(bytes) => (SortKey::Bytes, bytes.order),
    }
}

/// Listing rows by directory, built in one pass: a directory with several
/// entries on the page is a `{dir, files}` group, a directory with one is
/// that entry's repo-relative path ([`lone_entry`]). `dir` is
/// repo-relative (the same base as `path`), `"."` only for
/// the repository root, which leads. Each listed entry carries its facts in
/// the structureSearch entry form, `"<name> (<fields>)"` (the last `" ("`
/// opens them): a file its size in bytes; a folder is named `"<name>/"`.
/// Names sort within each row.
pub(super) fn build_structure(entries: &[TreeEntry], targets: &LinkTargets) -> Vec<Value> {
    let mut named = entries
        .iter()
        .map(|entry| {
            let path = entry.path.as_str();
            let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
            (parent, name, entry_line(entry, name, targets))
        })
        .collect::<Vec<_>>();
    // The root leads; the other directories follow in path order, and names
    // sort within each.
    named.sort_by(|left, right| {
        (left.0 != ".")
            .cmp(&(right.0 != "."))
            .then(left.0.cmp(right.0))
            .then(left.1.cmp(right.1))
    });
    response_groups(
        named.into_iter().map(|(dir, _, text)| (dir, text)),
        octocode_runtime_base::response::groups::Order::Merged,
    )
}

/// The `files` rows of `(dir, entry text)` pairs in `order`: the shared
/// `{dir, files}` grouping every listing view uses, with each group of one
/// entry as that entry's repo-relative path instead ([`lone_entry`]).
fn response_groups<'a>(
    entries: impl IntoIterator<Item = (&'a str, String)>,
    order: octocode_runtime_base::response::groups::Order,
) -> Vec<Value> {
    octocode_runtime_base::response::groups::groups(
        entries.into_iter().map(
            |(dir, text)| octocode_runtime_base::response::groups::Entry {
                dir: dir.to_owned(),
                item: Value::String(text),
            },
        ),
        order,
    )
    .into_iter()
    .map(|group| lone_entry(&group).unwrap_or(group))
    .collect()
}

/// The flat row of a group that lists one entry: the entry text under its
/// repo-relative path (`dir + "/" + name`, a root entry its name), so the
/// row states what the group stated without the group around it. A group's
/// header is longer than the `dir/` it saves only when it is shared, so a
/// group of one is always the longer form; `None` for any other group.
fn lone_entry(group: &Value) -> Option<Value> {
    let [Value::String(entry)] = group["files"].as_array()?.as_slice() else {
        return None;
    };
    Some(Value::String(
        octocode_runtime_base::response::groups::path_of(group["dir"].as_str()?, entry),
    ))
}

/// The target path of each listed symlink, by the link's repository path.
pub(super) type LinkTargets = std::collections::BTreeMap<String, String>;

/// One entry in the structureSearch form: a file its size, a folder a
/// trailing `/`, a symlink `link -> <target>` (`link` alone when its target
/// could not be read).
fn entry_line(entry: &TreeEntry, name: &str, targets: &LinkTargets) -> String {
    let fields = match entry.kind {
        EntryKind::File => entry
            .size
            .map(|size| size.to_string())
            .into_iter()
            .collect(),
        EntryKind::Dir => Vec::new(),
        EntryKind::Link => vec![match targets.get(&entry.path) {
            Some(target) => format!("link -> {target}"),
            None => "link".to_owned(),
        }],
    };
    entry_text(name, entry.kind == EntryKind::Dir, &fields)
}

/// The target of each symlink among `entries`: the link's own git blob is
/// its target path, so the read names where the link points without
/// requesting any byte of the target. A link whose blob cannot be read is
/// listed without a target.
pub(super) async fn link_targets(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    entries: &[TreeEntry],
    context: &RequestContext,
) -> Result<LinkTargets, ProviderError> {
    use futures_util::StreamExt as _;
    let reads = entries
        .iter()
        .filter(|entry| {
            entry.kind == EntryKind::Link
                && !octocode_runtime_base::policy::discovery::is_sensitive_repository_file(
                    &entry.path,
                )
        })
        .filter_map(|entry| Some((&entry.path, entry.oid.as_deref()?)))
        .map(|(path, oid)| async move {
            (
                path.clone(),
                link_target(provider, listing, oid, context).await,
            )
        });
    let answers = futures_util::stream::iter(reads)
        .buffered(read_concurrency(provider))
        .collect::<Vec<_>>()
        .await;
    let mut targets = LinkTargets::new();
    for (path, answer) in answers {
        match answer {
            Ok(Some(target)) => {
                targets.insert(path, target);
            }
            Ok(None) => {}
            Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
            Err(_) => {}
        }
    }
    Ok(targets)
}

async fn link_target(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    oid: &str,
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    use base64::Engine as _;
    let url = provider.transport.endpoint().rest(&[
        "repos",
        listing.owner,
        listing.repo,
        "git",
        "blobs",
        oid,
    ])?;
    let page = provider
        .transport
        .execute(octocode_github::RequestSpec::get(url), context)
        .await?;
    let blob = serde_json::from_slice::<Value>(&page.body).ok();
    let target = blob
        .as_ref()
        .and_then(|blob| blob["content"].as_str())
        .and_then(|content| {
            let packed: String = content.split_whitespace().collect();
            base64::engine::general_purpose::STANDARD
                .decode(packed)
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok());
    Ok(target.filter(|target| !target.is_empty() && target.len() < 4096))
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
    use SortKey as By;
    use octocode_contract_types::SortOrder;
    let size = |entry: &TreeEntry| entry.size.filter(|_| entry.kind == EntryKind::File);
    match sort_key(sort) {
        (By::Path, SortOrder::Asc) => return false,
        (By::Path, SortOrder::Desc) => entries.reverse(),
        (By::Bytes, order) => entries.sort_by(|left, right| {
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
/// entries of one directory is a `{dir, files}` group (a run of one its
/// repo-relative path, [`lone_entry`]), and a directory the rank returns to
/// opens a new row, as structureSearch lists a metric rank.
/// Entries keep the [`build_structure`] text.
pub(super) fn ranked_structure(entries: &[TreeEntry], targets: &LinkTargets) -> Vec<Value> {
    response_groups(
        entries.iter().map(|entry| {
            let path = entry.path.as_str();
            let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
            (parent, entry_line(entry, name, targets))
        }),
        octocode_runtime_base::response::groups::Order::Runs,
    )
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
        if let Some(call) = octocode_runtime_base::response::collection::continued_page(
            ToolId::GhStructure,
            next_query,
            "page",
            current + 1,
            &[],
        ) {
            octocode_runtime_base::response::collection::insert_row_walk(
                value,
                ToolId::GhStructure,
                call,
            );
        }
    }
    if retry_tree
        && let Some(call) = octocode_runtime_base::response::collection::continued_page(
            ToolId::GhStructure,
            query_row(query, &[]),
            "page",
            current,
            &[],
        )
    {
        value["calls"]["retry"] = call.build();
    }
    Ok(())
}

/// An exact `ghStructure` call of `query` (a page, retry or ref listing).
pub(super) fn continuation(query: Value) -> Value {
    octocode_runtime_base::response::result::Continuation::continued(ToolId::GhStructure, query)
        .build()
}

pub(super) fn structure_is_empty(value: &Value) -> bool {
    value
        .get("files")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}
