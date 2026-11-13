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
//! Secrets: a kept view was built from a source redacted whole before any
//! view or window existed (`prepare_source`), so a replayed page is cut from
//! the same redacted view a cold read builds, and equals it. The key holds
//! the scanner's limits, so a scan that reads the source differently never
//! replays another's view.
//!
//! Lifetime: a value is kept only by a page that issues a next page, and
//! lives as issued continuation state does ([`PageMemo`]): in process
//! memory only, weighed against the process budget, never written to disk.
//! A one-page read keeps nothing, and a fresh read (no page position) never
//! replays.
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

/// The store key of a value: the source digest and the policy that shaped
/// it, so two reads of the same bytes (another view, another path) each
/// keep their own value instead of evicting each other's.
fn key(sha256: &str, policy: &str) -> String {
    format!("{sha256}\n{policy}")
}

/// What shaped a view besides the source bytes: both paths, whether the bytes
/// start inside the file, the scanner and the limits it reads a source at,
/// and the read when the view depends on it.
fn policy<S: ContentScan>(
    read: Option<&LocalFetchQuery>,
    paths: (&std::path::Path, &str),
    facts: &SourceFacts,
    security: &S,
) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "read": read,
        "sourcePath": paths.0,
        "providerPath": paths.1,
        "interior": facts.interior,
        "scan": std::any::type_name::<S>(),
        "limits": format!("{:?}", security.limits()),
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
    /// any other read (a whole read's walk keeps its view in [`Retention`], and a streamed window
    /// is not a source).
    pub(super) fn of<S: ContentScan>(
        q: &LocalFetchQuery,
        sha256: &str,
        paths: (&std::path::Path, &str),
        facts: &SourceFacts,
        security: &S,
    ) -> Option<Self> {
        if facts.byte_origin.is_some()
            || !(local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q))
        {
            return None;
        }
        Some(Self {
            sha256: sha256.to_owned(),
            policy: policy(None, paths, facts, security)?,
        })
    }

    /// The source these bytes scanned to; a cursor naming other bytes finds
    /// nothing and ends as a stale snapshot.
    pub(super) fn get(&self, q: &LocalFetchQuery) -> Option<Arc<Source<'static>>> {
        if local_fetch_query::snapshot(q).is_some_and(|snapshot| snapshot != self.sha256) {
            return None;
        }
        SCANNED.get(&key(&self.sha256, &self.policy), &self.policy, Arc::clone)
    }

    /// Keep `source`, as the scan left it, for the next read of these bytes.
    pub(super) fn keep(self, source: &Source<'_>) {
        let weight = source_weight(source);
        let kept = owned(source.shared());
        SCANNED.put(
            key(&self.sha256, &self.policy),
            self.policy,
            weight,
            Arc::new(kept),
        );
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
        security: &S,
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
            policy: policy(Some(&read), paths, facts, security)?,
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
        STORE.get(&key(&self.sha256, &self.policy), &self.policy, Arc::clone)
    }

    /// Keep the view `result` was cut from when the page continues; every
    /// page cut from it is scanned (see the module note).
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
        if !continues {
            return;
        }
        let source = owned(source);
        let weight = source_weight(&source)
            + view.text.len()
            + ext.text.len()
            + std::mem::size_of::<usize>()
                * (ext.source_lines.as_ref().map_or(0, Vec::len)
                    + ext.matched_lines.len()
                    + 2 * ext.match_windows.len());
        STORE.put(
            key(&self.sha256, &self.policy),
            self.policy,
            weight,
            Arc::new(Kept { source, ext, view }),
        );
    }
}

/// Bytes retained by a source, charging any shared line index in full so
/// either store can keep it after the other releases its reference.
fn source_weight(source: &Source<'_>) -> usize {
    source.text.len()
        + source
            .offsets
            .as_ref()
            .map_or(0, |offsets| offsets.weight())
        + std::mem::size_of::<(usize, usize)>() * source.redacted_lines.len()
}

/// `source` holding its own text and sharing its existing line index.
fn owned(source: Source<'_>) -> Source<'static> {
    Source {
        sha256: source.sha256,
        text: std::borrow::Cow::Owned(source.text.into_owned()),
        bytes: source.bytes,
        total_lines: source.total_lines,
        offsets: source.offsets,
        key_blocks_redacted: source.key_blocks_redacted,
        match_redacted: source.match_redacted,
        redacted_lines: source.redacted_lines,
        warnings: source.warnings,
    }
}

/// The keys `store` holds for `sha256`, under any policy.
#[cfg(test)]
fn keys_of<V: Send + Sync + 'static>(store: &PageMemo<V>, sha256: &str) -> Vec<String> {
    let prefix = format!("{sha256}\n");
    store
        .contents()
        .0
        .into_iter()
        .filter(|kept| kept.starts_with(&prefix))
        .collect()
}

/// Whether a view is kept for `sha256`.
#[cfg(test)]
pub(crate) fn holds(sha256: &str) -> bool {
    !keys_of(&STORE, sha256).is_empty()
}

/// Forget the views kept for `sha256`: its next page builds them again, as
/// a cold read does.
#[cfg(test)]
pub(crate) fn forget_view(sha256: &str) {
    keys_of(&STORE, sha256)
        .iter()
        .for_each(|kept| STORE.evict(kept));
}

/// Forget the scans kept for `sha256`: its next read scans again.
#[cfg(test)]
pub(crate) fn forget_scan(sha256: &str) {
    keys_of(&SCANNED, sha256)
        .iter()
        .for_each(|kept| SCANNED.evict(kept));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::security::ContentSecurity;
    use std::path::Path;

    /// A raw selection needs its source-line index to place byte windows.
    /// Keeping the index lets every continuation reuse the first scan's
    /// allocation instead of building a source-sized table for each page.
    #[test]
    fn selection_pages_reuse_the_scanned_source_line_index() {
        let source = (0..512)
            .map(|line| format!("let retained_line_{line} = {line};\n"))
            .collect::<String>();
        let path = Path::new("/fixture/retained_source_index.rs");
        let q: LocalFetchQuery = serde_json::from_value(serde_json::json!({
            "path": path,
            "ranges": [{"startLine": 100, "endLine": 400}],
            "limit": 2,
        }))
        .expect("read query");
        let facts = SourceFacts::default();
        let security = ContentSecurity::new();
        let first = super::super::view::process_fetched_content(
            &q,
            source.as_bytes(),
            path,
            &facts,
            &security,
            &NeverCancel,
        );
        assert!(first.error.is_none(), "{first:?}");
        let next = first
            .next
            .and_then(|next| next.continue_window)
            .expect("selection spans several pages");
        let sha256 = octocode_engine::digest::sha256(source.as_bytes());
        let paths = (path, "retained_source_index.rs");
        let scanned = ScannedSource::of(&q, &sha256, paths, &facts, &security)
            .expect("range scan")
            .get(&q)
            .expect("scan retained");
        let kept = Retention::of(&q, &sha256, paths, &facts, &security)
            .expect("view retention")
            .replay(&next.query)
            .expect("view retained");
        assert!(Arc::ptr_eq(
            scanned.offsets.as_ref().expect("scanned source index"),
            kept.source.offsets.as_ref().expect("retained source index"),
        ));
        forget_view(&sha256);
        forget_scan(&sha256);
    }
}
