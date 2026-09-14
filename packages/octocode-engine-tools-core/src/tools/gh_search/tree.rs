//! GitHub repository tree and independently paged metadata execution.
use super::{GhSearchQuery, MaterializeEnv};
use crate::{
    providers::github::{
        ContentsEntry, CredentialResolver, GitHubProvider, GitHubTransport, ProviderError,
        ProviderErrorKind, RequestContext, TreeRequest, ensure_snapshot_directory,
        publish_tree_snapshot, tree_cache_root, write_snapshot_file,
    },
    tools::result::ToolData,
};
use futures_util::{StreamExt, stream};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

const MAX_DIRECTORY_FILES: usize = 50;
const MAX_TOTAL_SIZE: u64 = 5 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 300 * 1024;
const MATERIALIZE_CONCURRENCY: usize = 5;
const BINARY_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp", ".mp3", ".mp4", ".wav", ".avi",
    ".mov", ".mkv", ".webm", ".zip", ".tar", ".gz", ".bz2", ".7z", ".rar", ".xz", ".exe", ".dll",
    ".so", ".dylib", ".bin", ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".woff",
    ".woff2", ".ttf", ".eot", ".otf", ".pyc", ".class", ".o", ".obj",
];

const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE: usize = 1000;
const CONTENTS_LIMIT: usize = 1000;

#[derive(Clone, Debug)]
struct TreeEntry {
    path: String,
    kind: EntryKind,
    size: Option<u64>,
    sha: Option<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    File,
    Dir,
}
#[derive(Default)]
struct Traversal {
    entries: Vec<TreeEntry>,
    failed_subtrees: usize,
    contents_limit: bool,
}

pub(super) async fn execute<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhSearchQuery,
    context: &RequestContext,
    materialize_env: &MaterializeEnv<'_>,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    let GhSearchQuery::Tree {
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
    } = query
    else {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "expected tree query",
        ));
    };
    let want_materialize = *materialize == Some(true);
    let requested_branch = branch.clone();
    let mut resolved_branch = match branch {
        Some(branch) => branch.clone(),
        None => {
            transport
                .repository_metadata(owner, repo, context)
                .await?
                .default_branch
        }
    };
    let requested_path = path.as_deref().unwrap_or("").trim_matches('/');
    let clean_path = if requested_path == "." {
        String::new()
    } else {
        requested_path.to_owned()
    };
    let depth = max_depth.unwrap_or(1).max(1);
    let mut fallback = None;
    let mut traversal = match traverse(
        provider,
        owner,
        repo,
        &resolved_branch,
        &clean_path,
        depth,
        context,
    )
    .await
    {
        Ok(value) => value,
        Err(error) if error.status == Some(404) && requested_branch.is_some() => {
            let actual = transport
                .repository_metadata(owner, repo, context)
                .await?
                .default_branch;
            if actual == resolved_branch {
                return Err(error);
            }
            let value =
                traverse(provider, owner, repo, &actual, &clean_path, depth, context).await?;
            fallback = Some(json!({
                "requestedBranch": resolved_branch,
                "actualBranch": actual,
                "warning": format!(
                    "Branch/ref '{}' was not found. Showing '{}' (default branch) instead. Re-query with the correct branch name if branch-specific results are required.",
                    resolved_branch, actual
                )
            }));
            resolved_branch = actual;
            value
        }
        Err(error) => return Err(error),
    };
    traversal.entries.sort_by(|left, right| {
        let left_dir = left.kind == EntryKind::Dir;
        let right_dir = right.kind == EntryKind::Dir;
        right_dir
            .cmp(&left_dir)
            .then_with(|| path_depth(&left.path).cmp(&path_depth(&right.path)))
            .then_with(|| left.path.cmp(&right.path))
    });

    let current_page = page.unwrap_or(1);
    let per_page = page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 200);
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
    let mut structure = build_structure(&page_entries, &clean_path);
    filter_structure(&mut structure);
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
        "operation": "tree",
        "structure": structure,
        "summary": {"totalFiles": total_files, "totalFolders": total_folders},
        "resolvedBranch": resolved_branch,
    });
    if !want_materialize && total_pages > 1 {
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
    if include
        .as_ref()
        .is_some_and(|values| values.iter().any(|value| value == "sizes"))
    {
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
    if let Some(fallback) = fallback {
        value["branchFallback"] = fallback;
    }

    let metadata_page = metadata_page.unwrap_or(1);
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
        include.as_deref().unwrap_or(&[]),
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
    attach_continuations(
        &mut value,
        query,
        current_page,
        per_page,
        has_more && !want_materialize,
        traversal.failed_subtrees > 0,
    )?;
    if want_materialize {
        materialize_tree(
            provider,
            query,
            materialize_env,
            owner,
            repo,
            &resolved_branch,
            &clean_path,
            &page_entries,
            current_page,
            per_page,
            has_more,
            materialize_offset.unwrap_or(0),
            context,
            &mut value,
            &mut output,
        )
        .await?;
    }
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
    }
    output.data = value;
    Ok(output)
}

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
                if let Ok(partition) = provider.transport.cache_partition(context, None).await
                    && let Ok(bytes) = serde_json::to_vec(&tree)
                {
                    provider
                        .cache
                        .put(
                            &partition,
                            resource,
                            crate::providers::github::CachedContent {
                                etag: None,
                                bytes,
                                resolved_ref: branch.to_owned(),
                            },
                        )
                        .await;
                }
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
    let mut traversal = Traversal::default();
    let mut pending = vec![(root.to_owned(), 1usize, true)];
    let mut visited = HashSet::new();
    while let Some((path, depth, required)) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
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
        for entry in listing.entries {
            let Some(kind) = entry_kind(&entry) else {
                continue;
            };
            if ignored_entry(&entry.name, kind) {
                continue;
            }
            let child_path = entry.path.clone();
            traversal.entries.push(TreeEntry {
                path: child_path.clone(),
                kind,
                size: entry.size,
                sha: entry.sha,
            });
            if kind == EntryKind::Dir && depth < max_depth {
                pending.push((child_path, depth + 1, false));
            }
        }
    }
    Ok(traversal)
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
        let name = relative.rsplit('/').next().unwrap_or(&relative);
        if ignored_entry(name, kind) {
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
            sha: entry.sha,
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

fn filter_structure(rows: &mut Vec<Value>) {
    rows.retain(|row| {
        let Some(dir) = row.get("dir").and_then(Value::as_str) else {
            return false;
        };
        dir == "."
            || !dir
                .split('/')
                .any(|part| ignored_entry(part, EntryKind::Dir))
    });
}
fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
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

#[derive(Clone, Copy)]
enum MaterializeBound {
    WriteCap,
    TotalSize,
    Listing,
}

struct MaterializePlan {
    selected: Vec<usize>,
    next_offset: usize,
    skipped_binary: usize,
    skipped_too_large: usize,
    skipped_limit: usize,
    listing_exhausted: bool,
}

fn is_binary_name(name: &str) -> bool {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| format!(".{}", ext.to_ascii_lowercase()))
        .unwrap_or_default();
    BINARY_EXTENSIONS.contains(&ext.as_str())
}

