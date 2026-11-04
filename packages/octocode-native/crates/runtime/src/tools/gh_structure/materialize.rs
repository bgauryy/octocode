//! `materialize`: write a listing's whole selection under the Octocode home.
use super::*;
use std::path::{Path, PathBuf};

mod snapshot;
use snapshot::{FileIdentity, Store};
#[cfg(test)]
mod tests;
mod writer;
pub(super) use writer::{Acquired, MaterializeSkips, MaterializeWriter, write_error};

/// Write the selection's files under `location.localPath` (the listed
/// directory) and return where a continuation resumes, if anywhere. The whole
/// selection, not the listing page shown: only the call's own time stops it.
pub(super) async fn materialize_page(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    scope: &Scope,
    commit_sha: &str,
    page: &ListingPage<'_>,
    selection: &[TreeEntry],
    home: &Path,
    context: &RequestContext,
    value: &mut Value,
) -> Result<Option<MaterializeResume>, ProviderError> {
    let offset = query
        .materialize_offset
        .map_or(0, octocode_engine::num::saturating_usize);
    let snapshot = Snapshot {
        owner: &query.owner,
        repo: &query.repo,
        commit: commit_sha,
        root: home,
    };
    let Some(outcome) =
        materialize_tree(provider, &snapshot, selection, offset, &scope.path, context).await?
    else {
        return Ok(None);
    };
    let resume = outcome
        .next_offset
        .map(|offset| MaterializeResume { offset });
    let mut warnings = outcome.warnings;
    // Folders at the listing's depth are listed, not written: say so, so a
    // local search below them is not mistaken for absence.
    let unwritten = unwritten_folders(selection, scope);
    if let Some(first) = unwritten.first() {
        warnings.push(format!(
            "{} listed folder(s) at maxDepth {} were not written.",
            unwritten.len(),
            scope.depth
        ));
        value["calls"]["expandDepth"] = expand_depth(query, scope, commit_sha, first)?;
    }
    let mut location = outcome.location;
    // The first write of a listing leads into the local tools: the best file
    // it wrote, read from the copy.
    if offset == 0
        && page.current == 1
        && let Some(local) = location["localPath"].as_str()
    {
        let skipped = location["skipped"]
            .as_array()
            .map_or_else(Vec::new, |paths| {
                paths.iter().filter_map(Value::as_str).collect::<Vec<_>>()
            });
        let end = outcome.next_offset.unwrap_or(selection.len());
        let written = selection[..end]
            .iter()
            .filter(|entry| !skipped.contains(&entry.path.as_str()));
        if let Some(read) = local_entry_read(query, written, scope, Path::new(local)) {
            value["calls"]["read"] = read;
        }
    }
    // The checkout is complete when this listing wrote everything it can name
    // as text: no offset left, no folder below the depth, and no file skipped
    // but those that are not text (`location.skipped` still names them).
    let finished = resume.is_none() && unwritten.is_empty() && !outcome.gap;
    location["coverage"] = json!({"state": if finished { "complete" } else { "partial" }});
    value["location"] = location;
    if !unwritten.is_empty() {
        crate::response::paging::mark_partial(
            value,
            crate::response::paging::PartialReason::MaterializeDepth,
            &[],
        );
    }
    crate::response::paging::push_warnings(value, warnings);
    if let Some(hint) = outcome.hint {
        value["hints"] = json!([hint]);
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

/// Where in the selection a continuation resumes.
#[derive(Clone, Copy)]
pub(super) struct MaterializeResume {
    pub(super) offset: usize,
}

/// A materialized checkout that misses files the listing names leaves the
/// row partial until `next.continueMaterialize` writes the rest.
pub(super) fn mark_materialize_partial(value: &mut Value) {
    if value.get("location").is_none() {
        return;
    }
    crate::response::paging::mark_partial(
        value,
        crate::response::paging::PartialReason::MaterializeIncomplete,
        &[],
    );
}

pub(super) struct MaterializeOutcome {
    pub(super) location: Value,
    pub(super) next_offset: Option<usize>,
    pub(super) warnings: Vec<String>,
    /// A skip that leaves text unwritten (`location.skipped`): not a binary.
    pub(super) gap: bool,
    /// What a skip of only binaries says: complete, with their names.
    pub(super) hint: Option<String>,
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
/// reading up to [`READ_CONCURRENCY`] files ahead and writing them in listing
/// order. The call's own limits stop the walk, never a count of files or bytes:
/// past three quarters of its time, or at a rate limit or deadline, it resumes
/// from that entry with what it wrote kept, so the row still reaches its
/// caller. `location.localPath` is the listed directory.
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
    let now = std::time::Instant::now();
    let time_left = context
        .budget
        .effective_deadline()
        .saturating_duration_since(now);
    let stop_at = now + time_left.mul_f64(0.75);
    let read_limit = context.budget.max_body_bytes;
    let mut writer = MaterializeWriter {
        root: &root,
        home: snapshot.root,
        written: 0,
        cursor: offset,
        stopped: None,
        skips: MaterializeSkips::new(read_limit),
        recorded: Vec::new(),
    };
    {
        use futures_util::StreamExt as _;
        // Up to READ_CONCURRENCY reads run ahead of the writer; a slow file
        // holds back only the writes after it, never the next reads.
        let mut reads = futures_util::stream::iter(&entries[offset..])
            .map(|entry| async move {
                if entry.kind != EntryKind::File {
                    return (entry, Acquired::NotAFile);
                }
                // Past what this call may read, by its listed size: asking
                // would spend a request on the same refusal.
                if entry.size.is_some_and(|size| size > read_limit as u64) {
                    return (entry, Acquired::Oversized);
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
            if std::time::Instant::now() >= stop_at || !writer.write(entry, read).await? {
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
    let mut warnings = writer.skips.warnings();
    warnings.extend(writer.stopped.take());
    if !writer.skips.is_empty() {
        location["skipped"] = json!(writer.skips.paths());
    }
    Ok(Some(MaterializeOutcome {
        location,
        next_offset: has_more.then_some(writer.cursor),
        warnings,
        gap: writer.skips.leaves_a_gap(),
        hint: writer.skips.binary_hint(),
    }))
}
