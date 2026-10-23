//! The scanning seam every read passes through: [`ContentScan`] (implemented by
//! [`super::ContentSecurity`] and test doubles) and the memo that lets paged
//! reads of one large view reuse a single scan.
use octocode_contract_types::ErrorCode;
use std::path::Path;

pub(crate) trait ContentScan {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)>;
    /// Redact whole PEM/OpenSSH/PGP private-key blocks across the FULL file
    /// before any read/search window is cut, closing the interior-window leak the
    /// anchored full-block patterns cannot catch. The default applies to every
    /// implementer (including test mocks); see
    /// [`crate::security::redact_private_key_blocks`].
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        crate::security::redact_private_key_blocks(content)
    }
    /// `source` sanitized whole with its line structure kept, so match line
    /// numbers stay source-accurate, and whether it changed (see
    /// [`redact_lines`]). `digest` is the SHA-256 the caller holds of the
    /// bytes `source` was derived from, by a pure decode and key-block
    /// redaction: a memoizing scanner keys on it rather than hashing the
    /// text again.
    fn redact_source_lines(
        &self,
        source: &str,
        path: &Path,
        _digest: &str,
    ) -> Result<(String, bool), (ErrorCode, String)> {
        redact_lines(self, source, path)
    }
}

/// `source` sanitized with its line structure kept: one whole-source scan,
/// or line by line when the whole scan changes the line count (a multi-line
/// pattern) or exceeds the scanner's size limit.
pub(crate) fn redact_lines(
    scan: &(impl ContentScan + ?Sized),
    source: &str,
    path: &Path,
) -> Result<(String, bool), (ErrorCode, String)> {
    let lines = |text: &str| text.split_inclusive('\n').count();
    if let Ok((whole, _)) = scan.sanitize(source, path) {
        if whole == source {
            return Ok((whole, false));
        }
        if lines(&whole) == lines(source) {
            return Ok((whole, true));
        }
    }
    let mut out = String::with_capacity(source.len());
    for segment in source.split_inclusive('\n') {
        let (body, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |body| (body, "\n"));
        let (clean, _) = scan.sanitize(body, path)?;
        out.push_str(&clean.replace('\n', " "));
        out.push_str(newline);
    }
    let changed = out != source;
    Ok((out, changed))
}

/// [`ContentScan::sanitize`] of provider text: the redacted text, or the
/// scanner's refusal message as a `Validation` provider error.
pub(crate) fn sanitize_provider_text(
    security: &impl ContentScan,
    text: &str,
    path: &Path,
) -> Result<String, octocode_github::ProviderError> {
    security
        .sanitize(text, path)
        .map(|(text, _)| text)
        .map_err(|(_, message)| {
            octocode_github::ProviderError::new(
                octocode_github::ProviderErrorKind::Validation,
                message,
            )
        })
}

/// Every string leaf of provider `value` sanitized as [`sanitize_provider_text`]
/// at `path`, through the one response walker ([`super::sanitize_json`]): a
/// value redacted where its context shows is redacted wherever else the
/// payload repeats it bare, so a secret named in a body that a row never
/// shows still redacts its bare copy in a title.
pub(crate) fn sanitize_provider_json(
    security: &impl ContentScan,
    value: &mut serde_json::Value,
    path: &Path,
) -> Result<(), octocode_github::ProviderError> {
    super::sanitize_json(value, &mut |text, _| {
        sanitize_provider_text(security, text, path)
    })
}

/// A test scan double that returns every text unchanged.
#[cfg(test)]
pub(crate) struct Passthrough;

#[cfg(test)]
impl ContentScan for Passthrough {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.to_owned(), vec![]))
    }
}

impl ContentScan for super::ContentSecurity {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        let result = self.sanitize_text(text, Some(path));
        if result
            .secrets_detected
            .iter()
            .any(|name| name == "content-size-exceeded")
        {
            return Err((
                ErrorCode::ContentSecurityLimit,
                "The selected content view exceeds the secret scanner size limit.".into(),
            ));
        }
        Ok((result.content, Vec::new()))
    }
}

