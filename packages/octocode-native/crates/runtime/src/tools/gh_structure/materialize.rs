//! `materialize`: write a listing page's files under the Octocode home.
use super::*;
use std::path::PathBuf;

/// Write this page's files under `location.localPath` (the listed
/// directory) and return where a continuation resumes, if anywhere.
#[allow(clippy::too_many_arguments)]
pub(super) async fn materialize_page<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    scope: &Scope,
    commit_sha: &str,
    page: &ListingPage<'_>,
    home: &Path,
    context: &RequestContext,
    value: &mut Value,
) -> Result<Option<MaterializeResume>, ProviderError> {
    let offset = query
        .materialize_offset
        .map_or(0, |offset| usize::try_from(offset).unwrap_or(0));
    let snapshot = Snapshot {
        owner: &query.owner,
        repo: &query.repo,
        commit: commit_sha,
        root: home,
    };
    let Some(outcome) = materialize_tree(
        provider,
        &snapshot,
        page.entries,
        offset,
        &scope.path,
        context,
    )
    .await?
    else {
        return Ok(None);
    };
    let resume = match outcome.next_offset {
        Some(offset) => Some(MaterializeResume {
            page: page.current,
            offset,
            reason: outcome.reason,
        }),
        None => page.has_more().then_some(MaterializeResume {
            page: page.current + 1,
            offset: 0,
            reason: "listing",
        }),
    };
    let mut warnings = outcome.warnings;
    // Folders at the listing's depth are listed, not written: say so, so a
    // local search below them is not mistaken for absence.
    let unwritten = unwritten_folders(page.entries, scope);
    if let Some(first) = unwritten.first() {
        warnings.push(format!(
            "{} listed folder(s) at maxDepth {} were not written; run next.expandDepth.",
            unwritten.len(),
            scope.depth
        ));
        value["next"]["expandDepth"] = expand_depth(query, scope, commit_sha, first)?;
    }
    let mut location = outcome.location;
    // The checkout is complete only when this listing wrote everything it
    // names: no later page or offset, no folder below the depth, no skip.
    location["complete"] = json!(resume.is_none() && unwritten.is_empty() && !outcome.skipped);
    value["location"] = location;
    if !unwritten.is_empty() {
        value["isPartial"] = json!(true);
        push_partial_reason(value, "materializeDepth");
    }
    if !warnings.is_empty() {
        value["warnings"] = json!(warnings);
    }
    Ok(resume)
}

/// Listed folders at the listing's depth: their contents were not listed,
/// so a materialize did not write them.
pub(super) fn unwritten_folders<'a>(entries: &'a [TreeEntry], scope: &Scope) -> Vec<&'a str> {
    entries
        .iter()
        .filter(|entry| {
            entry.kind == EntryKind::Dir
                && relative(&entry.path, &scope.path).split('/').count() >= scope.depth
        })
        .map(|entry| entry.path.as_str())
        .collect()
}

/// The materialize that writes what a depth-bounded one left out: the same
/// listing at the deepest `maxDepth`, from its first page, pinned to the
/// commit; at that depth already, the first unwritten folder itself.
pub(super) fn expand_depth(
    query: &GhStructureQuery,
    scope: &Scope,
    commit_sha: &str,
    first_unwritten: &str,
) -> Result<Value, ProviderError> {
    let mut next = public_query(&GhStructureQuery {
        ref_: Some(commit_sha.to_owned()),
        ..query.clone()
    })?;
    let deepest = max_listing_depth();
    next["maxDepth"] = json!(deepest);
    next["materialize"] = json!(true);
    if scope.depth >= deepest {
        next["path"] = json!(first_unwritten);
    }
    if let Some(fields) = next.as_object_mut() {
        fields.remove("page");
        fields.remove("materializeOffset");
    }
    Ok(continuation(
        next,
        "Write the folders this depth listed but did not write.",
        "exact",
    ))
}

pub(super) fn push_partial_reason(value: &mut Value, reason: &str) {
    let reasons = value.as_object_mut().and_then(|object| {
        object
            .entry("partialReasons")
            .or_insert_with(|| json!([]))
            .as_array_mut()
    });
    if let Some(reasons) = reasons
        && !reasons.iter().any(|known| known == reason)
    {
        reasons.push(json!(reason));
    }
}

#[derive(Clone, Copy)]
pub(super) struct MaterializeResume {
    pub(super) page: usize,
    pub(super) offset: usize,
    pub(super) reason: &'static str,
}

