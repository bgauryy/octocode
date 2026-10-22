//! `materialize`: write a listing page's files under the Octocode home.
use super::*;
use std::path::{Path, PathBuf};

mod snapshot;
use snapshot::{FileIdentity, Store};
#[cfg(test)]
mod tests;

/// Write this page's files under `location.localPath` (the listed
/// directory) and return where a continuation resumes, if anywhere.
pub(super) async fn materialize_page(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
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
        }),
        None => page.has_more().then_some(MaterializeResume {
            page: page.current + 1,
            offset: 0,
        }),
    };
    let mut warnings = outcome.warnings;
    // Folders at the listing's depth are listed, not written: say so, so a
    // local search below them is not mistaken for absence.
    let unwritten = unwritten_folders(page.entries, scope);
    if let Some(first) = unwritten.first() {
        warnings.push(format!(
            "{} listed folder(s) at maxDepth {} were not written.",
            unwritten.len(),
            scope.depth
        ));
        value["next"]["expandDepth"] = expand_depth(query, scope, commit_sha, first)?;
    }
    let mut location = outcome.location;
    // The checkout is complete only when this listing wrote everything it
    // names: no later page or offset, no folder below the depth, no skip.
    let finished = resume.is_none() && unwritten.is_empty() && !outcome.skipped;
    location["coverage"] = json!({"state": if finished { "complete" } else { "partial" }});
    value["location"] = location;
    if !unwritten.is_empty() {
        crate::response::paging::mark_partial(value, "materializeDepth", &[]);
    }
    crate::response::paging::push_warnings(value, warnings);
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
    query: &GhStructureQueryTree,
    scope: &Scope,
    commit_sha: &str,
    first_unwritten: &str,
) -> Result<Value, ProviderError> {
    let mut next = query_row(
        &GhStructureQueryTree {
            ref_: Some(commit_sha.parse().map_err(
                |error: octocode_contract_types::error::ConversionError| {
                    ProviderError::new(ProviderErrorKind::Decode, error.to_string())
                },
            )?),
            ..query.clone()
        },
        &["page", "materializeOffset"],
    );
    let deepest = max_listing_depth();
    next["maxDepth"] = json!(deepest);
    next["materialize"] = json!(true);
    if scope.depth >= deepest {
        next["path"] = json!(first_unwritten);
    }
    Ok(continuation(next))
}

#[derive(Clone, Copy)]
pub(super) struct MaterializeResume {
    pub(super) page: usize,
    pub(super) offset: usize,
}

/// A materialized checkout that misses files the listing names leaves the
/// row partial until `next.continueMaterialize` writes the rest.
pub(super) fn mark_materialize_partial(value: &mut Value) {
    if value.get("location").is_none() {
        return;
    }
    crate::response::paging::mark_partial(value, "materializeIncomplete", &[]);
}

pub(super) const MATERIALIZE_FILE_CAP: usize = 50;

pub(super) const MATERIALIZE_FILE_BYTES: usize = 300 * 1024;

pub(super) const MATERIALIZE_TOTAL_BYTES: usize = 5 * 1024 * 1024;

