//! GitHub repository tree and independently paged metadata execution.
use super::{GhStructureQuery, GhStructureQueryIncludeItem};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use crate::{
    providers::github::{
        ContentsEntry, CredentialResolver, GitHubProvider, GitHubTransport, ProviderError,
        ProviderErrorKind, ProviderErrorReason, RequestContext, TreeRequest,
    },
    tools::result::ToolData,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;

/// Last metadata page a continuation may name (contract `metadataPage` maximum).
fn max_metadata_page() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "metadataPage")
}
/// Largest listing page (contract `pageSize` maximum). Path-only rows stay
/// compact: 500 entries of a deep tree render in about 13k chars.
fn max_entries_per_page() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "pageSize")
}
const CONTENTS_LIMIT: usize = 1000;
/// Upper bound on Contents API directory reads for one fallback walk (git
/// trees API truncated or unavailable). Past it the listing is a typed
/// terminal limit instead of an unbounded request fan-out.
pub(super) const MAX_DIRECTORY_FETCHES: usize = 200;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TreeEntry {
    path: String,
    kind: EntryKind,
    size: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
enum EntryKind {
    File,
    Dir,
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Traversal {
    entries: Vec<TreeEntry>,
    failed_subtrees: usize,
    contents_limit: bool,
    /// Fallback walk stopped at [`MAX_DIRECTORY_FETCHES`].
    #[serde(default)]
    fetch_limit: bool,
    /// Entries skipped by [`ignored_entry`], by name.
    #[serde(default)]
    omitted: std::collections::BTreeMap<String, usize>,
}

pub(super) async fn execute<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
    home: &Path,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    let GhStructureQuery {
        owner,
        repo,
        branch,
        path,
        max_depth,
        page,
        page_size,
        metadata_page,
        include,
        materialize,
        materialize_offset,
        pattern,
        ..
    } = query;
    let filter = pattern
        .as_deref()
        .map(|pattern| PathFilter::new(pattern))
        .transpose()?;
    let requested_path = path.as_deref().unwrap_or("").trim_matches('/');
    let clean_path = if requested_path == "." {
        String::new()
    } else {
        requested_path.to_owned()
    };
    // A name search looks at every level unless the caller bounds it.
    let depth = match (max_depth, &filter) {
        (Some(depth), _) => super::usize_of(*depth),
        (None, Some(_)) => max_listing_depth(),
        (None, None) => 1,
    };
    // Pin the listing to one commit: an explicit ref that does not resolve is
    // an error (like ghGetFileContent), never a silent default-branch
    // listing, and every page of one listing reads the same tree.
    let reference = branch.as_deref();
    // The recursive tree is fetched once, by the resolved commit SHA: a tree
    // fetched by ref name reports its tree-object SHA, which cannot prove it
    // belongs to the resolved commit.
    let (resolved_branch, commit_sha) = match reference {
        Some(branch) => {
            let sha = provider
                .resolve_reference(owner, repo, Some(branch), false, context)
                .await
                .map_err(|error| missing_ref(error, owner, repo, branch))?;
            (branch.to_owned(), sha)
        }
        None => {
            let (metadata, sha) = tokio::try_join!(
                transport.repository_metadata(owner, repo, context),
                provider.resolve_reference(owner, repo, None, false, context),
            )?;
            (metadata.default_branch, sha)
        }
    };
    let mut traversal = traverse(
        provider,
        owner,
        repo,
        &commit_sha,
        &clean_path,
        depth,
        context,
    )
    .await?;
    if let Some(filter) = &filter {
        traversal
            .entries
            .retain(|entry| filter.matches(&entry.path));
    }
    // Directory by directory, in path order: a page of a deep listing
    // carries each directory's files with its folders instead of every
    // folder of the tree before the first file.
    let parent = |path: &str| path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
    traversal.entries.sort_by(|left, right| {
        parent(&left.path)
            .cmp(&parent(&right.path))
            .then_with(|| left.path.cmp(&right.path))
    });

    let current_page = super::usize_of(*page);
    let per_page = super::usize_of(*page_size).clamp(1, max_entries_per_page());
    let total_entries = traversal.entries.len();
    let total_pages = total_entries.div_ceil(per_page).max(1);
    let start = current_page.saturating_sub(1).saturating_mul(per_page);
    let page_entries = traversal
        .entries
        .iter()
        .skip(start)
        .take(per_page)
        .cloned()
        .collect::<Vec<_>>();
    let has_more = current_page < total_pages;
    let structure = build_structure(&page_entries, &clean_path);
    let total_files = structure
        .iter()
        .map(|row| {
            row.get("files")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        })
        .sum::<usize>();
    let total_folders = structure
        .iter()
        .map(|row| {
            row.get("folders")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        })
        .sum::<usize>();
    let mut value = json!({
        "structure": structure,
        "summary": {"totalFiles": total_files, "totalFolders": total_folders},
        "resolvedBranch": resolved_branch,
    });
    if let Some(pattern) = pattern.as_deref() {
        value["summary"]["pattern"] = json!(pattern.as_str());
    }
    // A caller-supplied full SHA is not restated.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["commitSha"] = json!(commit_sha);
    }
    if total_pages > 1 {
        value["pagination"] = json!({
            "currentPage": current_page,
            "totalPages": total_pages,
            "hasMore": has_more,
            "nextPage": has_more.then_some(current_page + 1),
            "entriesPerPage": per_page,
            "totalEntries": total_entries,
        });
        remove_nulls(&mut value["pagination"]);
    }
    if include.contains(&GhStructureQueryIncludeItem::Sizes) {
        let file_sizes = page_entries
            .iter()
            .filter_map(|entry| {
                (entry.kind == EntryKind::File)
                    .then(|| {
                        entry
                            .size
                            .map(|size| (relative(&entry.path, &clean_path), json!(size)))
                    })
                    .flatten()
            })
            .collect::<Map<_, _>>();
        if !file_sizes.is_empty() {
            value["fileSizes"] = Value::Object(file_sizes);
        }
    }
    let mut materialize_resume = None;
    if *materialize == Some(true)
        && let Some(outcome) = materialize_tree(
            provider,
            owner,
            repo,
            &commit_sha,
            &page_entries,
            materialize_offset.map_or(0, |offset| usize::try_from(offset).unwrap_or(0)),
            home,
            context,
        )
        .await?
    {
        if let Some(offset) = outcome.next_offset {
            materialize_resume = Some(MaterializeResume {
                page: current_page,
                offset,
                reason: outcome.reason,
            });
        } else if has_more {
            materialize_resume = Some(MaterializeResume {
                page: current_page + 1,
                offset: 0,
                reason: "listing",
            });
        }
        value["location"] = outcome.location;
        if !outcome.warnings.is_empty() {
            value["warnings"] = json!(outcome.warnings);
        }
    }

    let metadata_page = super::usize_of(*metadata_page);
    let include: Vec<String> = include.iter().map(ToString::to_string).collect();
    let mut partial_reasons = Vec::<String>::new();
    let mut output = ToolData::from(Value::Null);
    if traversal.contents_limit {
        partial_reasons.push("providerContentsLimit".into());
        value["terminalLimit"] = json!(true);
        value["providerLimit"] = json!({
            "reason": "providerContentsLimit",
            "maxEntriesPerDirectory": CONTENTS_LIMIT,
            "completeness": "unknown"
        });
        output.diagnostics.add(
            "terminalLimitReached",
            "A GitHub Contents directory reached the 1000-entry provider limit; completeness is unknown.",
            true,
        );
    }
    if traversal.fetch_limit {
        partial_reasons.push("treeFetchLimit".into());
        value["terminalLimit"] = json!(true);
        value["providerLimit"] = json!({
            "reason": "treeFetchLimit",
            "maxDirectoryFetches": MAX_DIRECTORY_FETCHES,
            "completeness": "partial"
        });
        output.diagnostics.add(
            "terminalLimitReached",
            &format!(
                "The tree walk stopped after {MAX_DIRECTORY_FETCHES} directory reads; narrow path or lower maxDepth."
            ),
            true,
        );
    }
    if !traversal.omitted.is_empty() {
        // Dependency/VCS/build directories are skipped by design; say so
        // instead of silently dropping them.
        value["omitted"] = json!({
            "reason": "ignoredEntries",
            "entries": traversal.omitted,
        });
    }
    if traversal.failed_subtrees > 0 {
        partial_reasons.push("partialTreeFailures".into());
        output.diagnostics.add(
            "partialTreeFailures",
            "One or more repository subdirectories could not be read; retry the same page or narrow the path/depth.",
            true,
        );
    }
    fetch_metadata(
        transport,
        owner,
        repo,
        &include,
        metadata_page,
        context,
        &mut value,
        &mut partial_reasons,
        &mut output,
    )
    .await?;
    if !partial_reasons.is_empty() {
        value["isPartial"] = json!(true);
        value["partialReasons"] = json!(partial_reasons);
    }
    // Continuations read the same commit.
    let pinned = GhStructureQuery {
        branch: Some(commit_sha.clone()),
        ..query.clone()
    };
    attach_continuations(
        &mut value,
        &pinned,
        current_page,
        per_page,
        has_more,
        traversal.failed_subtrees > 0,
        *materialize == Some(true),
        materialize_resume,
    )?;
    if structure_is_empty(&value)
        && value.get("languages").is_none()
        && ["contributors", "branches", "tags"].iter().all(|key| {
            value
                .get(*key)
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
        })
        && value.get("isPartial").is_none()
    {
        output.status = Some("empty");
        if filter.is_some() {
            value["hints"] = json!([
                "No path matched pattern; try a bare word, or \"**/name\" for an exact file name."
            ]);
        }
    }
    output.data = value;
    Ok(output)
}

