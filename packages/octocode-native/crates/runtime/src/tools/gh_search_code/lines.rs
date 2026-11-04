//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so every file of the
//! page is read (core API quota) and its keyword
//! lines are listed as `"<line>\t<text>"` (the numbered-line form), the way
//! `grep -n` would: every line holding any requested keyword.
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::text::file_text;
use futures_util::{FutureExt as _, StreamExt as _, stream::FuturesUnordered};
use octocode_github::{GitHubProvider, ProviderError, ProviderErrorKind, RequestContext};
use std::path::Path;

mod blob;
mod budget;
mod scoring;

pub(super) use blob::Window;
use blob::{Blob, Hit, hit_declarations};
use budget::{SCANS, fan_out, scan_weight};

/// Blobs larger than this keep their index fragments. A read stops at this
/// many bytes: a larger body is never downloaded whole ([`resolve_files`]).
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

/// What each file's listing is built from: the requested keywords, the goal
/// that ranks their hits and the context listed around each.
pub(super) struct Listing<'a> {
    pub(super) keywords: &'a [String],
    pub(super) goal: &'a str,
    pub(super) window: Window,
}

/// One file's hits at a resolved commit.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FileHits {
    /// Every keyword line in source order, preserved whole for response pagination.
    Lines {
        lines: Vec<String>,
        /// The hit lines' numbers, ascending: `lines` also lists context
        /// when it holds more entries than this.
        matched: Vec<u32>,
        first: u32,
        last: u32,
        /// The hit that best fits the goal ([`hit_score`]).
        best: u32,
        total: usize,
        line_count: usize,
        /// The best hit's line, verbatim and trimmed, when it declares a
        /// block (a function, class, ...) and no other line of the file
        /// holds it: a `matchString` + `block` read returns that whole
        /// declaration.
        declaration: Option<String>,
    },
    /// The blob was read but no line holds a keyword (index lag or
    /// tokenization differences): keep the index fragments.
    Unmatched,
    /// The path does not exist at the resolved ref.
    Missing,
    /// The blob could not be read or scanned (binary, over [`MAX_SCAN_BYTES`],
    /// transient). The row says so with `lineResolved: false`.
    Unavailable,
}

/// Read each path at `sha` concurrently and list its keyword lines. A read
/// stops at [`MAX_SCAN_BYTES`] (the file stays `Unavailable`, like any
/// blob the scan refuses). Blobs are scanned as they arrive, whatever has
/// arrived together on parallel threads ([`scan_all`]), and dropped: only
/// the reads in flight hold a body. Cancellation and deadline expiry
/// abort; other read failures leave the file `Unavailable`.
pub(super) async fn resolve_files(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    sha: &str,
    paths: &[String],
    listing: &Listing<'_>,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Vec<FileHits>, ProviderError> {
    context.check()?;
    let bounded = read_bound(context);
    let bounded = &bounded;
    // Bytes: the one text rule ([`file_text`]) judges them, as local reads do.
    let mut reads = paths
        .iter()
        .enumerate()
        .map(|(at, path)| async move {
            let read = provider
                .get_file_bytes(
                    &octocode_github::ContentRequest {
                        owner: owner.to_owned(),
                        repo: repo.to_owned(),
                        path: path.clone(),
                        reference: Some(sha.to_owned()),
                    },
                    bounded,
                )
                .await;
            (at, read)
        })
        .collect::<FuturesUnordered<_>>();
    let mut out = vec![FileHits::Unavailable; paths.len()];
    while let Some(first) = reads.next().await {
        let mut arrived = vec![first];
        while let Some(Some(next)) = reads.next().now_or_never() {
            arrived.push(next);
        }
        context.check()?;
        let mut blobs = Vec::new();
        for (at, result) in &arrived {
            match result {
                Ok(content) => blobs.push((*at, content.bytes.as_ref(), paths[*at].as_str())),
                Err(error) if error.is_cancelled_or_timed_out() => {
                    return Err(error.clone());
                }
                Err(error)
                    if error.kind == ProviderErrorKind::NotFound || error.status == Some(404) =>
                {
                    out[*at] = FileHits::Missing;
                }
                // Over the read bound, or a transient failure.
                Err(_) => {}
            }
        }
        let scanned = scan_all(
            &blobs
                .iter()
                .map(|&(_, bytes, path)| (bytes, path))
                .collect::<Vec<_>>(),
            listing,
            security,
            context,
        )?;
        for ((at, _, _), hits) in blobs.into_iter().zip(scanned) {
            out[at] = hits;
        }
    }
    Ok(out)
}

