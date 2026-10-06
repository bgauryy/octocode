//! ghStructure: a known repository's tree at one commit, optionally written
//! to a local directory (`materialize`).
use crate::tools::gh_shared::{
    GhFailure, PathRecovery, RepoPath, locate_path, missing_path, parent_dir, search_failure,
    tree_recovery,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use crate::{
    providers::github::{
        ContentsEntry, CredentialResolver, GitHubProvider, ProviderError, ProviderErrorKind,
        ProviderErrorReason, RequestContext, TreeRequest,
    },
    tools::result::ToolData,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;

pub use crate::contracts::tool_types::GhStructureQuery;

mod listing;
mod materialize;
mod refs;
mod traverse;
use listing::*;
use materialize::*;
use traverse::*;

/// Run one ghStructure row. A listing of a missing path names the path and
/// lists the nearest existing directory (case-corrected) with
/// `hints.viewTree`; without a located directory it lists the parent.
pub async fn run<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    request: Result<&RequestContext, ProviderError>,
    home: &Path,
) -> Result<ToolData, GhFailure> {
    let context = request.map_err(|error| search_failure(error, WINDOW_HINT))?;
    let error = match execute(provider, query, context, home).await {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    let at = RepoPath {
        owner: query.owner.as_str(),
        repo: query.repo.as_str(),
        path: query.path.as_deref().unwrap_or_default().trim_matches('/'),
        reference: query.ref_.as_deref(),
    };
    if !missing_path(&error, Some(at.path)) {
        return Err(search_failure(error, WINDOW_HINT));
    }
    let found = locate_path(provider, &at, context).await;
    Err(path_failure(error, &at, query.max_depth, found))
}

const WINDOW_HINT: &str =
    "Lower page, or narrow with path, extensions, or filename to reach deeper results.";

fn path_failure(
    error: ProviderError,
    at: &RepoPath<'_>,
    max_depth: Option<std::num::NonZeroU64>,
    found: Option<PathRecovery>,
) -> GhFailure {
    let mut failure = search_failure(error, WINDOW_HINT);
    let requested = at.path;
    let (directory, confidence, hint) = match &found {
        Some(found) => {
            failure.message = format!("Path not found in {}/{}: {requested}", at.owner, at.repo);
            let directory = if found.directory.is_empty() {
                ".".to_owned()
            } else {
                found.directory.clone()
            };
            let hint = if found.file.is_some() {
                "The path names a file (its case differs); read it with ghGetFileContent, or list its directory with hints.viewTree."
            } else if found.directory.eq_ignore_ascii_case(requested) {
                "Only the path's case differs; run the viewTree continuation."
            } else {
                "The rest of the path does not exist; hints.viewTree lists the nearest existing directory."
            };
            (directory, "exact", hint)
        }
        None => (
            parent_dir(requested),
            "low",
            "Check the path's exact case (no leading slash) and the ref; list the parent directory with hints.viewTree.",
        ),
    };
    let depth = max_depth.map(|depth| i64::try_from(depth.get()).unwrap_or(i64::MAX));
    let listing = RepoPath {
        path: &directory,
        ..*at
    };
    failure.next = Some(json!({ "viewTree": tree_recovery(&listing, depth, confidence) }));
    failure.hint(hint)
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
pub(crate) const MAX_DIRECTORY_FETCHES: usize = 200;

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

pub(crate) async fn execute<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
    home: &Path,
) -> Result<ToolData, ProviderError> {
    if let Some(output) = refs::execute_operation(provider, query, context).await {
        return output;
    }
    let scope = Scope::of(query)?;
    let (resolved_branch, commit_sha) = resolve_listing_ref(provider, query, context).await?;
    let listing = Listing {
        owner: &query.owner,
        repo: &query.repo,
        branch: &commit_sha,
        root: &scope.path,
        max_depth: scope.depth,
    };
    let mut traversal = traverse(provider, &listing, context).await?;
    if let Some(filter) = &scope.filter {
        traversal
            .entries
            .retain(|entry| filter.iter().any(|glob| glob.matches(&entry.path)));
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
    let page = ListingPage::of(query, &traversal.entries);
    let mut value = json!({
        "entries": build_structure(page.entries, &scope.path),
        "summary": summary(page.entries),
        "resolvedRef": resolved_branch,
    });
    if !query.include.is_empty() {
        value["summary"]["include"] = json!(query.include);
    }
    // A caller-supplied full SHA is not restated.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["commitSha"] = json!(commit_sha);
    }
    page.paginate(&mut value);
    let mut output = ToolData::from(Value::Null);
    let materialize_resume = if query.materialize == Some(true) {
        materialize_page(
            provider,
            query,
            &scope,
            &commit_sha,
            &page,
            home,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &mut value, &mut output);
    // Continuations read the same commit.
    let pinned = GhStructureQuery {
        ref_: Some(commit_sha.clone()),
        ..query.clone()
    };
    attach_continuations(
        &mut value,
        &pinned,
        &page,
        traversal.failed_subtrees > 0,
        query.materialize == Some(true),
        materialize_resume,
    )?;
    if structure_is_empty(&value) && value.get("isPartial").is_none() {
        output.status = Some("empty");
        if scope.filter.is_some() {
            value["hints"] = json!([
                "No path matched include; try a bare word, or \"**/name\" for an exact file name."
            ]);
        }
    } else if let Some(local) = value.pointer("/location/localPath").cloned() {
        // A materialized listing continues with the local tools on disk.
        value["next"]["exploreClone"] = crate::tools::result::Continuation::new(
            ToolId::StructureSearch,
            json!({ "path": local }),
        )
        .confidence("exact")
        .build();
    } else if page.current == 1
        && let Some(read) = entry_read(&pinned, &traversal.entries, &scope.path, &commit_sha)
    {
        value["next"]["read"] = read;
    }
    output.data = value;
    Ok(output)
}

/// What one listing covers: the requested directory, the depth, and the
/// `include` globs (ORed: a path matching any is kept).
struct Scope {
    path: String,
    depth: usize,
    filter: Option<Vec<PathFilter>>,
}

impl Scope {
    fn of(query: &GhStructureQuery) -> Result<Self, ProviderError> {
        let filters = query
            .include
            .iter()
            .map(|glob| PathFilter::new(glob))
            .collect::<Result<Vec<_>, _>>()?;
        let filter = (!filters.is_empty()).then_some(filters);
        let requested = query.path.as_deref().unwrap_or("").trim_matches('/');
        let path = if requested == "." {
            String::new()
        } else {
            requested.to_owned()
        };
        // A name search looks at every level unless the caller bounds it.
        let depth = match (query.max_depth, &filter) {
            (Some(depth), _) => crate::tools::num::usize_of(depth),
            (None, Some(_)) => max_listing_depth(),
            (None, None) => 1,
        };
        Ok(Self {
            path,
            depth,
            filter,
        })
    }
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Verify owner/repo/ref, or broaden path/depth."
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    /// The resolved branch is next-call input when the caller named none;
    /// it goes only when it repeats the requested ref.
    fn compact(
        &self,
        data: &mut serde_json::Map<String, serde_json::Value>,
        query: &serde_json::Value,
    ) {
        if data.get("resolvedRef").is_some() && data.get("resolvedRef") == query.get("ref") {
            data.remove("resolvedRef");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_does_not_restate_the_requested_ref() {
        use crate::tools::output::ToolOutput;
        let sha = "63c5760d8a672cee96e1e523d84bfa1c77d9ee4c";
        let listing =
            serde_json::json!({"entries":[{"dir":"src","files":["a.rs"]}],"resolvedRef":sha});
        let mut pinned = listing.as_object().cloned().unwrap_or_default();
        Output.compact(&mut pinned, &serde_json::json!({"ref": sha}));
        assert!(pinned.get("resolvedRef").is_none(), "{pinned:?}");
        // A resolved default branch is news to the caller.
        let mut default_branch = listing.as_object().cloned().unwrap_or_default();
        Output.compact(&mut default_branch, &serde_json::json!({}));
        assert_eq!(default_branch["resolvedRef"], sha);
    }
    #[tokio::test]
    async fn materialize_never_writes_a_provider_path_outside_its_root() {
        let base = tempfile::tempdir().expect("tempdir");
        let root = base.path().join("snapshot");
        std::fs::create_dir_all(&root).expect("root");
        let outside = base.path().join("escape.txt");
        let mut writer = MaterializeWriter {
            root: &root,
            written: 0,
            total_bytes: 0,
            cursor: 0,
            reason: "listing",
            skips: MaterializeSkips::default(),
            recorded: Vec::new(),
        };
        let read = || {
            Acquired::Read(Ok(crate::providers::github::ContentResponse {
                bytes: b"owned".to_vec(),
                resolved_ref: "main".into(),
                etag: None,
                from_cache: false,
                raw_response_bytes: 5,
            }))
        };
        for path in [
            "../escape.txt".to_owned(),
            outside.to_string_lossy().into_owned(),
            "a/../../escape.txt".to_owned(),
            "ok/kept.txt".to_owned(),
        ] {
            let entry = TreeEntry {
                path,
                kind: EntryKind::File,
                size: Some(5),
            };
            assert!(writer.write(&entry, read()).await.expect("write"));
        }
        assert!(!outside.exists());
        assert_eq!(writer.written, 1);
        assert_eq!(
            std::fs::read(root.join("ok/kept.txt")).expect("kept"),
            b"owned"
        );
        assert_eq!(
            writer.skips.unreadable.len(),
            3,
            "{:?}",
            writer.skips.unreadable
        );
    }

    #[test]
    fn materialize_skips_name_each_path_once_and_warn_with_counts() {
        let skips = MaterializeSkips {
            oversized: (0..30).map(|n| format!("big/{n}.bin")).collect(),
            unreadable: vec![
                ("a.dat".into(), "binary".into()),
                ("b.dat".into(), "binary".into()),
                ("c".into(), "unsupported entry".into()),
            ],
        };
        let warnings = skips.warnings();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].starts_with("Skipped 30 file(s) larger than"),
            "{warnings:?}"
        );
        assert!(!warnings[0].contains("big/0.bin"), "{warnings:?}");
        assert_eq!(
            warnings[1],
            "Skipped 3 unreadable file(s) (location.skipped): binary; unsupported entry."
        );
        let paths = skips.paths();
        assert_eq!(paths.len(), 33);
        assert_eq!(paths[0], "big/0.bin");
        assert_eq!(paths[32], "c");
        assert!(MaterializeSkips::default().warnings().is_empty());
    }

    #[test]
    fn materialization_page_boundary_continues_and_page_ceiling_is_explicit() {
        let query: GhStructureQuery = serde_json::from_value(json!({
            "owner": "a", "repo": "b", "ref": "a".repeat(40),
            "path": "", "mainGoal": "Read every materialized file", "reasoning": "Preserve the remaining listing"
        })).expect("query");
        let mut value = json!({
            "entries": [{"dir": ".", "files": ["a.rs"]}],
            "location": {"localPath": "/t"}
        });
        let page = |current| ListingPage {
            current,
            per_page: 1,
            total_entries: current + 1,
            total_pages: current + 1,
            entries: &[],
        };
        attach_continuations(&mut value, &query, &page(1), false, true, None)
            .expect("continuation");
        assert_eq!(value["location"], json!({"localPath": "/t"}));
        assert_eq!(value["isPartial"], true);
        assert_eq!(value["partialReasons"], json!(["materializeIncomplete"]));
        assert_eq!(
            value["next"]["continueMaterialize"]["query"]["queries"][0]["page"],
            2
        );
        assert_eq!(
            value["next"]["continueMaterialize"]["query"]["queries"][0]["materializeOffset"],
            0
        );
        assert_eq!(
            value["next"]["continueMaterialize"]["query"]["queries"][0]["ref"],
            "a".repeat(40)
        );
        for materialize in [false, true] {
            let mut value = json!({"entries": [{"dir": ".", "files": ["last.rs"]}]});
            attach_continuations(&mut value, &query, &page(1000), false, materialize, None)
                .expect("terminal");
            assert_eq!(value["terminalLimit"], true);
            assert!(value.get("next").is_none());
            assert_eq!(value["entries"][0]["files"][0], "last.rs");
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
