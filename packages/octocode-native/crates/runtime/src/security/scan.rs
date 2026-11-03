//! The scanning seam every read passes through: [`ContentScan`] (implemented by
//! [`super::ContentSecurity`] and test doubles). Each read scans its own
//! source; derived scanner results are never reused by a fresh request. Only
//! a continuation page replays the scanned view its cursor's first page
//! built from the same bytes (`tools::source_view::retained`).
use octocode_contract_types::ErrorCode;
use std::borrow::Cow;
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
    fn redact_key_blocks<'a>(&self, content: &'a str) -> (Cow<'a, str>, bool) {
        crate::security::redact_private_key_blocks(content)
    }
    /// `source` sanitized whole with its line structure kept, so match line
    /// numbers stay source-accurate, and whether it changed (see
    /// [`redact_lines`]). Unchanged text borrows the request's source buffer.
    fn redact_source_lines<'a>(
        &self,
        source: &'a str,
        path: &Path,
    ) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
        redact_lines(self, source, path)
    }
}

/// `source` sanitized with its line structure kept: one whole-source scan,
/// or line by line when the whole scan changes the line count (a multi-line
/// pattern) or exceeds the scanner's size limit.
pub(crate) fn redact_lines<'a>(
    scan: &(impl ContentScan + ?Sized),
    source: &'a str,
    path: &Path,
) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
    redact_with(source, |text| {
        scan.sanitize(text, path).map(|(text, _)| text.into())
    })
}

fn redact_with<'a>(
    source: &'a str,
    sanitize: impl Fn(&'a str) -> Result<Cow<'a, str>, (ErrorCode, String)>,
) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
    let lines = |text: &str| text.split_inclusive('\n').count();
    if let Ok(whole) = sanitize(source) {
        if whole == source {
            return Ok((source.into(), false));
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
        let clean = sanitize(body)?;
        out.push_str(&clean.replace('\n', " "));
        out.push_str(newline);
    }
    let changed = out != source;
    Ok((out.into(), changed))
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
        checked_content(self.sanitize_text(text, Some(path)))
            .map(|text| (text.into_owned(), Vec::new()))
    }

    fn redact_source_lines<'a>(
        &self,
        source: &'a str,
        path: &Path,
    ) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
        redact_with(source, |text| {
            checked_content(self.sanitize_text(text, Some(path)))
        })
    }
}

fn checked_content<'a>(
    result: octocode_engine::security::types::SanitizationResult<'a>,
) -> Result<Cow<'a, str>, (ErrorCode, String)> {
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
    Ok(result.content)
}

#[cfg(test)]
type ScanOutcome = Result<(String, Vec<String>), (ErrorCode, String)>;

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

    /// Repeated reads and changed sources independently pass the same scanner.
    #[test]
    fn whole_source_line_redaction_scans_each_read() {
        let scanner = Counting(AtomicUsize::new(0));
        let scan = &scanner;
        let source = source();
        let path = Path::new("/w/a.env.example");
        let first = scan.redact_source_lines(&source, path).expect("redacted");
        assert!(first.1 && first.0.ends_with("key = [R]\n"));
        assert_eq!(scanner.0.load(Ordering::Relaxed), 1);
        assert_eq!(scan.redact_source_lines(&source, path), Ok(first));
        assert_eq!(
            scanner.0.load(Ordering::Relaxed),
            2,
            "each read independently applies the scanner"
        );
        scan.redact_source_lines(&source, path)
            .expect("other bytes");
        scan.redact_source_lines(&source, Path::new("/w/b"))
            .expect("other path");
        assert_eq!(scanner.0.load(Ordering::Relaxed), 4);
    }

    /// Concurrent reads own their scan work and retain identical redaction.
    #[test]
    fn concurrent_line_redactions_are_independent() {
        struct Slow(AtomicUsize);
        impl ContentScan for Slow {
            fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
                self.0.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok((text.replace("SECRET", "[R]"), Vec::new()))
            }
        }
        let scanner = Slow(AtomicUsize::new(0));
        let scan = &scanner;
        let source = source();
        let gate = std::sync::Barrier::new(8);
        let outcomes = std::thread::scope(|scope| {
            let rows = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        gate.wait();
                        scan.redact_source_lines(&source, Path::new("/w/a.env"))
                    })
                })
                .collect::<Vec<_>>();
            rows.into_iter()
                .map(|row| row.join().expect("row"))
                .collect::<Vec<_>>()
        });
        assert_eq!(scanner.0.load(Ordering::SeqCst), outcomes.len());
        assert!(outcomes.iter().all(|outcome| outcome == &outcomes[0]));
        assert!(
            outcomes[0]
                .as_ref()
                .is_ok_and(|(text, changed)| { *changed && text.ends_with("key = [R]\n") })
        );
    }

    #[test]
    fn repeated_reads_do_not_retain_scanner_results() {
        let scanner = Counting(AtomicUsize::new(0));
        let scan = &scanner;
        let text = source();
        let path = Path::new("/w/repeated.env");
        let first = scan.redact_source_lines(&text, path).unwrap();
        let second = scan.redact_source_lines(&text, path).unwrap();
        assert_eq!(first, second);
        assert_eq!(scanner.0.load(Ordering::Relaxed), 2);
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
        assert_eq!((text.as_ref(), changed), ("a\nb\nc\n", false));
    }
}
