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
/// ([`crate::cache::MEMORY`]); a view over all of it is not kept (its pages
/// rebuild it).
static STORE: PageMemo<Arc<Kept>> = PageMemo::new();

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
        let policy = serde_json::to_string(&serde_json::json!({
            "read": read,
            "sourcePath": paths.0,
            "providerPath": paths.1,
            "interior": facts.interior,
            "scan": std::any::type_name::<S>(),
        }))
        .ok()?;
        Some(Self {
            sha256: sha256.to_owned(),
            policy,
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

/// `source` holding its own text (its line offsets were spent on its
/// extraction).
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
