//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so every file of the
//! page is read (core API quota, through the contents cache) and its keyword
//! lines are listed as `"<line>\t<text>"` (the numbered-line form), the way
//! `grep -n` would: every line holding any requested keyword.
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::text::file_text;
use octocode_github::{GitHubProvider, ProviderError, ProviderErrorKind, RequestContext};

mod blob;
mod budget;
mod scoring;

use blob::{Blob, hit_declarations};
use budget::{SCANS, fan_out, scan_weight};

/// Blobs larger than this keep their index fragments.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

/// One file's hits at a resolved commit.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FileHits {
    /// Every keyword line in source order, preserved whole for response pagination.
    Lines {
        lines: Vec<String>,
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
    /// The blob could not be read or scanned (binary, too large, transient).
    Unavailable,
}

/// Read each path at `sha` concurrently and list its keyword lines, the
/// read blobs scanned on parallel threads ([`scan_all`]). Cancellation
/// and deadline expiry abort; other read failures leave the file `Unavailable`.
pub(super) async fn resolve_files(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    sha: &str,
    paths: &[String],
    keywords: &[String],
    goal: &str,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Vec<FileHits>, ProviderError> {
    context.check()?;
    // Bytes: the one text rule ([`file_text`]) judges them, as local reads do.
    let reads = paths.iter().map(|path| async move {
        provider
            .get_file_bytes(
                &octocode_github::ContentRequest {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                    path: path.clone(),
                    reference: Some(sha.to_owned()),
                    force_refresh: false,
                },
                context,
            )
            .await
    });
    let fetched = futures_util::future::join_all(reads).await;
    context.check()?;
    let mut out = Vec::with_capacity(paths.len());
    let mut blobs = Vec::new();
    for (path, result) in paths.iter().zip(&fetched) {
        out.push(match result {
            Ok(content) => {
                blobs.push((out.len(), content.bytes.as_ref(), path.as_str()));
                FileHits::Unavailable
            }
            Err(error) if error.is_cancelled_or_timed_out() => {
                return Err(error.clone());
            }
            Err(error)
                if error.kind == ProviderErrorKind::NotFound || error.status == Some(404) =>
            {
                FileHits::Missing
            }
            Err(_) => FileHits::Unavailable,
        });
    }
    let scanned = scan_all(
        &blobs
            .iter()
            .map(|&(_, bytes, path)| (bytes, path))
            .collect::<Vec<_>>(),
        keywords,
        goal,
        security,
        context,
    )?;
    for ((at, _, _), hits) in blobs.into_iter().zip(scanned) {
        out[at] = hits;
    }
    Ok(out)
}

/// [`scan`] of each `(bytes, path)`, in order, on scoped threads within the
/// process's scan budget ([`SCANS`]). A scan that panics leaves its file
/// `Unavailable` (its index fragments stay) rather than failing the row.
fn scan_all(
    files: &[(&[u8], &str)],
    keywords: &[String],
    goal: &str,
    security: &(impl ContentScan + Sync),
    context: &RequestContext,
) -> Result<Vec<FileHits>, ProviderError> {
    // This scan computes on this host: it counts toward the process's CPU
    // budget while it runs and fans out to its share of the cores.
    let _computing = octocode_engine::search::cpu::host().enter();
    let cores = crate::tools::workers::workers(files.len(), None);
    Ok(fan_out(
        &SCANS,
        context,
        cores,
        files,
        |&(bytes, _)| scan_weight(bytes),
        |&(bytes, path)| scan(bytes, path, keywords, goal, security),
    )?
    .into_iter()
    .map(|hits| hits.unwrap_or(FileHits::Unavailable))
    .collect())
}

/// Case-insensitive keyword lines of one blob, numbered from 1.
pub(super) fn scan(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    goal: &str,
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
    let outline = crate::tools::declarations_cache::file_document(&text, path);
    blob.declared = hit_declarations(outline.as_deref(), &found);
    let hits = blob.hits(found, &needles, goal);
    let anchor = blob.anchor(&hits, &needles, security);
    let lines = blob.listing(&hits, security);
    FileHits::Lines {
        declaration: blob.declaration(&hits, anchor.best, security),
        lines,
        first: anchor.first,
        last: anchor.last,
        best: anchor.best,
        total: hits.len(),
        line_count: blob.lowered.len(),
    }
}

#[cfg(test)]
mod tests;
