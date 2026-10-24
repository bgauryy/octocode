//! `materialize`: write a listing page's files under the Octocode home.
use super::*;
use std::path::{Path, PathBuf};

mod snapshot;
use snapshot::{FileIdentity, Store};
#[cfg(test)]
mod tests;
mod writer;
pub(super) use writer::{Acquired, MaterializeSkips, MaterializeWriter, write_error};

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
        value["calls"]["expandDepth"] = expand_depth(query, scope, commit_sha, first)?;
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
                // Bytes, admitted by the one text rule: a file it refuses
                // is the typed binary-file failure, skipped as unreadable.
                let read = provider
                    .get_file_bytes(
                        &octocode_github::ContentRequest {
                            owner: snapshot.owner.to_owned(),
                            repo: snapshot.repo.to_owned(),
                            path: entry.path.clone(),
                            reference: Some(snapshot.commit.to_owned()),
                            force_refresh: false,
                        },
                        context,
                    )
                    .await
                    .and_then(|read| {
                        crate::tools::gh_shared::text::admit_read(&read)?;
                        Ok(read)
                    });
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
        "coverage": {"state": if has_more { "partial" } else { "complete" }}});
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