/// Remembers the secret-scanner output for recently read views so paging one
/// large file does not rescan the whole blob on every `next.continueWindow`.
///
/// A line/byte page is cut from the sanitized full view, so each page
/// otherwise re-runs `sanitize` over the entire file (~1.6 ms release for a
/// 75 KB file, measured 2026-10-23, the largest cost of a cache-hit read). Entries are keyed by a SHA-256
/// of the scanned text and path, so a hit needs the exact bytes in hand and
/// returns exactly what the scanner produced for them; redaction is unchanged.
/// One memo must only ever wrap one scanner (the owning runtime's policy).
pub(crate) struct SanitizedViewMemo {
    entries: octocode_io::memo::Memo<[u8; 32], ScanOutcome>,
    /// One whole-source line redaction per key at a time: rows redacting
    /// one source concurrently wait for the first and take its outcome
    /// from the slot. Turns are blocking and held in synchronous code only.
    redacting: octocode_io::flight::KeyedFlight<[u8; 32], Option<ScanOutcome>>,
}

type ScanOutcome = Result<(String, Vec<String>), (ErrorCode, String)>;

impl SanitizedViewMemo {
    /// Small views are cheap to rescan; memoizing them only churns entries.
    const MIN_TEXT_BYTES: usize = 16 * 1024;

    pub fn new() -> Self {
        Self {
            entries: octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
                max_entries: 8,
                max_weight: 32 * 1024 * 1024,
                keep_newest: false,
            }),
            redacting: octocode_io::flight::KeyedFlight::transient(),
        }
    }

    /// Key of a scan: what it is (`purpose`), the path, and the text, or a
    /// digest that names the text for the caller.
    fn key(purpose: &str, path: &Path, text: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        let path = path.to_string_lossy();
        for part in [purpose.as_bytes(), path.as_bytes(), text] {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part);
        }
        digest.finalize().into()
    }

    /// [`redact_lines`] of `source`, remembered per path and source digest
    /// (with the text's length as a guard), so every page of a matchString
    /// read, and every concurrent row reading the source, redacts the whole
    /// source once instead of once each.
    fn redact_source_lines(
        &self,
        source: &str,
        path: &Path,
        digest: &str,
        inner: &impl ContentScan,
    ) -> Result<(String, bool), (ErrorCode, String)> {
        if source.len() < Self::MIN_TEXT_BYTES {
            return redact_lines(inner, source, path);
        }
        let named = format!("{digest}:{}", source.len());
        let key = Self::key("sourceLines", path, named.as_bytes());
        // The scan holds no cancellation: a waiter waits for the one scan.
        let Ok(outcome) = self.redacting.memoized_blocking(
            key,
            |key| self.entries.get(key).map(|hit| hit.value.as_ref().clone()),
            |key| {
                let outcome = redact_lines(inner, source, path).map(|(text, _)| (text, Vec::new()));
                self.store(key, source.len(), outcome)
            },
            || Ok::<(), std::convert::Infallible>(()),
        );
        outcome.map(|(text, _)| {
            let changed = text != source;
            (text, changed)
        })
    }

    /// Store `outcome` of a `scanned`-byte text, weighed by both, and hand
    /// it back.
    fn store(&self, key: [u8; 32], scanned: usize, outcome: ScanOutcome) -> ScanOutcome {
        let bytes = scanned.saturating_add(match &outcome {
            Ok((safe, warnings)) => safe.len() + warnings.iter().map(String::len).sum::<usize>(),
            Err((code, message)) => crate::id::error_codes::as_str(*code).len() + message.len(),
        });
        let stored = self.entries.insert(key, outcome, bytes, None);
        std::sync::Arc::try_unwrap(stored).unwrap_or_else(|shared| shared.as_ref().clone())
    }

    /// The scan of `text`, remembered for large views. The caller owns the
    /// returned outcome, so a hit copies it once, outside the memo's lock.
    fn scan(&self, text: &str, path: &Path, inner: &impl ContentScan) -> ScanOutcome {
        if text.len() < Self::MIN_TEXT_BYTES {
            return inner.sanitize(text, path);
        }
        let key = Self::key("view", path, text.as_bytes());
        if let Some(hit) = self.entries.get(&key) {
            return hit.value.as_ref().clone();
        }
        self.store(key, text.len(), inner.sanitize(text, path))
    }
}

impl Default for SanitizedViewMemo {
    fn default() -> Self {
        Self::new()
    }
}

