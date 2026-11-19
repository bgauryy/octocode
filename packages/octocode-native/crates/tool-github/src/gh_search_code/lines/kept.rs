//! Scans kept by what they read: a repeated search whose files a saved answer
//! served lists them as scanned before ([`SCANNED`]).
use super::{FileHits, Window, scan};
use octocode_runtime_base::security::scan::ContentScan;

/// Blobs as scanned, by everything the scan reads: the bytes (by digest),
/// the path, the keywords, the goal and the window, under the scanner's
/// limits. A repeated search whose files a saved answer served lists them
/// as scanned before, not sealed and outlined again. Only the process's own
/// scanner's work is kept.
static SCANNED: octocode_tools_shared::page_memo::PageMemo<FileHits> =
    octocode_tools_shared::page_memo::PageMemo::new();

/// Where [`SCANNED`] keeps a scan: its key and the scanner limits it was
/// made under. `None` when the scan is not the process scanner's own work,
/// which is never kept.
#[derive(Clone)]
pub(super) struct KeptKey {
    key: String,
    policy: String,
}

impl KeptKey {
    pub(super) fn of(
        bytes: &[u8],
        path: &str,
        keywords: &[String],
        goal: &str,
        window: Window,
        security: &impl ContentScan,
    ) -> Option<Self> {
        let scanner = security.kept()?;
        Some(Self {
            key: format!(
                "{}\n{path}\n{keywords:?}\n{goal}\n{}:{}",
                octocode_engine::digest::sha256(bytes),
                window.before,
                window.after,
            ),
            policy: format!("{:?}", scanner.limits()),
        })
    }

    /// The kept scan, when the same blob was scanned the same way.
    pub(super) fn kept(&self) -> Option<FileHits> {
        SCANNED.get(&self.key, &self.policy, Clone::clone)
    }
}

/// [`scan`], answered from [`SCANNED`] when the same blob was scanned the
/// same way, and kept there after.
#[cfg(test)]
pub(super) fn scan_kept(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    goal: &str,
    window: Window,
    security: &impl ContentScan,
) -> FileHits {
    let key = KeptKey::of(bytes, path, keywords, goal, window, security);
    if let Some(hits) = key.as_ref().and_then(KeptKey::kept) {
        return hits;
    }
    scan_keeping(bytes, path, keywords, goal, window, security, key)
}

/// [`scan`], kept under `key` ([`KeptKey::of`]) when it has one.
pub(super) fn scan_keeping(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    goal: &str,
    window: Window,
    security: &impl ContentScan,
    key: Option<KeptKey>,
) -> FileHits {
    let hits = scan(bytes, path, keywords, goal, window, security);
    let Some(KeptKey { key, policy }) = key else {
        return hits;
    };
    // A scan a deadline stopped (its outline cut short) is never kept.
    if octocode_engine::deep_stack::current_stop()
        .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed))
    {
        return hits;
    }
    let weight = match &hits {
        FileHits::Lines { lines, .. } => lines.iter().map(String::len).sum::<usize>() + 64,
        _ => 64,
    };
    SCANNED.put(key, policy, weight, hits.clone());
    hits
}