/// Deepest level a listing may reach (contract `maxDepth` maximum).
fn max_listing_depth() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "maxDepth")
}

/// `pattern`: a case-insensitive glob over repo-relative paths. Without a
/// `/` it matches entry names at any depth; a bare word (no glob
/// metacharacters) matches names containing it.
struct PathFilter {
    matcher: globset::GlobMatcher,
    by_name: bool,
}

impl PathFilter {
    fn new(pattern: &str) -> Result<Self, ProviderError> {
        let pattern = pattern
            .trim()
            .trim_start_matches("./")
            .trim_start_matches('/');
        let by_name = !pattern.contains('/');
        let bare = !pattern.contains(['*', '?', '[', '{']);
        let glob = match (bare, by_name) {
            (true, true) => format!("*{pattern}*"),
            (true, false) => format!("**/*{pattern}*"),
            (false, _) => pattern.to_owned(),
        };
        let matcher = globset::GlobBuilder::new(&glob)
            .literal_separator(true)
            .case_insensitive(true)
            .build()
            .map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Validation,
                    format!("Invalid pattern \"{pattern}\": {error}"),
                )
            })?
            .compile_matcher();
        Ok(Self { matcher, by_name })
    }

    fn matches(&self, path: &str) -> bool {
        if self.by_name {
            self.matcher
                .is_match(path.rsplit('/').next().unwrap_or(path))
        } else {
            self.matcher.is_match(path)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn traverse<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    branch: &str,
    root: &str,
    max_depth: usize,
    context: &RequestContext,
) -> Result<Traversal, ProviderError> {
    if max_depth > 1 {
        let resource = format!("git-tree:{owner}/{repo}:{branch}");
        if let Ok(partition) = provider.transport.cache_partition(context, None).await
            && let Some(cached) = provider.cache.get(&partition, &resource).await
            && let Ok(tree) =
                serde_json::from_slice::<crate::providers::github::TreeResponse>(&cached.bytes)
        {
            root_is_directory(&tree, root)?;
            return Ok(traversal_from_git_tree(tree, root, max_depth));
        }
        match provider
            .transport
            .get_tree(
                &TreeRequest {
                    owner: owner.into(),
                    repo: repo.into(),
                    reference: branch.into(),
                    recursive: true,
                },
                context,
            )
            .await
        {
            Ok(tree) if !tree.truncated => {
                root_is_directory(&tree, root)?;
                cache_tree(provider, context, resource, branch, &tree).await;
                return Ok(traversal_from_git_tree(tree, root, max_depth));
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind,
                    ProviderErrorKind::Authentication
                        | ProviderErrorKind::Permission
                        | ProviderErrorKind::RateLimited
                ) =>
            {
                return Err(error);
            }
            Err(_) => {}
        }
    }
    // Every tree page re-runs the walk; reuse a complete one for the cache TTL.
    let walk_key = format!("git-tree-walk:{owner}/{repo}:{branch}:{root}:{max_depth}");
    let partition = provider.transport.cache_partition(context, None).await.ok();
    if let Some(partition) = &partition
        && let Some(cached) = provider.cache.get(partition, &walk_key).await
        && let Ok(walk) = serde_json::from_slice::<Traversal>(&cached.bytes)
    {
        return Ok(walk);
    }
    let mut traversal = Traversal::default();
    let mut pending = vec![(root.to_owned(), 1usize, true)];
    let mut visited = HashSet::new();
    let mut fetches = 0usize;
    while let Some((path, depth, required)) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if fetches >= MAX_DIRECTORY_FETCHES {
            traversal.fetch_limit = true;
            break;
        }
        fetches += 1;
        let listing = match provider
            .repository_contents(owner, repo, &path, branch, context)
            .await
        {
            Ok(listing) => listing,
            Err(error)
                if !required
                    && !matches!(
                        error.kind,
                        ProviderErrorKind::Authentication
                            | ProviderErrorKind::Permission
                            | ProviderErrorKind::RateLimited
                    ) =>
            {
                traversal.failed_subtrees += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        traversal.contents_limit |= listing.raw_entry_count >= CONTENTS_LIMIT;
        // The Contents API answers a file path with a single object whose
        // path is the requested path itself; a tree cannot list a file.
        if required
            && !path.is_empty()
            && let [entry] = listing.entries.as_slice()
            && entry.path == path
            && entry.kind != "dir"
        {
            return Err(not_a_directory(&path, &entry.kind));
        }
        for entry in listing.entries {
            let Some(kind) = entry_kind(&entry) else {
                continue;
            };
            if ignored_entry(&entry.name, kind) {
                *traversal.omitted.entry(entry.name.clone()).or_default() += 1;
                continue;
            }
            let child_path = entry.path.clone();
            traversal.entries.push(TreeEntry {
                path: child_path.clone(),
                kind,
                size: entry.size,
            });
            if kind == EntryKind::Dir && depth < max_depth {
                pending.push((child_path, depth + 1, false));
            }
        }
    }
    if traversal.failed_subtrees == 0
        && let Some(partition) = &partition
        && let Ok(bytes) = serde_json::to_vec(&traversal)
    {
        provider
            .cache
            .put(
                partition,
                walk_key,
                crate::providers::github::CachedContent {
                    etag: None,
                    bytes,
                    resolved_ref: branch.to_owned(),
                },
            )
            .await;
    }
    Ok(traversal)
}

/// Keep a complete recursive tree for the cache TTL under its commit.
async fn cache_tree<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    context: &RequestContext,
    resource: String,
    commit: &str,
    tree: &crate::providers::github::TreeResponse,
) {
    if let Ok(partition) = provider.transport.cache_partition(context, None).await
        && let Ok(bytes) = serde_json::to_vec(tree)
    {
        provider
            .cache
            .put(
                &partition,
                resource,
                crate::providers::github::CachedContent {
                    etag: None,
                    bytes,
                    resolved_ref: commit.to_owned(),
                },
            )
            .await;
    }
}

/// A ref that did not resolve names the ref, like ghGetFileContent does.
fn missing_ref(error: ProviderError, owner: &str, repo: &str, reference: &str) -> ProviderError {
    if error.reason != Some(ProviderErrorReason::RefNotFound) {
        return error;
    }
    let mut missing = ProviderError::new(
        ProviderErrorKind::NotFound,
        format!("Branch, tag, or SHA not found for {owner}/{repo}: \"{reference}\""),
    )
    .with_reason(ProviderErrorReason::RefNotFound);
    missing.status = error.status;
    missing.request_id = error.request_id;
    missing
}

fn root_is_directory(
    tree: &crate::providers::github::TreeResponse,
    root: &str,
) -> Result<(), ProviderError> {
    if root.is_empty() {
        return Ok(());
    }
    match tree.tree.iter().find(|entry| entry.path == root) {
        Some(entry) if entry.kind != "tree" => Err(not_a_directory(root, &entry.kind)),
        Some(_) => Ok(()),
        // A complete tree without the root: the path does not exist at this
        // commit, as the contents API would answer (404).
        None => Err(ProviderError::new(
            ProviderErrorKind::NotFound,
            format!("Path \"{root}\" not found"),
        )),
    }
}

fn not_a_directory(path: &str, kind: &str) -> ProviderError {
    let kind = match kind {
        "blob" | "file" => "a file",
        "commit" | "submodule" => "a git submodule",
        "symlink" => "a symlink",
        _ => "an unsupported entry",
    };
    ProviderError::new(
        ProviderErrorKind::Validation,
        format!(
            "Path \"{path}\" is {kind}, not a directory; read files with ghGetFileContent, or list its parent directory."
        ),
    )
}

fn traversal_from_git_tree(
    tree: crate::providers::github::TreeResponse,
    root: &str,
    max_depth: usize,
) -> Traversal {
    let prefix = if root.is_empty() {
        String::new()
    } else {
        format!("{root}/")
    };
    let mut traversal = Traversal::default();
    for entry in tree.tree {
        let relative = if root.is_empty() {
            entry.path.clone()
        } else if entry.path == root {
            continue;
        } else if let Some(stripped) = entry.path.strip_prefix(&prefix) {
            stripped.to_owned()
        } else {
            continue;
        };
        let depth = relative.split('/').filter(|part| !part.is_empty()).count();
        if depth == 0 || depth > max_depth {
            continue;
        }
        let kind = match entry.kind.as_str() {
            "blob" => EntryKind::File,
            "tree" => EntryKind::Dir,
            _ => continue,
        };
        // Beneath an ignored directory nothing is listed, sized, or
        // materialized, as the Contents walk never descends into one; only
        // the ignored entry itself is counted.
        let (ancestors, name) = relative.rsplit_once('/').unwrap_or(("", &relative));
        if ancestors
            .split('/')
            .any(|part| ignored_entry(part, EntryKind::Dir))
        {
            continue;
        }
        if ignored_entry(name, kind) {
            *traversal.omitted.entry(name.to_owned()).or_default() += 1;
            continue;
        }
        traversal.entries.push(TreeEntry {
            path: if root.is_empty() {
                relative
            } else {
                format!("{root}/{relative}")
            },
            kind,
            size: entry.size,
        });
    }
    traversal
}

fn entry_kind(entry: &ContentsEntry) -> Option<EntryKind> {
    match entry.kind.as_str() {
        "file" => Some(EntryKind::File),
        "dir" => Some(EntryKind::Dir),
        _ => None,
    }
}

fn ignored_entry(name: &str, kind: EntryKind) -> bool {
    if kind == EntryKind::Dir {
        matches!(
            name,
            ".git" | ".hg" | ".svn" | "node_modules" | "target" | "vendor"
        )
    } else {
        matches!(name, ".DS_Store")
    }
}

fn build_structure(entries: &[TreeEntry], root: &str) -> Vec<Value> {
    let mut dirs = Map::<String, Value>::new();
    for entry in entries {
        let path = relative(&entry.path, root);
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", &path));
        let bucket = dirs
            .entry(parent.to_owned())
            .or_insert_with(|| json!({"files": [], "folders": []}));
        let key = if entry.kind == EntryKind::File {
            "files"
        } else {
            "folders"
        };
        if let Some(values) = bucket[key].as_array_mut() {
            values.push(json!(name));
        }
    }
    for bucket in dirs.values_mut() {
        for key in ["files", "folders"] {
            if let Some(values) = bucket[key].as_array_mut() {
                values.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            }
        }
    }
    let mut rows = dirs
        .into_iter()
        .map(|(dir, entry)| {
            let mut row = json!({"dir": dir});
            if entry["files"]
                .as_array()
                .is_some_and(|values| !values.is_empty())
            {
                row["files"] = entry["files"].clone();
            }
            if entry["folders"]
                .as_array()
                .is_some_and(|values| !values.is_empty())
            {
                row["folders"] = entry["folders"].clone();
            }
            row
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let left = left["dir"].as_str().unwrap_or("");
        let right = right["dir"].as_str().unwrap_or("");
        if left == "." {
            std::cmp::Ordering::Less
        } else if right == "." {
            std::cmp::Ordering::Greater
        } else {
            left.cmp(right)
        }
    });
    rows
}

fn relative(path: &str, root: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        path.strip_prefix(root)
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_owned()
    }
}

