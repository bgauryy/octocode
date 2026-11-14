//! The scanning seam every read passes through: [`ContentScan`] (implemented by
//! [`super::ContentSecurity`] and test doubles). A read's scan is a pure function
//! of its source bytes, path and scanner, so a result is reused only under a key
//! holding all of them, in this process: a range or match read reuses the source
//! a previous read of the same bytes at the same path scanned, and a continuation
//! page replays the view its cursor's first page built
//! (`tools::source_view::retained`). A file whose bytes changed scans again.
use octocode_contract_types::ErrorCode;
use std::borrow::Cow;
use std::path::Path;

pub(crate) trait ContentScan {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)>;
    /// The source sizes this scanner judges at; a test double that does not say
    /// is judged at the real ones.
    fn limits(&self) -> super::SourceLimits {
        super::SourceLimits::REAL
    }
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
    /// The process's scanner when this scan is it: the one whose whole-source
    /// pass is kept by text, path and limits (`tools::judged_rows`). `None`
    /// for any other scan, of which nothing is kept and to which nothing
    /// kept is served.
    fn kept(&self) -> Option<&super::ContentSecurity> {
        None
    }
}

/// `source` sanitized with its line structure kept: one whole-source scan.
/// A whole scan that changes the line count (a rewrite that joined lines) has
/// no line to give back for each source line, and judging lines one by one
/// would serve a value that spans lines (a continued property, a key block)
/// less redacted than the whole scan hides it. The source is then withheld:
/// every line becomes the sanitizer-failure placeholder, so line numbers hold,
/// and the read reports the text as redacted. A scanner's refusal is the
/// read's refusal.
pub(crate) fn redact_lines<'a>(
    scan: &(impl ContentScan + ?Sized),
    source: &'a str,
    path: &Path,
) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
    redact_with(source, |text| {
        scan.sanitize(text, path).map(|(text, _)| text.into())
    })
}

/// [`redact_lines`] of `source` by `sanitize`, the whole scan of a text.
pub(crate) fn redact_with<'a>(
    source: &'a str,
    sanitize: impl Fn(&'a str) -> Result<Cow<'a, str>, (ErrorCode, String)>,
) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
    let lines = |text: &str| text.split_inclusive('\n').count();
    let whole = sanitize(source)?;
    if whole == source {
        return Ok((source.into(), false));
    }
    if lines(&whole) == lines(source) {
        return Ok((whole, true));
    }
    Ok((withheld_lines(source).into(), true))
}

/// `source` with the text of every line replaced by the sanitizer-failure
/// placeholder, its line breaks kept.
pub(super) fn withheld_lines(source: &str) -> String {
    source
        .split_inclusive('\n')
        .map(|segment| {
            let carriage = if segment.ends_with("\r\n") {
                "\r\n"
            } else if segment.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            format!(
                "{}{carriage}",
                octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER
            )
        })
        .collect()
}

/// [`ContentScan::sanitize`] of provider text: the redacted text, or the
/// scanner's refusal message as a `Validation` provider error.
pub(crate) fn sanitize_provider_text(
    security: &(impl ContentScan + ?Sized),
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
/// shows still redacts its bare copy in a title. Returns how many values it
/// replaced by placeholders.
pub(crate) fn sanitize_provider_json(
    security: &impl ContentScan,
    value: &mut serde_json::Value,
    path: &Path,
) -> Result<usize, octocode_github::ProviderError> {
    let before = super::placeholders_in(value);
    super::sanitize_json(value, &mut |text, _| {
        sanitize_provider_text(security, text, path)
    })?;
    Ok(super::placeholders_in(value).saturating_sub(before))
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
    fn limits(&self) -> super::SourceLimits {
        Self::limits(self)
    }
    fn kept(&self) -> Option<&Self> {
        Some(self)
    }
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((
            self.sanitize_text(text, Some(path)).content.into_owned(),
            Vec::new(),
        ))
    }
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

    /// Text past the scanner's limit, as a read of a source that grew when
    /// decoded hands it over: `line`, then pad words past the limit.
    fn over_the_limit(line: &str) -> String {
        let pad = "ordinary words\n";
        let lines = octocode_engine::security::MAX_CONTENT_SIZE / pad.len() + 1;
        format!("{line}\n{}", pad.repeat(lines))
    }

    /// Past what a read holds at once, a source is judged as any other: a known
    /// secret is hidden, and the text around it stays.
    #[test]
    fn past_the_limit_a_source_is_judged_whole() {
        let security = crate::security::ContentSecurity::new();
        let path = Path::new("/w/dump.json");
        let token = format!("ghp_{}", "c2VjcmV0".repeat(5));
        let text = over_the_limit(&format!("{{\"token\":\"{token}\"}}"));
        let (clean, changed) = security.redact_source_lines(&text, path).expect("read");
        assert!(changed);
        assert!(!clean.contains(&token));
        assert!(clean.contains("ordinary words"));
        for plain in [
            "{\"SecretName\":\"a\"}",
            "\"kind\": \"Deployment\"",
            "plain",
        ] {
            let text = over_the_limit(plain);
            let (clean, changed) = security.redact_source_lines(&text, path).expect("read");
            assert!(!changed, "{plain}");
            assert_eq!(clean.len(), text.len(), "{plain}");
        }
    }

    /// A whole scan that changes the line count (a rewrite that joined the
    /// lines of a continued value) is never answered by judging lines one by
    /// one: that serves the value's tail unredacted. Every line is withheld,
    /// lines and their breaks keep their numbers, and the read says it
    /// redacted.
    #[test]
    fn a_line_count_change_withholds_every_line() {
        struct Joining;
        impl ContentScan for Joining {
            fn sanitize(&self, text: &str, _: &Path) -> ScanOutcome {
                Ok((text.replace("a\\\nb", "[R]"), Vec::new()))
            }
        }
        let source = "a\\\nb\nsecondline\r\nlast";
        let (text, changed) = redact_lines(&Joining, source, Path::new("x")).expect("ok");
        let marker = octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER;
        assert!(changed);
        assert_eq!(text, format!("{marker}\n{marker}\n{marker}\r\n{marker}"));
        assert!(!text.contains("secondline"));
        assert_eq!(text.matches('\n').count(), source.matches('\n').count());
    }

    /// A scanner that refuses the whole source refuses the read; no line is
    /// scanned on its own.
    #[test]
    fn a_whole_source_refusal_is_the_reads_refusal() {
        struct Refusing;
        impl ContentScan for Refusing {
            fn sanitize(&self, _: &str, _: &Path) -> ScanOutcome {
                Err((ErrorCode::ContentSecurityLimit, "too large".into()))
            }
        }
        let refused = redact_lines(&Refusing, "a\nb\n", Path::new("x")).expect_err("refused");
        assert_eq!(refused.0, ErrorCode::ContentSecurityLimit);
    }
}
