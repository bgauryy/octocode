//! The view a read's first page built, kept for the pages its cursor
//! continues. A continuation page of the same source bytes and the same
//! read replays the kept view and only cuts its page: no second decode,
//! source-wide secret scan, extraction or outline.
//!
//! Exactness: a value is keyed by the digest of the source bytes the page
//! was built from and by everything else that shaped the view (the read
//! without its page position, both paths, whether the bytes start inside
//! the file, and the scanner). Each page digests the bytes it read before it
//! looks a value up, and a cursor whose snapshot names other bytes ends as
//! a stale snapshot before any lookup, so a changed file never replays.
//!
//! Secrets: only a view whose every kept text passed the source-wide scan
//! is kept ([`super::view::View::sanitized`]): a pattern or range read's
//! line-redacted source, and an outline rendered from its redacted source.
//! Private-key blocks were redacted across the whole source before either,
//! so a block spanning page windows stays redacted on every replayed page.
//!
//! Lifetime: a value is kept only by a page that issues a next page, and
//! lives as issued continuation state does ([`PageMemo`]); a one-page read
//! keeps nothing, and a fresh read (no page position) never replays.
//!
//! [`ScannedSource`] is the fresh read's side: the source a range or match
//! read scanned whole, for the next such read of the same bytes. Its key holds
//! all a scan depends on besides the bytes: redaction reads the path (the same
//! bytes redact `password:` lines in `a.yaml` and not in `a.rs`), so a digest
//! alone would serve one file's text for the other.
use super::extraction::Extraction;
use super::prepare::Source;
use super::types::{LocalFetchResult, SourceFacts};
use super::view::View;
use crate::security::scan::ContentScan;
use crate::tools::page_memo::PageMemo;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;
use std::sync::Arc;

/// A first page's view of its source, as every later page cuts from it.
pub(super) struct Kept {
    pub(super) source: Source<'static>,
    pub(super) ext: Extraction,
    pub(super) view: View,
}

/// Kept views, weighed in estimated bytes against the one process budget
/// ([`octocode_engine::memory::MEMORY`]); a view over all of it is not kept (its pages
/// rebuild it).
static STORE: PageMemo<Arc<Kept>> = PageMemo::new();

/// Whole scanned sources, weighed against the same budget; a source over all
/// of it is not kept (the next read scans again).
static SCANNED: PageMemo<Arc<Source<'static>>> = PageMemo::new();

/// What shaped a view besides the source bytes: both paths, whether the bytes
/// start inside the file, the scanner, and the read when the view depends on it.
fn policy<S: ContentScan>(
    read: Option<&LocalFetchQuery>,
    paths: (&std::path::Path, &str),
    facts: &SourceFacts,
) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "read": read,
        "sourcePath": paths.0,
        "providerPath": paths.1,
        "interior": facts.interior,
        "scan": std::any::type_name::<S>(),
    }))
    .ok()
}

/// What keys a whole source's scan: its digest and everything else redaction
/// reads.
pub(super) struct ScannedSource {
    sha256: String,
    policy: String,
}

impl ScannedSource {
    /// The key of a range or match read of whole `sha256` bytes; `None` for
    /// any other read (a whole read scans only its page, and a streamed window
    /// is not a source).
    pub(super) fn of<S: ContentScan>(
        q: &LocalFetchQuery,
        sha256: &str,
        paths: (&std::path::Path, &str),
        facts: &SourceFacts,
    ) -> Option<Self> {
        if facts.byte_origin.is_some()
            || !(local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q))
        {
            return None;
        }
        Some(Self {
            sha256: sha256.to_owned(),
            policy: policy::<S>(None, paths, facts)?,
        })
    }

    /// The source these bytes scanned to; a cursor naming other bytes finds
    /// nothing and ends as a stale snapshot.
    pub(super) fn get(&self, q: &LocalFetchQuery) -> Option<Arc<Source<'static>>> {
        if local_fetch_query::snapshot(q).is_some_and(|snapshot| snapshot != self.sha256) {
            return None;
        }
        SCANNED.get(&self.sha256, &self.policy, Arc::clone)
    }

    /// Keep `source`, as the scan left it, for the next read of these bytes.
    pub(super) fn keep(self, source: &Source<'_>) {
        let offsets = source.total_lines + 1;
        let weight = source.text.len()
            + std::mem::size_of::<usize>() * (offsets + 2 * source.redacted_lines.len());
        let kept = Source {
            offsets: source.offsets.clone(),
            ..owned(source.shared())
        };
        SCANNED.put(self.sha256, self.policy, weight, Arc::new(kept));
    }
}