pub(super) struct MaterializeOutcome {
    pub(super) location: Value,
    pub(super) next_offset: Option<usize>,
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

/// Where materialized checkouts live under the Octocode home, versioned so
/// a layout change never shares a path with another writer's files.
pub(super) const MATERIALIZE_LAYOUT: [&str; 3] = ["tmp", "materialize", "v3"];

/// Root of every materialized checkout under the cache home; local tools may
/// read it.
pub(crate) fn materialize_root(home: &Path) -> PathBuf {
    MATERIALIZE_LAYOUT
        .iter()
        .fold(home.to_path_buf(), |path, segment| path.join(segment))
}

/// Write `entries[offset..]` under the locked, content-verified snapshot,
/// reading up to [`READ_CONCURRENCY`] files ahead and writing them in
/// listing order, so the write caps stop at the same entry as a serial
/// walk. A cached file is reused only when its actual bytes match the recorded
/// identity. `location.localPath` is the listed directory.
pub(super) async fn materialize_tree(
    provider: &GitHubProvider,
    snapshot: &Snapshot<'_>,
    entries: &[TreeEntry],
    offset: usize,
    listed: &str,
    context: &RequestContext,
) -> Result<Option<MaterializeOutcome>, ProviderError> {
    if offset >= entries.len() {
        return Ok(None);
    }
    let mut store = Store::open(snapshot, context).await?;
    let root = store.root.clone();
    let mut writer = MaterializeWriter {
        root: &root,
        home: snapshot.root,
        written: 0,
        total_bytes: 0,
        cursor: offset,
        skips: MaterializeSkips::default(),
        recorded: Vec::new(),
    };
    {
        use futures_util::StreamExt as _;
        let store = &store;
        // Up to READ_CONCURRENCY reads run ahead of the writer; a slow file
        // holds back only the writes after it, never the next reads.
        let mut reads = futures_util::stream::iter(&entries[offset..])
            .map(|entry| async move {
                if entry.kind != EntryKind::File {
                    return (entry, Acquired::NotAFile);
                }
                if entry
                    .size
                    .is_some_and(|size| size > MATERIALIZE_FILE_BYTES as u64)
                {
                    return (entry, Acquired::Oversized);
                }
                if store.reusable(entry).await {
                    return (entry, Acquired::Reused);
                }
                let read = provider
                    .get_file_content(
                        &octocode_github::ContentRequest {
                            owner: snapshot.owner.to_owned(),
                            repo: snapshot.repo.to_owned(),
                            path: entry.path.clone(),
                            reference: Some(snapshot.commit.to_owned()),
                            force_refresh: false,
                        },
                        context,
                    )
                    .await;
                (entry, Acquired::Read(read))
            })
            .buffered(READ_CONCURRENCY);
        while let Some((entry, read)) = reads.next().await {
            if !writer.write(entry, read).await? {
                break;
            }
        }
    }
    store.record(std::mem::take(&mut writer.recorded)).await?;
    let has_more = writer.cursor < entries.len();
    let local = if listed.is_empty() {
        root.clone()
    } else {
        root.join(listed)
    };
    // The snapshot is the commit segment of `localPath`; `coverage` (set
    // by the page) is the only coverage flag.
    let mut location = json!({
        "localPath": local.to_string_lossy(),
        "coverage": {"state": if has_more { "partial" } else { "complete" }},
    });
    let warnings = writer.skips.warnings();
    let skipped = !writer.skips.is_empty();
    if skipped {
        location["skipped"] = json!(writer.skips.paths());
    }
    Ok(Some(MaterializeOutcome {
        location,
        next_offset: has_more.then_some(writer.cursor),
        warnings,
        skipped,
    }))
}

/// What a materialize page has for one listed entry.
pub(super) enum Acquired {
    NotAFile,
    /// Larger than [`MATERIALIZE_FILE_BYTES`] by its listed size.
    Oversized,
    /// Already on disk, as the manifest recorded it.
    Reused,
    Read(Result<octocode_github::ContentResponse, ProviderError>),
}

/// The ordered write side of a materialize page and its caps.
pub(super) struct MaterializeWriter<'a> {
    pub(super) root: &'a Path,
    pub(super) home: &'a Path,
    pub(super) written: usize,
    pub(super) total_bytes: usize,
    pub(super) cursor: usize,
    pub(super) skips: MaterializeSkips,
    /// Files this page wrote completely, for the manifest.
    pub(super) recorded: Vec<(String, FileIdentity)>,
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
            return Ok(false);
        }
        if self.total_bytes >= MATERIALIZE_TOTAL_BYTES {
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
                if !access_failure(&error) && !error.is_cancelled_or_timed_out() =>
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
        snapshot::write_file(self.home, self.root, &entry.path, &acquired.bytes).await?;
        self.total_bytes += acquired.bytes.len();
        self.written += 1;
        self.recorded
            .push((entry.path.clone(), FileIdentity::of(&acquired.bytes)));
        Ok(true)
    }
}

pub(super) fn write_error(action: &str, error: &std::io::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Configuration,
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
                "Skipped {} file(s) larger than the {} KiB materialize per-file limit (location.skipped).",
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