#[allow(clippy::too_many_arguments)]
async fn fetch_metadata<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    owner: &str,
    repo: &str,
    include: &[String],
    page: usize,
    context: &RequestContext,
    value: &mut Value,
    partial_reasons: &mut Vec<String>,
    output: &mut ToolData,
) -> Result<(), ProviderError> {
    for kind in include {
        match kind.as_str() {
            "languages" => match transport
                .repository_auxiliary(owner, repo, "languages", 1, 1, context)
                .await
            {
                Ok((languages, _)) => {
                    if let Some(map) = languages.as_object() {
                        value["languages"] = languages.clone();
                        if let Some((dominant, _)) = map
                            .iter()
                            .max_by_key(|(_, bytes)| bytes.as_u64().unwrap_or(0))
                        {
                            value["dominantLanguage"] = json!(dominant);
                        }
                    }
                }
                Err(error) if fatal_metadata_error(&error) => return Err(error),
                Err(_) => metadata_failed(value, "languages", 1, 1, partial_reasons, output),
            },
            "contributors" => match transport
                .repository_auxiliary(owner, repo, kind, page, 30, context)
                .await
            {
                Ok((items, more)) => {
                    let items = items
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|item| {
                            Some(json!({
                                "login": item.get("login")?.as_str()?,
                                "contributions": item.get("contributions").and_then(Value::as_u64).unwrap_or(0)
                            }))
                        })
                        .collect::<Vec<_>>();
                    value["contributors"] = json!(items);
                    value["returnedContributors"] = json!(items.len());
                    add_metadata_page(
                        value,
                        kind,
                        page,
                        30,
                        items.len(),
                        more,
                        partial_reasons,
                        output,
                    );
                }
                Err(error) if fatal_metadata_error(&error) => return Err(error),
                Err(_) => metadata_failed(value, kind, page, 30, partial_reasons, output),
            },
            "branches" | "tags" => {
                let per = if kind == "branches" { 100 } else { 50 };
                match transport
                    .repository_auxiliary(owner, repo, kind, page, per, context)
                    .await
                {
                    Ok((items, more)) => {
                        let shaped = items
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|item| {
                                if kind == "branches" {
                                    item.get("name").cloned()
                                } else {
                                    Some(json!({
                                        "name": item.get("name")?,
                                        "sha": item.pointer("/commit/sha")?
                                    }))
                                }
                            })
                            .collect::<Vec<_>>();
                        value[kind] = json!(shaped);
                        value[if kind == "branches" {
                            "returnedBranches"
                        } else {
                            "returnedTags"
                        }] = json!(shaped.len());
                        add_metadata_page(
                            value,
                            kind,
                            page,
                            per,
                            shaped.len(),
                            more,
                            partial_reasons,
                            output,
                        );
                    }
                    Err(error) if fatal_metadata_error(&error) => return Err(error),
                    Err(_) => metadata_failed(value, kind, page, per, partial_reasons, output),
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn fatal_metadata_error(error: &ProviderError) -> bool {
    matches!(
        error.kind,
        ProviderErrorKind::Cancelled
            | ProviderErrorKind::Timeout
            | ProviderErrorKind::ResponseTooLarge
            | ProviderErrorKind::Authentication
            | ProviderErrorKind::Permission
            | ProviderErrorKind::RateLimited
    )
}

#[allow(clippy::too_many_arguments)]
fn add_metadata_page(
    value: &mut Value,
    kind: &str,
    page: usize,
    per: usize,
    returned: usize,
    more: bool,
    partial_reasons: &mut Vec<String>,
    output: &mut ToolData,
) {
    value["metadataPagination"][kind] = json!({
        "currentPage": page,
        "perPage": per,
        "returned": returned,
        "hasMore": more,
        "terminalLimit": (more && page >= max_metadata_page()).then_some(true)
    });
    remove_nulls(&mut value["metadataPagination"][kind]);
    if more {
        if page >= max_metadata_page() {
            push_reason(partial_reasons, "metadataPageLimit");
            value["terminalLimit"] = json!(true);
            output.diagnostics.add(
                "terminalLimitReached",
                "GitHub metadata pagination reached the schema page limit.",
                true,
            );
        } else {
            push_reason(partial_reasons, "metadataPagination");
            output.diagnostics.partial = true;
        }
    }
}
fn metadata_failed(
    value: &mut Value,
    kind: &str,
    page: usize,
    per: usize,
    partial_reasons: &mut Vec<String>,
    output: &mut ToolData,
) {
    value["metadataPagination"][kind] = json!({
        "currentPage": page,
        "perPage": per,
        "returned": 0,
        "hasMore": false,
        "failed": true
    });
    push_reason(partial_reasons, "metadataFetchFailed");
    output.diagnostics.add(
        "metadataFetchFailed",
        "One or more requested GitHub metadata collections could not be read.",
        true,
    );
}

#[derive(Clone, Copy)]
struct MaterializeResume {
    page: usize,
    offset: usize,
    reason: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn attach_continuations(
    value: &mut Value,
    query: &GhStructureQuery,
    page: usize,
    page_size: usize,
    has_more: bool,
    retry_tree: bool,
    materialize: bool,
    materialize_resume: Option<MaterializeResume>,
) -> Result<(), ProviderError> {
    let materialize_resume = materialize_resume.or_else(|| {
        (has_more && materialize).then_some(MaterializeResume {
            page: page + 1,
            offset: 0,
            reason: "listing",
        })
    });
    if materialize_resume.is_some_and(|resume| resume.page > 1000)
        || (has_more && !materialize && page >= 1000)
    {
        value["terminalLimit"] = json!(true);
        if let Some(location) = value.get_mut("location") {
            location["hasMore"] = json!(true);
            location["complete"] = json!(false);
        }
    } else if let Some(resume) = materialize_resume {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(resume.page);
        next_query["pageSize"] = json!(page_size);
        next_query["materialize"] = json!(true);
        next_query["materializeOffset"] = json!(resume.offset);
        value["next"]["continueMaterialize"] = continuation(
            next_query,
            format!(
                "Continue writing tree files from listing offset {} ({})",
                resume.offset, resume.reason
            ),
            "exact",
        );
        if let Some(location) = value.get_mut("location") {
            location["hasMore"] = json!(true);
            location["complete"] = json!(false);
        }
    } else if has_more && !materialize {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(page + 1);
        next_query["pageSize"] = json!(page_size);
        // Metadata collections page independently (next.<kind>); only the
        // per-page `sizes` include belongs to the listing continuation.
        if let Some(object) = next_query.as_object_mut() {
            object.remove("metadataPage");
            let sizes = object
                .get("include")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value == "sizes"));
            if sizes {
                object.insert("include".into(), json!(["sizes"]));
            } else {
                object.remove("include");
            }
        }
        value["next"]["nextPage"] = continuation(
            next_query,
            format!("Continue tree results on page {}.", page + 1),
            "exact",
        );
    }
    if retry_tree {
        let mut retry = public_query(query)?;
        retry["page"] = json!(page);
        retry["pageSize"] = json!(page_size);
        value["next"]["retry"] = continuation(
            retry,
            "Retry the same tree provider page because the provider reported incomplete results.",
            "exact",
        );
    }
    let metadata = value
        .get("metadataPagination")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (kind, state) in metadata {
        let failed = state.get("failed").and_then(Value::as_bool) == Some(true);
        let more = state.get("hasMore").and_then(Value::as_bool) == Some(true);
        let terminal = state.get("terminalLimit").and_then(Value::as_bool) == Some(true);
        if terminal || (!failed && !more) {
            continue;
        }
        let current = state
            .get("currentPage")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        let mut next_query = base_tree_query(query, page, page_size)?;
        next_query["include"] = json!([kind]);
        next_query["metadataPage"] = json!(if failed { current } else { current + 1 });
        value["next"][&kind] = continuation(
            next_query,
            if failed {
                format!("Retry the failed {kind} page.")
            } else {
                format!("Continue the {kind} list.")
            },
            "exact",
        );
    }
    Ok(())
}
fn base_tree_query(
    query: &GhStructureQuery,
    page: usize,
    page_size: usize,
) -> Result<Value, ProviderError> {
    let mut query = public_query(query)?;
    query["page"] = json!(page);
    query["pageSize"] = json!(page_size);
    Ok(query)
}
fn public_query(query: &GhStructureQuery) -> Result<Value, ProviderError> {
    let mut query = serde_json::to_value(query)
        .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
    remove_nulls(&mut query);
    Ok(query)
}
fn continuation(query: Value, why: impl Into<String>, confidence: &str) -> Value {
    json!({
        "tool": ToolId::GhStructure.as_str(),
        "query": query,
        "why": why.into(),
        "confidence": confidence
    })
}
fn push_reason(reasons: &mut Vec<String>, value: &str) {
    if !reasons.iter().any(|reason| reason == value) {
        reasons.push(value.into());
    }
}
fn structure_is_empty(value: &Value) -> bool {
    value
        .get("structure")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}

const MATERIALIZE_FILE_CAP: usize = 50;
const MATERIALIZE_FILE_BYTES: usize = 300 * 1024;
const MATERIALIZE_TOTAL_BYTES: usize = 5 * 1024 * 1024;

struct MaterializeOutcome {
    location: Value,
    next_offset: Option<usize>,
    reason: &'static str,
    warnings: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
async fn materialize_tree<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    branch: &str,
    entries: &[TreeEntry],
    offset: usize,
    home: &Path,
    context: &RequestContext,
) -> Result<Option<MaterializeOutcome>, ProviderError> {
    if offset >= entries.len() {
        return Ok(None);
    }
    let root = home
        .join("tmp")
        .join("tree")
        .join(owner)
        .join(repo)
        .join(branch);
    tokio::fs::create_dir_all(&root).await.map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::Validation,
            format!("failed to create materialize directory: {error}"),
        )
    })?;
    let mut written = 0usize;
    let mut total_bytes = 0usize;
    let mut cursor = offset;
    let mut reason = "listing";
    let mut warnings = Vec::new();
    while cursor < entries.len() {
        if written >= MATERIALIZE_FILE_CAP {
            reason = "writeCap";
            break;
        }
        if total_bytes >= MATERIALIZE_TOTAL_BYTES {
            reason = "totalSize";
            break;
        }
        let entry = &entries[cursor];
        cursor += 1;
        if entry.kind != EntryKind::File {
            continue;
        }
        if entry
            .size
            .is_some_and(|size| size > MATERIALIZE_FILE_BYTES as u64)
        {
            warnings.push(format!(
                "Skipped {}: larger than the {} KiB materialize per-file limit.",
                entry.path,
                MATERIALIZE_FILE_BYTES / 1024
            ));
            continue;
        }
        // One unreadable file (binary, oversized, unsupported entry) must not
        // abort the batch; only auth/rate/timeout/cancel failures propagate.
        let acquired = match provider
            .get_file_content(
                &crate::providers::github::ContentRequest {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                    path: entry.path.clone(),
                    reference: Some(branch.to_owned()),
                    force_refresh: false,
                    session_id: None,
                },
                context,
            )
            .await
        {
            Ok(acquired) => acquired,
            Err(error)
                if !matches!(
                    error.kind,
                    ProviderErrorKind::Authentication
                        | ProviderErrorKind::Permission
                        | ProviderErrorKind::RateLimited
                        | ProviderErrorKind::Timeout
                        | ProviderErrorKind::Cancelled
                ) =>
            {
                warnings.push(format!("Skipped {}: {}.", entry.path, error.message));
                continue;
            }
            Err(error) => return Err(error),
        };
        if acquired.bytes.len() > MATERIALIZE_FILE_BYTES {
            warnings.push(format!(
                "Skipped {}: larger than the {} KiB materialize per-file limit.",
                entry.path,
                MATERIALIZE_FILE_BYTES / 1024
            ));
            continue;
        }
        if total_bytes.saturating_add(acquired.bytes.len()) > MATERIALIZE_TOTAL_BYTES {
            reason = "totalSize";
            cursor -= 1;
            break;
        }
        let dest = root.join(&entry.path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Validation,
                    format!("failed to create materialize path: {error}"),
                )
            })?;
        }
        tokio::fs::write(&dest, &acquired.bytes)
            .await
            .map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Validation,
                    format!("failed to write materialized file: {error}"),
                )
            })?;
        total_bytes += acquired.bytes.len();
        written += 1;
    }
    let has_more = cursor < entries.len();
    let location = json!({
        "kind": "local",
        "localPath": root.to_string_lossy(),
        "source": "github-tree",
        "cached": false,
        "complete": !has_more,
        "hasMore": has_more,
        "resolvedBranch": branch,
    });
    Ok(Some(MaterializeOutcome {
        location,
        next_offset: has_more.then_some(cursor),
        reason,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn materialization_page_boundary_continues_and_page_ceiling_is_explicit() {
        let query: GhStructureQuery = serde_json::from_value(json!({
            "owner": "a", "repo": "b", "branch": "a".repeat(40),
            "path": "", "goal": "Read every materialized file", "reasoning": "Preserve the remaining listing"
        })).expect("query");
        let mut value = json!({"structure": [{"dir": ".", "files": ["a.rs"]}]});
        attach_continuations(&mut value, &query, 1, 1, true, false, true, None)
            .expect("continuation");
        assert_eq!(value["next"]["continueMaterialize"]["query"]["page"], 2);
        assert_eq!(
            value["next"]["continueMaterialize"]["query"]["materializeOffset"],
            0
        );
        assert_eq!(
            value["next"]["continueMaterialize"]["query"]["branch"],
            "a".repeat(40)
        );
        for materialize in [false, true] {
            let mut value = json!({"structure": [{"dir": ".", "files": ["last.rs"]}]});
            attach_continuations(&mut value, &query, 1000, 1, true, false, materialize, None)
                .expect("terminal");
            assert_eq!(value["terminalLimit"], true);
            assert!(value.get("next").is_none());
            assert_eq!(value["structure"][0]["files"][0], "last.rs");
        }
    }

    /// D9: a recursive git-tree listing of a path the tree lacks is a
    /// missing path (not an empty listing), so the viewTree recovery runs.
    #[test]
    fn a_root_missing_from_the_git_tree_is_not_found() {
        let tree: crate::providers::github::TreeResponse = serde_json::from_value(json!({
            "sha":"s","truncated":false,"tree":[
                {"path":"src","type":"tree"},{"path":"src/lib.rs","type":"blob"}
            ]
        }))
        .expect("tree");
        assert!(root_is_directory(&tree, "").is_ok());
        assert!(root_is_directory(&tree, "src").is_ok());
        let missing = root_is_directory(&tree, "no/such").expect_err("missing");
        assert_eq!(missing.kind, ProviderErrorKind::NotFound);
        assert_eq!(missing.reason, None);
        let file = root_is_directory(&tree, "src/lib.rs").expect_err("file");
        assert_eq!(file.kind, ProviderErrorKind::Validation);
    }
    #[test]
    fn skips_symlinks_and_submodules() {
        for kind in ["symlink", "submodule"] {
            assert!(
                entry_kind(&ContentsEntry {
                    name: "x".into(),
                    path: "x".into(),
                    kind: kind.into(),
                    size: None,
                    sha: None,
                })
                .is_none()
            );
        }
    }
    #[test]
    fn structure_paths_are_relative_to_scope() {
        let rows = build_structure(
            &[
                TreeEntry {
                    path: "src/lib.rs".into(),
                    kind: EntryKind::File,
                    size: Some(1),
                },
                TreeEntry {
                    path: "src/nested".into(),
                    kind: EntryKind::Dir,
                    size: None,
                },
            ],
            "src",
        );
        assert_eq!(
            rows[0],
            json!({"dir":".","files":["lib.rs"],"folders":["nested"]})
        );
    }
}
