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

pub trait ContentScan {
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
pub fn redact_lines<'a>(
    scan: &(impl ContentScan + ?Sized),
    source: &'a str,
    path: &Path,
) -> Result<(Cow<'a, str>, bool), (ErrorCode, String)> {
    redact_with(source, |text| {
        scan.sanitize(text, path).map(|(text, _)| text.into())
    })
}

/// [`redact_lines`] of `source` by `sanitize`, the whole scan of a text.
pub fn redact_with<'a>(
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
pub fn sanitize_provider_text(
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
pub fn sanitize_provider_json(
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
#[cfg(any(test, feature = "test-support"))]
pub struct Passthrough;

#[cfg(any(test, feature = "test-support"))]
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
mod tests;
