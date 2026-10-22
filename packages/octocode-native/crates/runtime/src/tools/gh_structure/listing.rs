//! One listing page: the pinned ref, the page window, the entries and
//! their continuations.
use super::*;
use crate::response::result::Confidence;
use crate::tools::root_read::read_rank;

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
        let current = query.page.map_or(1, crate::num::usize_of);
        let per_page = crate::num::usize_of(
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

    /// The requested page lies past the last page of the listing.
    pub(super) fn past_end(&self) -> bool {
        self.current > self.total_pages
    }

    /// The `pagination` block: shown when the listing has more than one page,
    /// and on a page past the end of any listing (the response layer words
    /// the page count and leads back to page 1).
    pub(super) fn paginate(&self, value: &mut Value) {
        if self.total_pages > 1 || self.past_end() {
            value["pagination"] = crate::response::pages::PageFacts::counted(
                self.current,
                self.per_page,
                self.total_entries,
            )
            .out_of_range(self.past_end())
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
    let mut summary = json!({"files": files, "dirs": entries.len() - files});
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
/// (`response::pages::coverage_codes`), never restated here.
pub(super) fn disclose_limits(
    traversal: &Traversal,
    page: &ListingPage<'_>,
    value: &mut Value,
    output: &mut ToolData,
) {
    use crate::response::paging::{mark_partial, terminal_limit};
    if traversal.contents_limit {
        terminal_limit(value, "providerContentsLimit", &[]);
        value["providerLimit"] = json!({
            "reason": "providerContentsLimit",
            "maxEntriesPerDirectory": CONTENTS_LIMIT
        });
    }
    if traversal.fetch_limit {
        terminal_limit(value, "treeFetchLimit", &[]);
        value["providerLimit"] = json!({
            "reason": "treeFetchLimit",
            "maxDirectoryFetches": MAX_DIRECTORY_FETCHES
        });
    }
    if !traversal.omitted.is_empty() && page.current == 1 {
        // Symlinks and submodules have no listable content: name each with
        // its count instead of silently dropping them, once per listing.
        value["omitted"] = json!(traversal.omitted);
    }
    if traversal.failed_subtrees > 0 {
        mark_partial(value, "partialTreeFailures", &[]);
        output.diagnostics.add(
            "partialTreeFailures",
            "One or more repository subdirectories could not be read; retry the same page or narrow the path/depth.",
            true,
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
    .confidence(Confidence::Exact)
    .build()
}

/// The read a listing leads to on its first page: the best-ranked listed
/// file ([`read_rank`](crate::tools::root_read::read_rank), the ranking
/// structureSearch shares): a source file a bare-word `include` names, else
/// the shallowest code entry file, a manifest, a README, a guide, then other
/// source. A small listing falls back to its first file. Source and docs are outlined
/// (`minify:"symbols"`).
pub(super) fn entry_read(
    query: &GhStructureQueryTree,
    entries: &[TreeEntry],
    commit_sha: &str,
) -> Option<Value> {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .collect::<Vec<_>>();
    let words = bare_words(query.include.iter().map(|word| word.as_str()));
    let base_depth = query.path.as_deref().map_or(0, |path| {
        path.split('/').filter(|part| !part.is_empty()).count()
    });
    let small = files.len() <= crate::tools::OUTLINE_LEAD_MAX_FILES;
    let path = files
        .iter()
        .filter_map(|entry| {
            let depth = entry.path.matches('/').count().saturating_sub(base_depth);
            let rank = read_rank(&entry.path, depth, &words).or(small.then_some(RANK_ANY))?;
            Some(((rank, depth, entry.path.as_str()), entry))
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
        crate::response::result::Continuation::new(ToolId::GhGetFileContent, read)
            .confidence(Confidence::High)
            .build(),
    )
}

/// Rank of a file no tier names; only a small listing reads it.
const RANK_ANY: u8 = 6;

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

/// Listing rows grouped by directory, built in one pass. `dir` is
/// repo-relative (the same base as `path` and `include`), `"."` only for
/// the repository root, which leads. Each listed entry carries its facts in
/// the structureSearch entry form, `"<name> (<fields>)"` (the last `" ("`
/// opens them): a file its size in bytes, then the day of its last commit
/// (`dates`, keyed by repo-relative path); a folder that day. Names sort
/// within each row.
pub(super) fn build_structure(
    entries: &[TreeEntry],
    dates: &std::collections::HashMap<String, String>,
) -> Vec<Value> {
    #[derive(Default)]
    struct Row<'a> {
        files: Vec<(&'a str, String)>,
        folders: Vec<(&'a str, String)>,
    }
    let mut dirs = std::collections::BTreeMap::<&str, Row<'_>>::new();
    for entry in entries {
        let path = entry.path.as_str();
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
        let file = entry.kind == EntryKind::File;
        let fields = file
            .then_some(entry.size)
            .flatten()
            .map(|size| size.to_string())
            .into_iter()
            .chain(dates.get(path).cloned())
            .collect::<Vec<_>>();
        let text = if fields.is_empty() {
            name.to_owned()
        } else {
            format!("{name} ({})", fields.join(", "))
        };
        let row = dirs.entry(parent).or_default();
        if file {
            row.files.push((name, text));
        } else {
            row.folders.push((name, text));
        }
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
        .map(|(dir, row)| {
            let mut value = json!({"dir": dir});
            if !row.files.is_empty() {
                value["files"] = listed(row.files);
            }
            if !row.folders.is_empty() {
                value["folders"] = listed(row.folders);
            }
            value
        })
        .collect()
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
        })
    });
    let cursor = |value: &mut Value, field: &str, next: usize| {
        crate::response::paging::next_cursor(value, ToolId::GhStructure, None, field, next)
    };
    if let Some(resume) = materialize_resume {
        let page_cursor = cursor(value, "page", resume.page);
        let offset_cursor = cursor(value, "materializeOffset", resume.offset);
        if let (Some(next_page), Some(offset)) = (page_cursor, offset_cursor) {
            let mut next_query = query_row(query, &[]);
            next_query["page"] = json!(next_page);
            next_query["pageSize"] = json!(page.per_page);
            next_query["materialize"] = json!(true);
            next_query["materializeOffset"] = json!(offset);
            value["next"]["continueMaterialize"] = continuation(next_query);
        }
        mark_materialize_partial(value);
    } else if has_more
        && !materialize
        && let Some(next_page) = cursor(value, "page", current + 1)
    {
        let mut next_query = query_row(query, &[]);
        next_query["page"] = json!(next_page);
        next_query["pageSize"] = json!(page.per_page);
        value["next"]["nextPage"] = continuation(next_query);
    }
    if retry_tree {
        let mut retry = query_row(query, &[]);
        retry["page"] = json!(current);
        retry["pageSize"] = json!(page.per_page);
        value["next"]["retry"] = continuation(retry);
    }
    Ok(())
}

/// An exact `ghStructure` call of `query` (a page, retry or ref listing).
pub(super) fn continuation(query: Value) -> Value {
    crate::response::result::Continuation::new(ToolId::GhStructure, query)
        .confidence(Confidence::Exact)
        .build()
}

pub(super) fn structure_is_empty(value: &Value) -> bool {
    value
        .get("files")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}

#[cfg(test)]
mod read_lead_tests {
    use super::*;

    fn lead(paths: &[&str], include: &[&str]) -> Option<String> {
        let query: GhStructureQueryTree = serde_json::from_value(json!({
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

    /// The read lead lands on source: the file a bare-word `include`
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
        // A root with loose scripts reads its manifest, not a script (zod's
        // `mm.mjs`); a nested README does not outrank source.
        assert_eq!(
            lead(&["README.md", "AGENTS.md", "mm.mjs", "package.json"], &[]).as_deref(),
            Some("package.json")
        );
        assert_eq!(
            lead(&["README.md", "download-latest.sh"], &[]).as_deref(),
            Some("README.md")
        );
        assert_eq!(
            lead(&["examples/a/README.md", "src/util.ts"], &[]).as_deref(),
            Some("src/util.ts")
        );
        // A large listing never leads to a file no tier names.
        let data = (0..8).map(|n| format!("data/{n}.csv")).collect::<Vec<_>>();
        let data = data.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(lead(&data, &[]), None);
        assert_eq!(lead(&data[..2], &[]).as_deref(), Some("data/0.csv"));
    }
}