/// A materialized checkout that misses files the listing names leaves the
/// row partial until `next.continueMaterialize` writes the rest.
pub(super) fn mark_materialize_partial(value: &mut Value) {
    if value.get("location").is_none() {
        return;
    }
    value["isPartial"] = json!(true);
    push_partial_reason(value, "materializeIncomplete");
}

pub(super) const MATERIALIZE_FILE_CAP: usize = 50;

pub(super) const MATERIALIZE_FILE_BYTES: usize = 300 * 1024;

pub(super) const MATERIALIZE_TOTAL_BYTES: usize = 5 * 1024 * 1024;

pub(super) struct MaterializeOutcome {
    pub(super) location: Value,
    pub(super) next_offset: Option<usize>,
    pub(super) reason: &'static str,
    pub(super) warnings: Vec<String>,
    /// Files the page could not write (`location.skipped`).
    pub(super) skipped: bool,
}

/// The commit a materialize writes, and the Octocode home it writes under.
pub(super) struct Snapshot<'a> {
    pub(super) owner: &'a str,
    pub(super) repo: &'a str,
    pub(super) commit: &'a str,
    pub(super) root: &'a Path,
}

/// Files read at once; the global request semaphore still bounds the total.
pub(super) const MATERIALIZE_CONCURRENCY: usize = 8;

/// Where materialized checkouts live under the Octocode home. The layout is
/// versioned and apart from `tmp/tree`, which octocode-mcp 19.x directory
/// fetches rewrite: a shared path let one writer replace the other's files.
pub(super) const MATERIALIZE_LAYOUT: [&str; 3] = ["tmp", "materialize", "v2"];

/// The manifest beside a checkout directory (`<commit>.manifest.json`), so the
/// checkout holds only repository files. It names the files this layout
/// wrote completely, with their sizes.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Manifest {
    format: String,
    owner: String,
    repo: String,
    commit: String,
    files: std::collections::BTreeMap<String, u64>,
}

const MANIFEST_FORMAT: &str = "octocode-materialize/v2";

impl Manifest {
    fn new(snapshot: &Snapshot<'_>) -> Self {
        Self {
            format: MANIFEST_FORMAT.to_owned(),
            owner: snapshot.owner.to_owned(),
            repo: snapshot.repo.to_owned(),
            commit: snapshot.commit.to_owned(),
            files: std::collections::BTreeMap::new(),
        }
    }

    /// The manifest at `path` when it describes this snapshot; a foreign,
    /// unreadable, or other-commit manifest is no manifest.
    async fn load(path: &Path, snapshot: &Snapshot<'_>) -> Option<Self> {
        let bytes = tokio::fs::read(path).await.ok()?;
        let manifest: Self = serde_json::from_slice(&bytes).ok()?;
        (manifest.format == MANIFEST_FORMAT
            && manifest.owner == snapshot.owner
            && manifest.repo == snapshot.repo
            && manifest.commit == snapshot.commit)
            .then_some(manifest)
    }

    /// Replace the manifest atomically (temp file + rename), so a reader never
    /// sees a half-written one.
    async fn save(&self, path: &Path) -> Result<(), ProviderError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
        let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        tokio::fs::write(&temp, bytes)
            .await
            .map_err(|error| write_error("write materialize manifest", &error))?;
        tokio::fs::rename(&temp, path)
            .await
            .map_err(|error| write_error("write materialize manifest", &error))
    }
}

/// A checkout directory this layout owns: `<home>/tmp/materialize/v2/
/// <owner>/<repo>/<commit>/` and the manifest beside it. A directory without
/// this snapshot's manifest (another writer's, or one left before its
/// manifest was written) is never trusted: it is removed and rewritten.
pub(super) async fn checkout_root(
    snapshot: &Snapshot<'_>,
) -> Result<(PathBuf, PathBuf, Manifest), ProviderError> {
    for segment in [snapshot.owner, snapshot.repo, snapshot.commit] {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains(['/', '\\'])
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!("cannot materialize under the path segment \"{segment}\""),
            ));
        }
    }
    let parent = MATERIALIZE_LAYOUT
        .iter()
        .chain([&snapshot.owner, &snapshot.repo])
        .fold(snapshot.root.to_path_buf(), |path, segment| {
            path.join(segment)
        });
    let root = parent.join(snapshot.commit);
    let manifest_path = parent.join(format!("{}.manifest.json", snapshot.commit));
    if let Some(manifest) = Manifest::load(&manifest_path, snapshot).await
        && tokio::fs::metadata(&root)
            .await
            .is_ok_and(|meta| meta.is_dir())
    {
        return Ok((root, manifest_path, manifest));
    }
    match tokio::fs::symlink_metadata(&root).await {
        Ok(meta) if meta.is_dir() => tokio::fs::remove_dir_all(&root).await,
        Ok(_) => tokio::fs::remove_file(&root).await,
        Err(_) => Ok(()),
    }
    .map_err(|error| write_error("clear an untrusted materialize directory", &error))?;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| write_error("create materialize directory", &error))?;
    let manifest = Manifest::new(snapshot);
    manifest.save(&manifest_path).await?;
    Ok((root, manifest_path, manifest))
}

