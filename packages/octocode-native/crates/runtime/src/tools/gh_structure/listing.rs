//! One listing page: the pinned ref, the page window, the entries and
//! their continuations.
use super::*;

/// Pin the listing to one commit: an explicit ref that does not resolve is
/// an error (like ghGetFileContent), never a silent default-branch listing,
/// and every page of one listing reads the same tree. The recursive tree is
/// fetched by the resolved commit SHA, never by the ref name.
pub(super) async fn resolve_listing_ref<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
) -> Result<(String, String), ProviderError> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    match query.ref_.as_deref() {
        Some(branch) => {
            let sha = provider
                .resolve_reference(owner, repo, Some(branch), false, context)
                .await
                .map_err(|error| missing_ref(error, owner, repo, branch))?;
            Ok((branch.to_owned(), sha))
        }
        None => Ok(tokio::try_join!(
            default_branch(provider, owner, repo, context),
            provider.resolve_reference(owner, repo, None, false, context),
        )?),
    }
}

/// The default branch's name, memoized per repository for the cache's
/// volatile lifetime (like the HEAD commit memo beside it).
pub(super) async fn default_branch<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<String, ProviderError> {
    let key = repo_key("repo-default-branch", owner, repo);
    let partition = provider.transport.cache_partition(context, None).await.ok();
    if let Some(partition) = &partition
        && let Some(cached) = provider.cache.get(partition, &key).await
        && let Ok(branch) = String::from_utf8(cached.bytes)
    {
        return Ok(branch);
    }
    let branch = provider
        .transport
        .repository_metadata(owner, repo, context)
        .await?
        .default_branch;
    if let Some(partition) = &partition {
        provider
            .cache
            .put(
                partition,
                key,
                crate::providers::github::CachedContent {
                    etag: None,
                    bytes: branch.as_bytes().to_vec(),
                    resolved_ref: branch.clone(),
                },
            )
            .await;
    }
    Ok(branch)
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
    pub(super) fn of(query: &GhStructureQuery, all: &'a [TreeEntry]) -> Self {
        let current = crate::tools::num::usize_of(query.page);
        let per_page =
            crate::tools::num::usize_of(query.page_size).clamp(1, max_entries_per_page());
        let total_entries = all.len();
        let total_pages = total_entries.div_ceil(per_page).max(1);
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

    pub(super) fn paginate(&self, value: &mut Value) {
        if self.total_pages > 1 {
            value["pagination"] = json!({
                "currentPage": self.current,
                "totalPages": self.total_pages,
                "hasMore": self.has_more(),
                "pageSize": self.per_page,
                "totalItems": self.total_entries,
            });
        }
    }
}

/// File and folder counts of one page.
pub(super) fn summary(entries: &[TreeEntry]) -> Value {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .count();
    json!({"totalFiles": files, "totalFolders": entries.len() - files})
}

/// Provider limits and skipped entries of the walk, stated on the page.
pub(super) fn disclose_limits(traversal: &Traversal, value: &mut Value, output: &mut ToolData) {
    let mut partial_reasons = Vec::<&str>::new();
    if traversal.contents_limit {
        partial_reasons.push("providerContentsLimit");
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
        partial_reasons.push("treeFetchLimit");
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
        partial_reasons.push("partialTreeFailures");
        output.diagnostics.add(
            "partialTreeFailures",
            "One or more repository subdirectories could not be read; retry the same page or narrow the path/depth.",
            true,
        );
    }
    if !partial_reasons.is_empty() {
        value["isPartial"] = json!(true);
        value["partialReasons"] = json!(partial_reasons);
    }
}

/// The read a listing leads to on its first page: a small listing's first
/// source or doc file, else the listed directory's entry file (package
/// init, index, module root, main, then README or a manifest). Source and
/// docs are outlined (`minify:"symbols"`).
pub(super) fn entry_read(
    query: &GhStructureQuery,
    entries: &[TreeEntry],
    root: &str,
    commit_sha: &str,
) -> Option<Value> {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .collect::<Vec<_>>();
    let path = if files.len() <= crate::tools::OUTLINE_LEAD_MAX_FILES {
        files
            .iter()
            .find(|entry| crate::tools::outlines(&entry.path))
            .or(files.first())?
            .path
            .clone()
    } else {
        let direct = files
            .iter()
            .filter(|entry| parent_of(&entry.path) == root)
            .collect::<Vec<_>>();
        ENTRY_FILES.iter().find_map(|wanted| {
            direct
                .iter()
                .find(|entry| entry_matches(file_name(&entry.path), wanted))
                .map(|entry| entry.path.clone())
        })?
    };
    let mut read = json!({
        "owner": query.owner.as_str(),
        "repo": query.repo.as_str(),
        "path": path,
        "ref": commit_sha,
    });
    if crate::tools::outlines(&path) {
        read["minify"] = json!("symbols");
    }
    Some(
        crate::tools::result::Continuation::new(ToolId::GhGetFileContent, read)
            .confidence("high")
            .build(),
    )
}

/// Entry files in the order a reader opens them; `*` matches any extension.
pub(super) const ENTRY_FILES: &[&str] = &[
    "__init__.py",
    "index.*",
    "mod.rs",
    "lib.rs",
    "main.*",
    "readme*",
    "package.json",
    "cargo.toml",
    "pyproject.toml",
    "go.mod",
];

pub(super) fn entry_matches(name: &str, wanted: &str) -> bool {
    let name = name.to_ascii_lowercase();
    match wanted.strip_suffix('*') {
        Some(stem) => name.starts_with(stem),
        None => name == wanted,
    }
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(super) fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(super) fn build_structure(entries: &[TreeEntry], root: &str) -> Vec<Value> {
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
    query: &GhStructureQuery,
    page: &ListingPage<'_>,
    retry_tree: bool,
    materialize: bool,
    materialize_resume: Option<MaterializeResume>,
) -> Result<(), ProviderError> {
    let (current, has_more) = (page.current, page.has_more());
    let materialize_resume = materialize_resume.or_else(|| {
        (has_more && materialize).then_some(MaterializeResume {
            page: current + 1,
            offset: 0,
            reason: "listing",
        })
    });
    let last_page = crate::tools::id::query_limits::gh_structure::PAGE_MAXIMUM;
    if materialize_resume.is_some_and(|resume| resume.page > last_page)
        || (has_more && !materialize && current >= last_page)
    {
        value["terminalLimit"] = json!(true);
        mark_materialize_partial(value);
    } else if let Some(resume) = materialize_resume {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(resume.page);
        next_query["pageSize"] = json!(page.per_page);
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
        mark_materialize_partial(value);
    } else if has_more && !materialize {
        let mut next_query = public_query(query)?;
        next_query["page"] = json!(current + 1);
        next_query["pageSize"] = json!(page.per_page);
        value["next"]["nextPage"] = continuation(
            next_query,
            format!("Continue tree results on page {}.", current + 1),
            "exact",
        );
    }
    if retry_tree {
        let mut retry = public_query(query)?;
        retry["page"] = json!(current);
        retry["pageSize"] = json!(page.per_page);
        value["next"]["retry"] = continuation(
            retry,
            "Retry the same tree provider page because the provider reported incomplete results.",
            "exact",
        );
    }
    Ok(())
}

pub(super) fn public_query(query: &GhStructureQuery) -> Result<Value, ProviderError> {
    let mut query = serde_json::to_value(query)
        .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
    remove_nulls(&mut query);
    Ok(query)
}

pub(super) fn continuation(
    query: Value,
    why: impl Into<String>,
    confidence: &'static str,
) -> Value {
    crate::tools::result::Continuation::new(ToolId::GhStructure, query)
        .why(why)
        .confidence(confidence)
        .build()
}

pub(super) fn structure_is_empty(value: &Value) -> bool {
    value
        .get("entries")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}
