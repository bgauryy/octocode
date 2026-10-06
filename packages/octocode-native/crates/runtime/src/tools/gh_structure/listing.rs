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
    let metadata = provider
        .transport
        .repository_metadata(owner, repo, context)
        .await?;
    // The same read names the repository's canonical name: a renamed
    // repository costs no further request.
    if let Some(full_name) = metadata.full_name.as_deref() {
        crate::tools::gh_shared::remember_canonical(provider, owner, repo, full_name, context)
            .await;
    }
    let branch = metadata.default_branch;
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
        let per_page = crate::tools::num::usize_of(
            query
                .page_size
                .map_or(DEFAULT_ENTRIES_PER_PAGE, std::num::NonZeroU64::get),
        )
        .clamp(1, max_entries_per_page());
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

/// The read a listing leads to on its first page: the best-ranked listed
/// file ([`read_rank`]): a source file a bare-word `include` names, else the
/// shallowest code entry file, other source, a manifest, then a README. A
/// small listing falls back to its first file. Source and docs are outlined
/// (`minify:"symbols"`).
pub(super) fn entry_read(
    query: &GhStructureQuery,
    entries: &[TreeEntry],
    commit_sha: &str,
) -> Option<Value> {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .collect::<Vec<_>>();
    let words = bare_words(query.include.iter().map(|word| word.as_str()));
    let small = files.len() <= crate::tools::OUTLINE_LEAD_MAX_FILES;
    let path = files
        .iter()
        .filter_map(|entry| {
            let rank = read_rank(&entry.path, &words).or(small.then_some(RANK_ANY))?;
            Some((
                (rank, entry.path.matches('/').count(), entry.path.as_str()),
                entry,
            ))
        })
        .min_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, entry)| entry.path.clone())?;
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

/// Rank of a file no tier names; only a small listing reads it.
const RANK_ANY: u8 = 5;

/// `include` entries that are bare words (no glob metacharacter or `/`),
/// lowercased: a file whose stem equals one is the file the caller named.
fn bare_words<'a>(include: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    include
        .into_iter()
        .map(str::trim)
        .filter(|word| !word.is_empty() && !word.contains(['/', '*', '?', '[', '{']))
        .map(str::to_lowercase)
        .collect()
}

/// A listed file's read rank (lower reads first): 0 a non-test source file
/// whose stem a bare-word `include` names; 1 a code entry file (package
/// init, index, mod/lib root, main); 2 other non-test source; 3 a manifest;
/// 4 a README. `None` for anything else (tests, data, config).
fn read_rank(path: &str, words: &[String]) -> Option<u8> {
    let name = file_name(path);
    let lower = name.to_ascii_lowercase();
    let code = matches!(
        crate::content::classify_file_type(path),
        Some(crate::content::FileType::Code)
    ) && !crate::content::is_test_path(path);
    let stem = lower
        .rsplit_once('.')
        .map_or(lower.as_str(), |(stem, _)| stem);
    if code && words.iter().any(|word| word == stem) {
        return Some(0);
    }
    if code
        && CODE_ENTRY_FILES
            .iter()
            .any(|wanted| entry_matches(name, wanted))
    {
        return Some(1);
    }
    if code {
        return Some(2);
    }
    if MANIFEST_FILES.contains(&lower.as_str()) {
        return Some(3);
    }
    if lower.starts_with("readme") {
        return Some(4);
    }
    None
}

/// Code entry files: package init, index, module root, main.
const CODE_ENTRY_FILES: &[&str] = &["__init__.py", "index.*", "mod.rs", "lib.rs", "main.*"];

/// Package manifests a read can lead to when a listing has no source.
const MANIFEST_FILES: &[&str] = &["package.json", "cargo.toml", "pyproject.toml", "go.mod"];

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

/// Listing rows grouped by directory. `dir` is repo-relative (the same base
/// as `path` and `include`), `"."` only for the repository root.
pub(super) fn build_structure(entries: &[TreeEntry]) -> Vec<Value> {
    let mut dirs = Map::<String, Value>::new();
    for entry in entries {
        let path = entry.path.as_str();
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
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

#[cfg(test)]
mod read_lead_tests {
    use super::*;

    fn lead(paths: &[&str], include: &[&str]) -> Option<String> {
        let query: GhStructureQuery = serde_json::from_value(json!({
            "owner": "o", "repo": "r", "include": include
        }))
        .expect("query");
        let entries = paths
            .iter()
            .map(|path| TreeEntry {
                path: (*path).to_owned(),
                kind: EntryKind::File,
                size: Some(1),
            })
            .collect::<Vec<_>>();
        entry_read(&query, &entries, "s").map(|read| {
            read["query"]["queries"][0]["path"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
    }

    /// GS4: the read lead lands on source: the file a bare-word `include`
    /// names over its test, and a code entry over README and other files.
    #[test]
    fn the_read_lead_prefers_source_over_tests_and_docs() {
        assert_eq!(
            lead(
                &["__tests__/X-test.js", "client/X.js", "client/XFB.js"],
                &["X"]
            )
            .as_deref(),
            Some("client/X.js")
        );
        assert_eq!(
            lead(&["README.md", "benches/b.rs", "src/lib.rs"], &[]).as_deref(),
            Some("src/lib.rs")
        );
        // Without source, a manifest before a README.
        assert_eq!(
            lead(&["README.md", "Cargo.toml"], &[]).as_deref(),
            Some("Cargo.toml")
        );
        // A large listing never leads to a file no tier names.
        let data = (0..8).map(|n| format!("data/{n}.csv")).collect::<Vec<_>>();
        let data = data.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(lead(&data, &[]), None);
        assert_eq!(lead(&data[..2], &[]).as_deref(), Some("data/0.csv"));
    }
}