/// Write `entries[offset..]` under the checkout root ([`checkout_root`]),
/// reading up to [`MATERIALIZE_CONCURRENCY`] files at once and writing them
/// in listing order, so the write caps stop at the same entry as a serial
/// walk. A file the manifest records at its listed size and that is still on
/// disk at that size is reused, not fetched again. `location.localPath` is the
/// listed directory.
pub(super) async fn materialize_tree<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    snapshot: &Snapshot<'_>,
    entries: &[TreeEntry],
    offset: usize,
    listed: &str,
    context: &RequestContext,
) -> Result<Option<MaterializeOutcome>, ProviderError> {
    if offset >= entries.len() {
        return Ok(None);
    }
    let (root, manifest_path, mut manifest) = checkout_root(snapshot).await?;
    let mut writer = MaterializeWriter {
        root: &root,
        written: 0,
        total_bytes: 0,
        cursor: offset,
        reason: "listing",
        skips: MaterializeSkips::default(),
        recorded: Vec::new(),
    };
    'chunks: while writer.cursor < entries.len() {
        let chunk =
            &entries[writer.cursor..entries.len().min(writer.cursor + MATERIALIZE_CONCURRENCY)];
        let manifest = &manifest;
        let root = &root;
        let reads = futures_util::future::join_all(chunk.iter().map(|entry| async move {
            if entry.kind != EntryKind::File {
                return Acquired::NotAFile;
            }
            if entry
                .size
                .is_some_and(|size| size > MATERIALIZE_FILE_BYTES as u64)
            {
                return Acquired::Oversized;
            }
            if reusable(manifest, root, entry).await {
                return Acquired::Reused;
            }
            Acquired::Read(
                provider
                    .get_file_content(
                        &crate::providers::github::ContentRequest {
                            owner: snapshot.owner.to_owned(),
                            repo: snapshot.repo.to_owned(),
                            path: entry.path.clone(),
                            reference: Some(snapshot.commit.to_owned()),
                            force_refresh: false,
                            session_id: None,
                        },
                        context,
                    )
                    .await,
            )
        }))
        .await;
        for (entry, read) in chunk.iter().zip(reads) {
            if !writer.write(entry, read).await? {
                break 'chunks;
            }
        }
    }
    if !writer.recorded.is_empty() {
        // Merge with what another writer recorded meanwhile; an entry lost to
        // a race only costs a later re-fetch, never a trusted wrong file.
        if let Some(current) = Manifest::load(&manifest_path, snapshot).await {
            manifest.files.extend(current.files);
        }
        manifest.files.extend(std::mem::take(&mut writer.recorded));
        manifest.save(&manifest_path).await?;
    }
    let has_more = writer.cursor < entries.len();
    let local = if listed.is_empty() {
        root.clone()
    } else {
        root.join(listed)
    };
    // The snapshot is the commit segment of `localPath`; `complete` (set
    // by the page) is the only coverage flag.
    let mut location = json!({
        "localPath": local.to_string_lossy(),
        "complete": !has_more,
    });
    let warnings = writer.skips.warnings();
    let skipped = !writer.skips.is_empty();
    if skipped {
        location["skipped"] = json!(writer.skips.paths());
    }
    Ok(Some(MaterializeOutcome {
        location,
        next_offset: has_more.then_some(writer.cursor),
        reason: writer.reason,
        warnings,
        skipped,
    }))
}

/// A listed file the manifest recorded at the listed size, still on disk at
/// that size.
async fn reusable(manifest: &Manifest, root: &Path, entry: &TreeEntry) -> bool {
    let Some(size) = entry.size else {
        return false;
    };
    manifest.files.get(&entry.path) == Some(&size)
        && crate::tools::gh_shared::is_repo_relative(&entry.path)
        && tokio::fs::symlink_metadata(root.join(&entry.path))
            .await
            .is_ok_and(|meta| meta.is_file() && meta.len() == size)
}

/// What a materialize page has for one listed entry.
pub(super) enum Acquired {
    NotAFile,
    /// Larger than [`MATERIALIZE_FILE_BYTES`] by its listed size.
    Oversized,
    /// Already on disk, as the manifest recorded it.
    Reused,
    Read(Result<crate::providers::github::ContentResponse, ProviderError>),
}