/// `ContentScan` adapter that routes `sanitize` and the whole-source line
/// redaction through a [`SanitizedViewMemo`] and forwards everything else
/// (full-file key-block redaction still runs on every read) to the wrapped
/// scanner.
pub(crate) struct MemoizedScan<'a, S> {
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
    fn redact_source_lines(
        &self,
        source: &str,
        path: &Path,
        digest: &str,
    ) -> Result<(String, bool), (ErrorCode, String)> {
        self.memo
            .redact_source_lines(source, path, digest, self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A scanner refusal reaches the provider error as the scanner's own
    /// message (the reason the text was withheld), not its code.
    #[test]
    fn provider_text_refusals_carry_the_scanner_message() {
        struct Refusing;
        impl ContentScan for Refusing {
            fn sanitize(&self, _: &str, _: &Path) -> ScanOutcome {
                Err((
                    ErrorCode::ContentSecurityLimit,
                    "The selected content view exceeds the secret scanner size limit.".into(),
                ))
            }
        }
        let error = sanitize_provider_text(&Refusing, "text", Path::new("github-history"))
            .expect_err("refused");
        assert_eq!(error.kind, octocode_github::ProviderErrorKind::Validation);
        assert_eq!(
            &*error.message,
            "The selected content view exceeds the secret scanner size limit."
        );
    }

    /// Redacts `SECRET` (a multi-line hit when it spans lines) and counts
    /// its scans.
    struct Counting(AtomicUsize);

    impl ContentScan for Counting {
        fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok((text.replace("SECRET", "[R]"), Vec::new()))
        }
    }

    fn source() -> String {
        let mut text = "plain line\n".repeat(2_000);
        text.push_str("key = SECRET\n");
        text
    }

    /// The whole-source line redaction of one source is scanned once per
    /// path and digest: a later page reuses it, and another digest or path
    /// scans again.
    #[test]
    fn whole_source_line_redaction_is_memoized_per_path_and_digest() {
        let scanner = Counting(AtomicUsize::new(0));
        let memo = SanitizedViewMemo::new();
        let scan = MemoizedScan::new(&scanner, &memo);
        let source = source();
        let path = Path::new("/w/a.env.example");
        let first = scan
            .redact_source_lines(&source, path, "d1")
            .expect("redacted");
        assert!(first.1 && first.0.ends_with("key = [R]\n"));
        assert_eq!(scanner.0.load(Ordering::Relaxed), 1);
        assert_eq!(scan.redact_source_lines(&source, path, "d1"), Ok(first));
        assert_eq!(
            scanner.0.load(Ordering::Relaxed),
            1,
            "a page reuses the scan"
        );
        scan.redact_source_lines(&source, path, "d2")
            .expect("other bytes");
        scan.redact_source_lines(&source, Path::new("/w/b"), "d1")
            .expect("other path");
        assert_eq!(scanner.0.load(Ordering::Relaxed), 3);
    }

    /// Rows redacting one source at once scan it once: the others wait for
    /// the first scan and share its outcome.
    #[test]
    fn concurrent_line_redactions_of_one_source_scan_it_once() {
        struct Slow(AtomicUsize);
        impl ContentScan for Slow {
            fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
                self.0.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok((text.replace("SECRET", "[R]"), Vec::new()))
            }
        }
        let scanner = Slow(AtomicUsize::new(0));
        let memo = SanitizedViewMemo::new();
        let scan = MemoizedScan::new(&scanner, &memo);
        let source = source();
        let gate = std::sync::Barrier::new(8);
        let outcomes = std::thread::scope(|scope| {
            let rows = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        gate.wait();
                        scan.redact_source_lines(&source, Path::new("/w/a.env"), "d1")
                    })
                })
                .collect::<Vec<_>>();
            rows.into_iter()
                .map(|row| row.join().expect("row"))
                .collect::<Vec<_>>()
        });
        assert_eq!(scanner.0.load(Ordering::SeqCst), 1, "one scan for the wave");
        assert!(outcomes.iter().all(|outcome| outcome == &outcomes[0]));
        assert!(
            outcomes[0]
                .as_ref()
                .is_ok_and(|(text, changed)| { *changed && text.ends_with("key = [R]\n") })
        );
    }

    /// A whole scan that changes the line count falls back to line by line,
    /// so lines keep their numbers.
    #[test]
    fn a_line_count_change_falls_back_to_line_scans() {
        struct Joining;
        impl ContentScan for Joining {
            fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
                Ok((text.replace("a\nb", "[R]"), Vec::new()))
            }
        }
        let (text, changed) = redact_lines(&Joining, "a\nb\nc\n", Path::new("x")).expect("ok");
        assert_eq!((text.as_str(), changed), ("a\nb\nc\n", false));
    }
}
