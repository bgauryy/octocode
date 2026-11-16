//! `materialize`: write a listing's whole selection under the Octocode home.
use super::*;
use crate::tools::gh_git::blobs::BlobSource;
use std::path::{Path, PathBuf};

mod blobs;
mod snapshot;
use blobs::{Blobs, Fetched, wave_len};
use snapshot::{FileIdentity, Store};
#[cfg(test)]
mod tests;
mod writer;
pub(super) use writer::{Acquired, MaterializeSkips, MaterializeWriter, write_error};

/// Write the selection's files under `location.path` (the listed
/// directory) and return where a continuation resumes, if anywhere. The whole
/// selection, not the listing page shown: only the call's own time stops it.
pub(super) async fn materialize_page(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    scope: &Scope,
    commit_sha: &str,
    selection: &[TreeEntry],
    home: &Path,
    blobs: Option<&BlobSource>,
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
        blobs,
    };
    let Some(outcome) =
        materialize_tree(provider, &snapshot, selection, offset, &scope.path, context).await?
    else {
        return Ok(None);
    };
    // A read that failed leaves text unwritten: repeating the materialize
    // from this call's own offset reads it again (what the snapshot already
    // holds is not read twice).
    let resume = outcome
        .next_offset
        .or_else(|| outcome.retryable.then_some(offset))
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
    // The checkout is complete when this listing wrote everything it can name
    // as text: no offset left, no folder below the depth, and no file skipped
    // but those that are not text (`location.skipped` still names them).
    let finished = resume.is_none() && unwritten.is_empty() && !outcome.gap;
    location["coverage"] = json!({"state": if finished { "complete" } else { "partial" }});
    value["location"] = location;
    if outcome.gap {
        mark_materialize_partial(value);
    }
    if !unwritten.is_empty() {
        crate::response::paging::mark_partial(
            value,
            crate::response::paging::PartialReason::MaterializeDepth,
            &[],
        );
    }
    crate::response::paging::push_warnings(value, warnings);
    if let Some(hint) = outcome.hint {
        crate::response::paging::push_warnings(value, [hint]);
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
    /// A file failed to read (not oversized): reading it again can write it.
    pub(super) retryable: bool,
    /// What a skip of only binaries says: complete, with their names.
    pub(super) hint: Option<String>,
}

/// The commit a materialize writes, and the Octocode home it writes under.
pub(super) struct Snapshot<'a> {
    pub(super) owner: &'a str,
    pub(super) repo: &'a str,
    pub(super) commit: &'a str,
    pub(super) root: &'a Path,
    /// Where file bytes come from besides REST reads, when git can serve them.
    pub(super) blobs: Option<&'a BlobSource>,
}

/// Where materialized checkouts live under the Octocode home, versioned so
/// a layout change never shares a path with another writer's files.
pub(super) const MATERIALIZE_LAYOUT: [&str; 3] = ["tmp", "materialize", "v4"];

/// How deep each snapshot sits under [`materialize_root`]:
/// `owner/repo/commit`.
pub(crate) const SNAPSHOT_DEPTH: usize = 3;

/// Root of every materialized checkout under the cache home; local tools may
/// read it.
pub(crate) fn materialize_root(home: &Path) -> PathBuf {
    MATERIALIZE_LAYOUT
        .iter()
        .fold(home.to_path_buf(), |path, segment| path.join(segment))
}

/// Write `entries[offset..]` under the locked, content-verified snapshot,
/// reading up to [`read_concurrency`] files ahead and writing them in listing
/// order. The call's own limits stop the walk, never a count of files or bytes:
/// past three quarters of its time, or at a rate limit or deadline, it resumes
/// from that entry with what it wrote kept, so the row still reaches its
/// caller. `location.path` is the listed directory.
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
    let requested_offset = offset;
    let offset = store
        .resume_offset(snapshot.root, entries, requested_offset, context)
        .await?;
    let root = store.root.clone();
    let now = std::time::Instant::now();
    let time_left = context.budget.deadline.saturating_duration_since(now);
    let stop_at = now + time_left.mul_f64(0.75);
    let read_limit = context.budget.max_body_bytes;
    let mut writer = MaterializeWriter {
        root: &root,
        home: snapshot.root,
        written: 0,
        cursor: offset,
        stopped: None,
        skips: store.prior_skips(&entries[..offset], read_limit),
        recorded: Vec::new(),
    };
    let mut git = Blobs::new(snapshot.blobs);
    let mut next = offset;
    'walk: while next < entries.len() {
        // A wave starts only with time left, once something was written.
        if writer.cursor > offset && std::time::Instant::now() >= stop_at {
            break;
        }
        let wave = &entries[next..next + wave_len(&entries[next..], read_limit)];
        next += wave.len();
        let mut acquired =
            acquire_wave(provider, snapshot, &store, &mut git, wave, read_limit).await;
        use futures_util::StreamExt as _;
        // Up to `read_concurrency` REST reads run ahead of the writer; a slow
        // file holds back only the writes after it, never the next reads.
        let mut reads = futures_util::stream::iter(wave.iter().zip(acquired.drain(..)))
            .map(|(entry, held)| async move {
                match held {
                    Some(held) => (entry, held),
                    None => (entry, rest_read(provider, snapshot, entry, context).await),
                }
            })
            .buffered(read_concurrency(provider));
        while let Some((entry, read)) = reads.next().await {
            // The stop applies once the call wrote something: a call that
            // wrote nothing would resume at the same entry for ever.
            let out_of_time = writer.cursor > offset && std::time::Instant::now() >= stop_at;
            if out_of_time || !writer.write(entry, read).await? {
                break 'walk;
            }
        }
    }
    store
        .record(std::mem::take(&mut writer.recorded), &writer.skips)
        .await?;
    let has_more = writer.cursor < entries.len();
    let local = if listed.is_empty() {
        root.clone()
    } else {
        root.join(listed)
    };
    // The snapshot is the commit segment of `location.path`; `coverage` (set
    // by the page) is the only coverage flag.
    let mut location = json!({
        "path": local.to_string_lossy(),
        "coverage": {"state": if has_more { "partial" } else { "complete" }}});
    let mut warnings = writer.skips.warnings();
    if offset != requested_offset {
        warnings.push(format!(
            "A previous materialize file was missing or changed; resumed from entry {offset} instead of {requested_offset}."
        ));
    }
    warnings.extend(writer.stopped.take());
    if !writer.skips.is_empty() {
        location["skipped"] = json!(writer.skips.paths());
    }
    Ok(Some(MaterializeOutcome {
        location,
        next_offset: has_more.then_some(writer.cursor),
        warnings,
        gap: writer.skips.leaves_a_gap(),
        retryable: !writer.skips.unreadable.is_empty(),
        hint: writer.skips.binary_hint(),
    }))
}