/// The ordered write side of a materialize page and its caps.
pub(super) struct MaterializeWriter<'a> {
    pub(super) root: &'a Path,
    pub(super) written: usize,
    pub(super) total_bytes: usize,
    pub(super) cursor: usize,
    pub(super) reason: &'static str,
    pub(super) skips: MaterializeSkips,
    /// Files this page wrote completely, for the manifest.
    pub(super) recorded: Vec<(String, u64)>,
}

impl MaterializeWriter<'_> {
    /// Write one listed entry (its read, when it is a file); `false` stops
    /// the page at this entry, which the continuation resumes.
    pub(super) async fn write(
        &mut self,
        entry: &TreeEntry,
        read: Acquired,
    ) -> Result<bool, ProviderError> {
        if self.written >= MATERIALIZE_FILE_CAP {
            self.reason = "writeCap";
            return Ok(false);
        }
        if self.total_bytes >= MATERIALIZE_TOTAL_BYTES {
            self.reason = "totalSize";
            return Ok(false);
        }
        self.cursor += 1;
        let acquired = match read {
            Acquired::NotAFile | Acquired::Reused => return Ok(true),
            Acquired::Oversized => {
                self.skips.oversized.push(entry.path.clone());
                return Ok(true);
            }
            Acquired::Read(Ok(acquired)) => acquired,
            // One unreadable file (binary, unsupported entry) must not abort
            // the batch; only auth/rate/timeout/cancel failures propagate.
            Acquired::Read(Err(error))
                if !access_failure(&error)
                    && !matches!(
                        error.kind,
                        ProviderErrorKind::Timeout | ProviderErrorKind::Cancelled
                    ) =>
            {
                self.skips
                    .unreadable
                    .push((entry.path.clone(), error.message.to_string()));
                return Ok(true);
            }
            Acquired::Read(Err(error)) => return Err(error),
        };
        if acquired.bytes.len() > MATERIALIZE_FILE_BYTES {
            self.skips.oversized.push(entry.path.clone());
            return Ok(true);
        }
        if self.total_bytes.saturating_add(acquired.bytes.len()) > MATERIALIZE_TOTAL_BYTES {
            self.reason = "totalSize";
            self.cursor -= 1;
            return Ok(false);
        }
        if !crate::tools::gh_shared::is_repo_relative(&entry.path) {
            self.skips.unreadable.push((
                entry.path.clone(),
                "the provider path leaves the materialize root".to_owned(),
            ));
            return Ok(true);
        }
        let dest = self.root.join(&entry.path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| write_error("create materialize path", &error))?;
        }
        tokio::fs::write(&dest, &acquired.bytes)
            .await
            .map_err(|error| write_error("write materialized file", &error))?;
        self.total_bytes += acquired.bytes.len();
        self.written += 1;
        self.recorded
            .push((entry.path.clone(), acquired.bytes.len() as u64));
        Ok(true)
    }
}

pub(super) fn write_error(action: &str, error: &std::io::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Validation,
        format!("failed to {action}: {error}"),
    )
}

/// Files a materialize page did not write. `location.skipped` names each
/// path once; the warnings carry counts and each distinct read error once.
#[derive(Default)]
pub(super) struct MaterializeSkips {
    pub(super) oversized: Vec<String>,
    pub(super) unreadable: Vec<(String, String)>,
}

impl MaterializeSkips {
    pub(super) fn is_empty(&self) -> bool {
        self.oversized.is_empty() && self.unreadable.is_empty()
    }

    pub(super) fn paths(&self) -> Vec<&str> {
        self.oversized
            .iter()
            .map(String::as_str)
            .chain(self.unreadable.iter().map(|(path, _)| path.as_str()))
            .collect()
    }

    pub(super) fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !self.oversized.is_empty() {
            warnings.push(format!(
                "Skipped {} file(s) larger than the {} KiB materialize per-file limit (location.skipped); read them with ghGetFileContent.",
                self.oversized.len(),
                MATERIALIZE_FILE_BYTES / 1024
            ));
        }
        if !self.unreadable.is_empty() {
            let mut errors: Vec<&str> = Vec::new();
            for (_, message) in &self.unreadable {
                let message = message.trim_end_matches('.');
                if !errors.contains(&message) {
                    errors.push(message);
                }
            }
            warnings.push(format!(
                "Skipped {} unreadable file(s) (location.skipped): {}.",
                self.unreadable.len(),
                errors.join("; ")
            ));
        }
        warnings
    }
}
