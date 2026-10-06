//! The scanning seam every read passes through: [`ContentScan`] (implemented by
//! [`super::ContentSecurity`] and test doubles) and the memo that lets paged
//! reads of one large view reuse a single scan.
use std::path::Path;

pub trait ContentScan {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)>;
    /// Redact whole PEM/OpenSSH/PGP private-key blocks across the FULL file
    /// before any read/search window is cut, closing the interior-window leak the
    /// anchored full-block patterns cannot catch. The default applies to every
    /// implementer (including test mocks); see
    /// [`crate::security::redact_private_key_blocks`].
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        crate::security::redact_private_key_blocks(content)
    }
}

/// A test scan double that returns every text unchanged.
#[cfg(test)]
pub(crate) struct Passthrough;

#[cfg(test)]
impl ContentScan for Passthrough {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (String, String)> {
        Ok((text.to_owned(), vec![]))
    }
}

impl ContentScan for super::ContentSecurity {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)> {
        let result = self.sanitize_text(text, Some(path));
        if result
            .secrets_detected
            .iter()
            .any(|name| name == "content-size-exceeded")
        {
            return Err((
                "contentSecurityLimit".into(),
                "The selected content view exceeds the secret scanner size limit.".into(),
            ));
        }
        Ok((result.content, Vec::new()))
    }
}

/// Remembers the secret-scanner output for recently read views so paging one
/// large file does not rescan the whole blob on every `next.continue`.
///
/// A line/byte page is cut from the sanitized full view, so each page
/// otherwise re-runs `sanitize` over the entire file (~55 ms release for a
/// 176 KB file, dominating a cache-hit read). Entries are keyed by a SHA-256
/// of the scanned text and path, so a hit needs the exact bytes in hand and
/// returns exactly what the scanner produced for them; redaction is unchanged.
/// One memo must only ever wrap one scanner (the owning runtime's policy).
pub struct SanitizedViewMemo {
    entries: std::sync::Mutex<std::collections::VecDeque<MemoEntry>>,
}

type ScanOutcome = Result<(String, Vec<String>), (String, String)>;

struct MemoEntry {
    key: [u8; 32],
    bytes: usize,
    outcome: std::sync::Arc<ScanOutcome>,
}

impl SanitizedViewMemo {
    const MAX_ENTRIES: usize = 8;
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    /// Small views are cheap to rescan; memoizing them only churns entries.
    const MIN_TEXT_BYTES: usize = 16 * 1024;

    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    fn key(text: &str, path: &Path) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        let path = path.to_string_lossy();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update(text.as_bytes());
        digest.finalize().into()
    }

    fn scan(&self, text: &str, path: &Path, inner: &impl ContentScan) -> ScanOutcome {
        if text.len() < Self::MIN_TEXT_BYTES {
            return inner.sanitize(text, path);
        }
        let key = Self::key(text, path);
        {
            let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = entries
                .iter()
                .position(|entry| entry.key == key)
                .and_then(|index| entries.remove(index))
            {
                let outcome = std::sync::Arc::clone(&entry.outcome);
                entries.push_back(entry);
                return (*outcome).clone();
            }
        }
        let outcome = inner.sanitize(text, path);
        let bytes = text.len().saturating_add(match &outcome {
            Ok((safe, warnings)) => safe.len() + warnings.iter().map(String::len).sum::<usize>(),
            Err((code, message)) => code.len() + message.len(),
        });
        if bytes <= Self::MAX_BYTES {
            let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            if !entries.iter().any(|entry| entry.key == key) {
                entries.push_back(MemoEntry {
                    key,
                    bytes,
                    outcome: std::sync::Arc::new(outcome.clone()),
                });
            }
            let mut total: usize = entries.iter().map(|entry| entry.bytes).sum();
            while entries.len() > Self::MAX_ENTRIES || total > Self::MAX_BYTES {
                let Some(evicted) = entries.pop_front() else {
                    break;
                };
                total = total.saturating_sub(evicted.bytes);
            }
        }
        outcome
    }
}

impl Default for SanitizedViewMemo {
    fn default() -> Self {
        Self::new()
    }
}

/// `ContentScan` adapter that routes `sanitize` through a [`SanitizedViewMemo`]
/// and forwards everything else (full-file key-block redaction still runs on
/// every read) to the wrapped scanner.
pub struct MemoizedScan<'a, S> {
    inner: &'a S,
    memo: &'a SanitizedViewMemo,
}

impl<'a, S: ContentScan> MemoizedScan<'a, S> {
    pub fn new(inner: &'a S, memo: &'a SanitizedViewMemo) -> Self {
        Self { inner, memo }
    }
}

impl<S: ContentScan> ContentScan for MemoizedScan<'_, S> {
    fn sanitize(&self, text: &str, path: &Path) -> ScanOutcome {
        self.memo.scan(text, path, self.inner)
    }
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        self.inner.redact_key_blocks(content)
    }
}