fn plan_materialize(entries: &[TreeEntry], offset: usize) -> MaterializePlan {
    let mut plan = MaterializePlan {
        selected: Vec::new(),
        next_offset: offset.min(entries.len()),
        skipped_binary: 0,
        skipped_too_large: 0,
        skipped_limit: 0,
        listing_exhausted: false,
    };
    let mut index = offset.min(entries.len());
    while index < entries.len() && plan.selected.len() < MAX_DIRECTORY_FILES {
        let entry = &entries[index];
        if entry.kind == EntryKind::Dir {
            index += 1;
            continue;
        }
        let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        if is_binary_name(name) {
            plan.skipped_binary += 1;
            index += 1;
            continue;
        }
        if entry.size.is_some_and(|size| size > MAX_FILE_SIZE) {
            plan.skipped_too_large += 1;
            index += 1;
            continue;
        }
        plan.selected.push(index);
        index += 1;
    }
    plan.next_offset = index;
    plan.listing_exhausted = index >= entries.len();
    plan
}

#[allow(clippy::too_many_arguments)]
async fn materialize_tree<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    query: &GhSearchQuery,
    env: &MaterializeEnv<'_>,
    owner: &str,
    repo: &str,
    resolved_branch: &str,
    clean_path: &str,
    page_entries: &[TreeEntry],
    page: usize,
    page_size: usize,
    listing_has_more: bool,
    offset: usize,
    context: &RequestContext,
    value: &mut Value,
    output: &mut ToolData,
) -> Result<(), ProviderError> {
    if !env.persistent {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "Tree materialization requires persistent local storage. Set storage.mode=\"persistent\" or OCTOCODE_STORAGE_MODE=persistent.",
        ));
    }
    let commit_sha = provider
        .transport
        .resolve_commit_sha(owner, repo, resolved_branch, context)
        .await?;
    let cache_root = tree_cache_root(env.home, owner, repo, &commit_sha);
    env.paths
        .validate_output(&cache_root)
        .map_err(|error| ProviderError::new(ProviderErrorKind::Validation, error.message))?;
    let mut plan = plan_materialize(page_entries, offset);
    let selected: Vec<(usize, TreeEntry)> = plan
        .selected
        .iter()
        .filter_map(|index| {
            page_entries
                .get(*index)
                .cloned()
                .map(|entry| (*index, entry))
        })
        .collect();
    let fetched = stream::iter(selected)
        .map(|(index, entry)| async move {
            let sha = entry.sha.clone();
            let result = match sha.as_deref() {
                Some(sha) => provider.transport.get_blob(owner, repo, sha, context).await,
                None => Err(ProviderError::new(
                    ProviderErrorKind::Decode,
                    "tree entry is missing a blob SHA",
                )),
            };
            (index, entry, result)
        })
        .buffered(MATERIALIZE_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    let mut written: Vec<(usize, String, Vec<u8>)> = Vec::new();
    let mut bytes_this_call = 0_u64;
    let mut bound = MaterializeBound::Listing;
    let mut resume_offset = offset.min(page_entries.len());
    for (index, entry, result) in fetched {
        let bytes = match result {
            Ok(bytes) if bytes.len() as u64 > MAX_FILE_SIZE => {
                plan.skipped_too_large += 1;
                resume_offset = index + 1;
                continue;
            }
            Ok(bytes) => bytes,
            Err(error) if error.message.as_ref() == "binary files are not supported" => {
                plan.skipped_binary += 1;
                resume_offset = index + 1;
                continue;
            }
            Err(_) => {
                plan.skipped_limit += 1;
                resume_offset = index + 1;
                continue;
            }
        };
        let size = bytes.len() as u64;
        if bytes_this_call.saturating_add(size) > MAX_TOTAL_SIZE {
            bound = MaterializeBound::TotalSize;
            resume_offset = index;
            break;
        }
        bytes_this_call += size;
        written.push((index, entry.path, bytes));
        resume_offset = index + 1;
        if written.len() >= MAX_DIRECTORY_FILES {
            bound = MaterializeBound::WriteCap;
            resume_offset = plan.next_offset;
            break;
        }
    }
    if resume_offset < page_entries.len() && matches!(bound, MaterializeBound::Listing) {
        bound = MaterializeBound::WriteCap;
    }
    let page_done = resume_offset >= page_entries.len();
    if page_done {
        bound = MaterializeBound::Listing;
        resume_offset = page_entries.len();
    }
    let complete = page_done && !listing_has_more;
    let has_more = !complete;
    let snapshot = publish_tree_snapshot(
        env.home,
        &cache_root,
        owner,
        repo,
        resolved_branch,
        &commit_sha,
        |root| {
            ensure_snapshot_directory(root, clean_path)?;
            for (_, path, bytes) in &written {
                write_snapshot_file(root, path, bytes)?;
            }
            Ok(())
        },
    )?;
    env.paths
        .validate_output(&snapshot)
        .map_err(|error| ProviderError::new(ProviderErrorKind::Validation, error.message))?;
    let local_path = if clean_path.is_empty() {
        snapshot
    } else {
        snapshot.join(clean_path)
    };
    let requested_path = if clean_path.is_empty() {
        ".".to_owned()
    } else {
        clean_path.to_owned()
    };
    let reason = match bound {
        MaterializeBound::WriteCap if has_more => "writeCap",
        MaterializeBound::TotalSize if has_more => "totalSize",
        _ if has_more => "listing",
        _ => "listing",
    };
    value["location"] = json!({
        "kind": "tree",
        "localPath": local_path.to_string_lossy(),
        "source": "github",
        "cached": false,
        "commitSha": commit_sha,
        "verified": true,
        "complete": complete,
        "resolvedBranch": resolved_branch,
        "requestedPath": requested_path,
    });
    value["skipped"] = json!({
        "binary": plan.skipped_binary,
        "tooLarge": plan.skipped_too_large,
        "limit": plan.skipped_limit,
    });
    let continue_offset = if page_done { 0 } else { resume_offset };
    let continue_page = if page_done && listing_has_more {
        page + 1
    } else {
        page
    };
    value["pagination"] = json!({
        "hasMore": has_more,
        "reason": reason,
        "page": page,
        "pageSize": page_size,
        "materializeOffset": if has_more { continue_offset } else { resume_offset },
        "written": written.len(),
    });
    if has_more {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(continue_page);
        next_query["pageSize"] = json!(page_size);
        next_query["materialize"] = json!(true);
        next_query["materializeOffset"] = json!(continue_offset);
        value["next"]["continueMaterialize"] = continuation(
            next_query,
            "Continue writing this tree listing page under location.localPath.",
            "exact",
        );
        output.diagnostics.add(
            "partialTreeMaterialize",
            "Copy location.localPath into localSearch.path",
            true,
        );
        output.diagnostics.partial = true;
    } else if let Some(next) = value.get_mut("next").and_then(Value::as_object_mut) {
        next.remove("continueMaterialize");
        next.remove("nextPage");
    }
    Ok(())
}

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
        "terminalLimit": (more && page >= MAX_PAGE).then_some(true)
    });
    remove_nulls(&mut value["metadataPagination"][kind]);
    if more {
        if page >= MAX_PAGE {
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

fn attach_continuations(
    value: &mut Value,
    query: &GhSearchQuery,
    page: usize,
    page_size: usize,
    has_more: bool,
    retry_tree: bool,
) -> Result<(), ProviderError> {
    if has_more {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(page + 1);
        next_query["pageSize"] = json!(page_size);
        next_query["metadataPage"] = json!(1);
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
    query: &GhSearchQuery,
    page: usize,
    page_size: usize,
) -> Result<Value, ProviderError> {
    let mut query = public_query(query)?;
    query["page"] = json!(page);
    query["pageSize"] = json!(page_size);
    Ok(query)
}
fn public_query(query: &GhSearchQuery) -> Result<Value, ProviderError> {
    let mut query = serde_json::to_value(query)
        .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
    remove_nulls(&mut query);
    Ok(query)
}
fn continuation(query: Value, why: impl Into<String>, confidence: &str) -> Value {
    json!({
        "tool": "ghSearch",
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
fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, value| !value.is_null());
            for value in map.values_mut() {
                remove_nulls(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                remove_nulls(value);
            }
        }
        _ => {}
    }
}
fn structure_is_empty(value: &Value) -> bool {
    value
        .get("structure")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    sha: None,
                },
                TreeEntry {
                    path: "src/nested".into(),
                    kind: EntryKind::Dir,
                    size: None,
                    sha: None,
                },
            ],
            "src",
        );
        assert_eq!(
            rows[0],
            json!({"dir":".","files":["lib.rs"],"folders":["nested"]})
        );
    }

    fn file_entry(path: &str, size: u64) -> TreeEntry {
        TreeEntry {
            path: path.into(),
            kind: EntryKind::File,
            size: Some(size),
            sha: Some("a".repeat(40)),
        }
    }

    #[test]
    fn plan_stops_at_fifty_files_and_keeps_listing_offset() {
        let entries = (0..120)
            .map(|index| file_entry(&format!("f{index:03}.rs"), 1024))
            .collect::<Vec<_>>();
        let first = plan_materialize(&entries[..100], 0);
        assert_eq!(first.selected.len(), 50);
        assert_eq!(first.next_offset, 50);
        assert!(!first.listing_exhausted);
        let second = plan_materialize(&entries[..100], 50);
        assert_eq!(second.selected, (50..100).collect::<Vec<_>>());
        assert!(second.listing_exhausted);
    }

    #[test]
    fn plan_skips_directories_binaries_and_oversize_but_consumes_offset() {
        let entries = vec![
            TreeEntry {
                path: "src".into(),
                kind: EntryKind::Dir,
                size: None,
                sha: None,
            },
            file_entry("a.rs", 10),
            file_entry("photo.png", 10),
            file_entry("huge.rs", MAX_FILE_SIZE + 1),
            file_entry("b.rs", 10),
        ];
        let plan = plan_materialize(&entries, 0);
        assert_eq!(plan.selected, vec![1, 4]);
        assert_eq!(plan.skipped_binary, 1);
        assert_eq!(plan.skipped_too_large, 1);
        assert!(plan.listing_exhausted);
    }

    fn blob_sha(index: usize) -> String {
        format!("{:040x}", index + 1)
    }

    fn listing_entry(index: usize) -> serde_json::Value {
        json!({
            "name": format!("f{index:03}.rs"),
            "path": format!("f{index:03}.rs"),
            "type": "file",
            "size": 16,
            "sha": blob_sha(index)
        })
    }

    #[tokio::test]
    async fn materialize_120_blobs_copy_forwards_and_keeps_page() {
        use crate::policy::path::{PathPolicy, PathPolicyConfig};
        use crate::providers::github::{
            CredentialSource, GitHubEndpoint, GitHubProvider, NoCache, RetryPolicy,
            StaticCredentialResolver,
        };
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use std::fs;
        use std::path::Path;
        use std::sync::Arc;
        use std::time::Duration;
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let entries = (0..120).map(listing_entry).collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits/main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": commit})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/contents"))
            .and(query_param("ref", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(entries)))
            .mount(&server)
            .await;
        for index in 0..120 {
            let body = format!("blob-{index:03}\n");
            Mock::given(method("GET"))
                .and(path(format!(
                    "/api/v3/repos/o/r/git/blobs/{}",
                    blob_sha(index)
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "encoding": "base64",
                    "content": STANDARD.encode(&body),
                    "sha": blob_sha(index)
                })))
                .mount(&server)
                .await;
        }
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
                .expect("endpoint");
        let transport = crate::providers::github::GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::new(
                "secret",
                CredentialSource::Environment,
            )),
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_retry_after: Duration::from_secs(1),
            },
        )
        .expect("transport");
        let provider = GitHubProvider {
            transport,
            cache: NoCache,
        };
        let home = tempfile::TempDir::new().expect("home");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: None,
            additional_roots: vec![home.path().to_path_buf()],
            include_home: true,
            home_dir: Some(home.path().to_path_buf()),
        })
        .expect("policy");
        let env = super::super::MaterializeEnv {
            home: home.path(),
            persistent: true,
            paths: &paths,
        };
        let context = RequestContext::with_timeout(Duration::from_secs(30), 16 * 1024 * 1024);
        let first_query = serde_json::from_value::<super::super::GhSearchQuery>(json!({
            "operation": "tree",
            "owner": "o",
            "repo": "r",
            "branch": "main",
            "materialize": true
        }))
        .expect("query");
        let first = execute(&provider, &first_query, &context, &env)
            .await
            .expect("first materialize");
        assert_eq!(first.data["pagination"]["page"], 1);
        assert_eq!(first.data["pagination"]["written"], 50);
        assert_eq!(first.data["pagination"]["materializeOffset"], 50);
        assert_eq!(first.data["pagination"]["reason"], "writeCap");
        assert_eq!(first.data["pagination"]["hasMore"], true);
        assert!(first.data["next"]["nextPage"].is_null());
        assert!(first.data["next"]["searchLocal"].is_null());
        assert_eq!(
            first.data["next"]["continueMaterialize"]["query"]["page"],
            1
        );
        assert_eq!(
            first.data["next"]["continueMaterialize"]["query"]["materializeOffset"],
            50
        );
        let first_path = first.data["location"]["localPath"]
            .as_str()
            .expect("localPath")
            .to_owned();
        assert!(Path::new(&first_path).is_absolute());
        assert!(Path::new(&first_path).join("f000.rs").is_file());
        assert!(Path::new(&first_path).join("f049.rs").is_file());
        assert!(!Path::new(&first_path).join("f050.rs").exists());

        let second_query = serde_json::from_value::<super::super::GhSearchQuery>(
            first.data["next"]["continueMaterialize"]["query"].clone(),
        )
        .expect("continue query");
        let second = execute(&provider, &second_query, &context, &env)
            .await
            .expect("second materialize");
        assert_eq!(second.data["pagination"]["page"], 1);
        assert_eq!(second.data["pagination"]["written"], 50);
        assert_eq!(
            second.data["next"]["continueMaterialize"]["query"]["page"],
            2
        );
        assert_eq!(
            second.data["next"]["continueMaterialize"]["query"]["materializeOffset"],
            0
        );
        let second_path = second.data["location"]["localPath"]
            .as_str()
            .expect("second path")
            .to_owned();
        assert_ne!(first_path, second_path);
        for index in 0..100 {
            let path = Path::new(&second_path).join(format!("f{index:03}.rs"));
            let body = fs::read_to_string(&path).unwrap_or_else(|_| "missing".into());
            assert_eq!(body, format!("blob-{index:03}\n"), "{}", path.display());
        }
        assert!(!Path::new(&second_path).join("f100.rs").exists());
    }
}