/// `context` for a hit's read: its body limit never exceeds what the scan
/// accepts, so a file the scan would refuse is not downloaded whole.
fn read_bound(context: &RequestContext) -> RequestContext {
    let mut bounded = context.clone();
    bounded.budget.max_body_bytes = bounded.budget.max_body_bytes.min(MAX_SCAN_BYTES);
    bounded
}

/// [`scan`] of each `(bytes, path)`, in order, on scoped threads within the
/// process's scan budget ([`SCANS`]). A scan that panics leaves its file
/// `Unavailable` (its index fragments stay) rather than failing the row.
fn scan_all(
    files: &[(&[u8], &str)],
    listing: &Listing<'_>,
    security: &(impl ContentScan + Sync),
    context: &RequestContext,
) -> Result<Vec<FileHits>, ProviderError> {
    let Listing {
        keywords,
        goal,
        window,
    } = *listing;
    // This scan computes on this host: it counts toward the process's CPU
    // budget while it runs and fans out to its share of the cores.
    let _computing = octocode_engine::cpu::host().enter();
    let cores = crate::tools::workers::workers(files.len(), None);
    Ok(fan_out(
        &SCANS,
        context,
        cores,
        files,
        |&(bytes, _)| scan_weight(bytes),
        |&(bytes, path)| scan(bytes, path, keywords, goal, window, security),
    )?
    .into_iter()
    .map(|hits| hits.unwrap_or(FileHits::Unavailable))
    .collect())
}

/// Case-insensitive keyword lines of one blob with `window` lines of
/// context around each, numbered from 1.
pub(super) fn scan(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    goal: &str,
    window: Window,
    security: &impl ContentScan,
) -> FileHits {
    if bytes.len() > MAX_SCAN_BYTES {
        return FileHits::Unavailable;
    }
    let Ok(text) = file_text(bytes) else {
        return FileHits::Unavailable;
    };
    let needles: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    if needles.is_empty() {
        return FileHits::Unmatched;
    }
    let mut blob = Blob {
        raw: text.lines().collect(),
        lowered: text.lines().map(str::to_lowercase).collect(),
        clean: None,
        key_blocks: Vec::new(),
        declared: Vec::new(),
        path,
    };
    let found = blob.found(&needles);
    if found.is_empty() {
        return FileHits::Unmatched;
    }
    // Only a file with a keyword line is outlined.
    blob.key_blocks = crate::security::private_key_block_line_ranges(&text);
    if octocode_engine::security::WholeSourcePatterns::for_path(path)
        .is_some_and(|patterns| patterns.named_by(&text))
    {
        let Ok((clean, _)) = security.redact_source_lines(&text, Path::new(path)) else {
            return FileHits::Unavailable;
        };
        blob.clean = Some(clean.lines().map(str::to_owned).collect());
    }
    let outline = crate::tools::declarations::file_document(&text, path);
    blob.declared = hit_declarations(outline.as_deref(), &found);
    let hits = blob.hits(found, &needles, goal);
    let anchor = blob.anchor(&hits, &needles, security);
    let lines = blob.listing(&hits, window, security);
    FileHits::Lines {
        declaration: blob.declaration(&hits, anchor.best, security),
        lines,
        matched: hits.iter().map(Hit::number).collect(),
        first: anchor.first,
        last: anchor.last,
        best: anchor.best,
        total: hits.len(),
        line_count: blob.lowered.len(),
    }
}

#[cfg(test)]
mod tests;