/// What each entry of `wave` has before any REST read: what needs no
/// request (not a file, withheld, oversized by its listed size, already in
/// the snapshot), and what git fetched for the rest in one transfer. `None`
/// is read over REST.
async fn acquire_wave(
    provider: &GitHubProvider,
    snapshot: &Snapshot<'_>,
    store: &Store,
    git: &mut Blobs<'_>,
    wave: &[TreeEntry],
    read_limit: usize,
) -> Vec<Option<Acquired>> {
    use futures_util::StreamExt as _;
    let mut acquired: Vec<Option<Acquired>> = futures_util::stream::iter(wave)
        .map(|entry| async move {
            if entry.kind != EntryKind::File {
                return Some(Acquired::NotAFile);
            }
            // A credential-named file is withheld before any request.
            if crate::policy::discovery::is_sensitive_repository_file(&entry.path) {
                return Some(Acquired::Withheld);
            }
            // Past what this call may read, by its listed size: asking
            // would spend a request on the same refusal.
            if entry.size.is_some_and(|size| size > read_limit as u64) {
                return Some(Acquired::Oversized);
            }
            // A file this snapshot already holds is not read again.
            if crate::tools::gh_shared::is_repo_relative(&entry.path)
                && let Some(identity) = store.kept(snapshot.root, &entry.path).await
            {
                return Some(Acquired::Kept(identity));
            }
            None
        })
        .buffered(read_concurrency(provider))
        .collect()
        .await;
    let mut wanted: Vec<String> = wave
        .iter()
        .zip(&acquired)
        .filter(|(_, held)| held.is_none())
        .filter_map(|(entry, _)| entry.oid.clone())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let fetched = git
        .wave(snapshot.owner, snapshot.repo, wanted, read_limit)
        .await;
    for (entry, held) in wave.iter().zip(&mut acquired) {
        let (None, Some(oid)) = (&held, &entry.oid) else {
            continue;
        };
        *held = match &fetched {
            Err(error) => Some(Acquired::Read(Err(error.clone()))),
            Ok(fetched) => match fetched.get(oid) {
                Some(Fetched::Bytes(bytes)) => Some(Acquired::Read(git_read(
                    bytes.clone(),
                    oid,
                    snapshot.commit,
                ))),
                Some(Fetched::Oversized) => Some(Acquired::Oversized),
                None => None,
            },
        };
    }
    acquired
}

/// The read the REST path would have produced for these bytes: admitted by
/// the one text rule, naming the blob in a refusal.
fn git_read(
    bytes: bytes::Bytes,
    oid: &str,
    commit: &str,
) -> Result<octocode_github::ContentResponse, ProviderError> {
    let read = octocode_github::ContentResponse {
        raw_response_bytes: bytes.len(),
        bytes,
        resolved_ref: commit.to_owned(),
        blob: Some(oid.to_owned()),
    };
    crate::tools::gh_shared::text::admit_read(&read)?;
    Ok(read)
}

/// Bytes over REST, admitted by the one text rule: a file it refuses is the
/// typed binary-file failure, skipped as unreadable. The snapshot stores
/// them: no saved answer keeps them again.
async fn rest_read(
    provider: &GitHubProvider,
    snapshot: &Snapshot<'_>,
    entry: &TreeEntry,
    context: &RequestContext,
) -> Acquired {
    let read = provider
        .get_file_bytes_kept_by_caller(
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
    Acquired::Read(read)
}
