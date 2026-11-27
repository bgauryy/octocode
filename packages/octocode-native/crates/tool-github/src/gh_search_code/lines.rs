//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so every file of the
//! page is read (core API quota) and its keyword
//! lines are listed as `"<line>\t<text>"` (the numbered-line form), the way
//! `grep -n` would: every line holding any requested keyword.
use crate::gh_search_code::Withholds;
use futures_util::{FutureExt as _, StreamExt as _, stream::FuturesUnordered};
use octocode_github::{GitHubProvider, ProviderError, ProviderErrorKind, RequestContext};
use octocode_runtime_base::security::scan::ContentScan;
use octocode_tools_shared::gh_shared::text::file_text;
use std::path::Path;

mod blob;
mod budget;
mod kept;
mod scoring;

pub(super) use blob::Declared;
pub(super) use blob::Window;
use blob::{Blob, Hit, hit_declarations};
use budget::{SCANS, fan_out, scan_weight};
use kept::{KeptKey, scan_keeping};

/// Blobs larger than this keep their index fragments. A read stops at this
/// many bytes: a larger body is never downloaded whole ([`resolve_files`]).
pub(super) const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

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
        /// The hit that best fits the goal ([`hit_score`](scoring::hit_score)):
        /// a cut row lists the hits around it.
        best: u32,
        total: usize,
        line_count: usize,
        /// What the hit lines state about the reads around them, in hit
        /// order and at most one a hit: the block declaration a line declares
        /// ([`Blob::declared_blocks`]) or, in a Markdown file, the innermost
        /// section holding it ([`Blob::sections_of`]).
        declared: Vec<Declared>,
        /// Secret-shaped values `lines` show as placeholders (beyond any
        /// literal placeholder of the source): the row's text is then not
        /// verbatim.
        redacted: usize,
    },
    /// The blob was read but no line holds a keyword (index lag or
    /// tokenization differences): keep the index fragments.
    Unmatched,
    /// The path does not exist at the resolved ref.
    Missing,
    /// The path is a symlink to a file the path policy withholds: nothing
    /// of the target was read, and the row lists its path only.
    Withheld,
    /// The blob is over [`MAX_SCAN_BYTES`], so it was not scanned: the row says
    /// so with `lineResolved: false` and the page names the bound.
    Oversized,
    /// The blob could not be read or scanned (binary, transient). The row says
    /// so with `lineResolved: false`.
    Unavailable,
    /// The scan of the blob panicked (a defect, not a read failure): the row
    /// says `lineResolved: false` and the page names the count.
    Panicked,
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
    withholds: Withholds<'_>,
) -> Result<Vec<FileHits>, ProviderError> {
    context.check()?;
    let bounded = read_bound(context);
    let bounded = &bounded;
    // Files of one folder share its symlink check: asked once each before
    // the reads, not by every read at once.
    if paths.len() > 1 {
        provider
            .check_links_once_per_folder(owner, repo, sha, paths, bounded)
            .await;
    }
    // Bytes: the one text rule ([`file_text`]) judges them, as local reads do.
    let mut reads = paths
        .iter()
        .enumerate()
        .map(|(at, path)| async move {
            // A symlink's bytes are its target's: a target the policy
            // withholds is never asked for and shows no line, as the file
            // itself would not.
            let read = provider
                .get_file_bytes_following_links(
                    &octocode_github::ContentRequest {
                        owner: owner.to_owned(),
                        repo: repo.to_owned(),
                        path: path.clone(),
                        reference: Some(sha.to_owned()),
                    },
                    bounded,
                    withholds,
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
                Ok(Err(_)) => out[*at] = FileHits::Withheld,
                // A followed link's text is judged by its target's name.
                Ok(Ok((content, target))) => {
                    let name = target.as_deref().unwrap_or(paths[*at].as_str());
                    blobs.push((*at, content.bytes.as_ref(), name));
                }
                Err(error) if error.is_cancelled_or_timed_out() => {
                    return Err(error.clone());
                }
                Err(error)
                    if error.kind == ProviderErrorKind::NotFound || error.status == Some(404) =>
                {
                    out[*at] = FileHits::Missing;
                }
                // Over the read bound: the body was never downloaded whole.
                Err(error) if error.kind == ProviderErrorKind::ResponseTooLarge => {
                    out[*at] = FileHits::Oversized;
                }
                // A transient failure.
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
pub(super) fn read_bound(context: &RequestContext) -> RequestContext {
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
    // A blob scanned the same way before is answered here; only the others
    // fan out (a repeated page starts no thread).
    let mut out = Vec::with_capacity(files.len());
    let mut unkept = Vec::new();
    for (at, &(bytes, path)) in files.iter().enumerate() {
        let key = KeptKey::of(bytes, path, keywords, goal, window, security);
        match key.as_ref().and_then(KeptKey::kept) {
            Some(hits) => out.push(Some(hits)),
            None => {
                out.push(None);
                unkept.push((at, bytes, path, key));
            }
        }
    }
    let cores = octocode_tools_shared::workers::workers(unkept.len(), None);
    let scanned = fan_out(
        &SCANS,
        context,
        cores,
        &unkept,
        |&(_, bytes, _, _)| scan_weight(bytes),
        |(_, bytes, path, key)| {
            scan_keeping(bytes, path, keywords, goal, window, security, key.clone())
        },
    )?;
    for ((at, ..), hits) in unkept.iter().zip(scanned) {
        out[*at] = Some(hits.unwrap_or(FileHits::Panicked));
    }
    Ok(out
        .into_iter()
        .map(|hits| hits.unwrap_or(FileHits::Panicked))
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
        return FileHits::Oversized;
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
    let lowered: Vec<String> = text.lines().map(str::to_lowercase).collect();
    let found = Blob::find(&lowered, &needles);
    if found.is_empty() {
        return FileHits::Unmatched;
    }
    // Only a file with a keyword line is sealed and outlined: as localFetch
    // seals a source, judged whole, so no listed line shows a secret its
    // neighbors name.
    let Ok(sealed) =
        octocode_tools_shared::source_view::prepare::sealed_view(bytes, Path::new(path), security)
    else {
        return FileHits::Unavailable;
    };
    let mut blob = Blob {
        raw: text.lines().collect(),
        lowered,
        clean: sealed.text.lines().collect(),
        key_blocks: octocode_runtime_base::security::private_key_block_line_ranges(&text),
        declared: Vec::new(),
    };
    if blob.clean.len() != blob.raw.len() {
        // The view's lines would not number the source's: show none.
        return FileHits::Unavailable;
    }
    // Only declarations named on keyword lines are read: a walk of the nodes
    // touching those lines finds them as the whole outline would; the whole
    // outline only when that walk cannot tell.
    let found_lines = found.iter().map(|&(index, _, _)| index).collect::<Vec<_>>();
    let near =
        octocode_engine::signatures::graph_facts::declarations_on_lines(&text, path, &found_lines);
    let outline = near
        .is_none()
        .then(|| octocode_tools_shared::declarations::file_document(&text, path))
        .flatten();
    let declarations = match (&near, &outline) {
        (Some(near), _) => near.as_slice(),
        (None, Some(outline)) => outline.declarations.as_slice(),
        (None, None) => &[],
    };
    blob.declared = hit_declarations(declarations, &found);
    let hits = blob.hits(found, &needles, goal);
    let best = blob.anchor(&hits, &needles);
    let lines = blob.listing(&hits, window);
    let redacted = blob.redactions(&lines);
    // A Markdown file's outline is its sections, the one a block read uses.
    let declared = match octocode_tools_shared::source_view::sections::sections(&text, path) {
        Some(sections) => blob.sections_of(&hits, &sections),
        None => blob.declared_blocks(&hits),
    };
    FileHits::Lines {
        redacted,
        declared,
        lines,
        matched: hits.iter().map(Hit::number).collect(),
        best,
        total: hits.len(),
        line_count: blob.lowered.len(),
    }
}

#[cfg(test)]
mod tests;