/// What keys a read's kept view: the source digest and the read's policy.
pub(super) struct Retention {
    sha256: String,
    policy: String,
}

impl Retention {
    /// The retention of a read of a whole source, or `None` for a read whose
    /// pages another owner continues (a streamed window, whose caller
    /// rewrites its cursor) or a classified read (its plan token owns its
    /// pages).
    pub(super) fn of<S: ContentScan>(
        q: &LocalFetchQuery,
        sha256: &str,
        paths: (&std::path::Path, &str),
        facts: &SourceFacts,
    ) -> Option<Self> {
        if facts.byte_origin.is_some()
            || local_fetch_query::has_classification(q)
            || local_fetch_query::selection(q).is_some()
        {
            return None;
        }
        let mut read = q.clone();
        local_fetch_query::set_progress(&mut read, None);
        local_fetch_query::set_snapshot(&mut read, None).ok()?;
        Some(Self {
            sha256: sha256.to_owned(),
            policy: policy::<S>(Some(&read), paths, facts)?,
        })
    }

    /// The view kept for this continuation page: only a page with a position
    /// whose snapshot, if any, names these bytes (another snapshot is stale).
    pub(super) fn replay(&self, q: &LocalFetchQuery) -> Option<Arc<Kept>> {
        if local_fetch_query::read_progress(q).is_none()
            || local_fetch_query::snapshot(q).is_some_and(|snapshot| snapshot != self.sha256)
        {
            return None;
        }
        STORE.get(&self.sha256, &self.policy, Arc::clone)
    }

    /// Keep the view `result` was cut from when the page continues and the
    /// view holds only scanned text.
    pub(super) fn keep(
        self,
        result: &LocalFetchResult,
        source: Source<'_>,
        ext: Extraction,
        view: View,
    ) {
        let continues = result
            .next
            .as_ref()
            .is_some_and(|next| next.continue_window.is_some());
        if !continues || !view.sanitized {
            return;
        }
        let source = owned(source);
        let weight = source.text.len()
            + view.text.len()
            + ext.text.len()
            + std::mem::size_of::<usize>()
                * (ext.source_lines.as_ref().map_or(0, Vec::len)
                    + ext.matched_lines.len()
                    + 2 * source.redacted_lines.len());
        STORE.put(
            self.sha256,
            self.policy,
            weight,
            Arc::new(Kept { source, ext, view }),
        );
    }
}

/// `source` holding its own text, without its line offsets.
fn owned(source: Source<'_>) -> Source<'static> {
    Source {
        sha256: source.sha256,
        text: std::borrow::Cow::Owned(source.text.into_owned()),
        bytes: source.bytes,
        total_lines: source.total_lines,
        offsets: None,
        key_blocks_redacted: source.key_blocks_redacted,
        match_redacted: source.match_redacted,
        redacted_lines: source.redacted_lines,
        warnings: source.warnings,
    }
}

/// Whether a view is kept for `sha256`.
#[cfg(test)]
pub(crate) fn holds(sha256: &str) -> bool {
    STORE.contents().0.iter().any(|kept| kept == sha256)
}

/// Forget the scan kept for `sha256`: its next read scans again.
#[cfg(test)]
pub(crate) fn forget_scan(sha256: &str) {
    SCANNED.evict(sha256);
}
