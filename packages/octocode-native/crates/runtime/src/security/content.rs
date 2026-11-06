mod key_blocks;
mod key_markers;
use key_blocks::opens_with_body_run;
pub(crate) use key_blocks::{
    KeyBlockState, KeyBlockTracker, LineShapeScan, hide_key_line, key_line_lead,
    match_window_intersects_key_block,
};
pub(crate) use key_markers::MarkerScan;
use key_markers::{line_markers, may_hold_key_marker};
use std::path::Path;

use crate::policy::{PolicyError, PolicyErrorCode};
use octocode_engine::security::types::SanitizationResult;

/// Secret label recorded when the split-private-key window guard fires.
const SPLIT_KEY_SECRET: &str = "privateKeyFragment";

/// A private-key boundary line (BEGIN or END), in any format
/// [`key_markers`] names. Public certificates (`-----BEGIN CERTIFICATE-----`)
/// are intentionally excluded: they are not secret.
fn is_private_key_boundary(line: &str) -> bool {
    line_markers(line.as_bytes()).any()
}

/// A base64 body line as emitted inside PEM/OpenSSH/PGP key blocks (wrapped at
/// 64–76 chars). The 80-char cap avoids redacting ordinary long source lines.
fn is_key_body_line(line: &str) -> bool {
    let trimmed = line.trim();
    (16..=80).contains(&trimmed.len())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
}

/// File names that can hold a private key without carrying a recognizable
/// BEGIN/END boundary in the selected view. This is deliberately conservative:
/// a `.pem` file may contain a public certificate, but exposing an ambiguous
/// base64-only window is worse than redacting that window.
fn is_private_key_path(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = octocode_engine::text::extension_of(&path.to_string_lossy(), true, "");
    matches!(extension.as_str(), "pem" | "key" | "p8" | "pk8" | "ppk")
        || matches!(
            file_name.as_str(),
            "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519"
        )
}

/// Full-file guard: redact every line of each *private-key* block in any format
/// (from its BEGIN marker through its END marker, inclusive), line-for-line.
///
/// This runs on the **whole file** before any bounded read/search window is cut,
/// which is the gap the anchored built-in patterns leave: those match only a
/// complete BEGIN…END block in a single view, so a window that selects interior
/// body lines (no marker) leaks the key. Scanning the full file lets us poison
/// exactly the block's source lines so any later window of them is already safe.
///
/// Line count and the trailing newline are preserved so the tool's reported
/// source-line ranges stay accurate. Content with no private-key marker is
/// returned byte-identical — ordinary base64 (config blobs, hashes, minified
/// assets) and public `CERTIFICATE` blocks are never touched. Inside a block
/// only key text is hidden (see [`KeyBlockState`]): code after a literal marker
/// string stays readable.
pub(crate) fn redact_private_key_blocks(content: &str) -> (std::borrow::Cow<'_, str>, bool) {
    let (text, edits) = private_key_blocks_hidden(content);
    (text, !edits.is_empty())
}

/// [`redact_private_key_blocks`] with what it replaced of `content`, by byte
/// position: empty when it changed nothing.
pub(crate) fn private_key_blocks_hidden(
    content: &str,
) -> (std::borrow::Cow<'_, str>, octocode_engine::security::Edits) {
    if !may_hold_key_marker(content) {
        return (content.into(), octocode_engine::security::Edits::default());
    }
    let mut block = KeyBlockState::default();
    match hide_lines(content, |line| {
        block.push_line(line).then(|| hide_key_line(line))
    }) {
        Some((text, edits)) => (text.into(), edits),
        None => (content.into(), octocode_engine::security::Edits::default()),
    }
}

/// `text` rewritten line by line as [`str::lines`] splits it, joined by `\n`
/// with its final line break kept, and what that replaced by byte position: a
/// line `hide` returns a text for goes whole, and a `\r` the split drops goes
/// alone. `None` when `hide` changes no line.
fn hide_lines(
    text: &str,
    mut hide: impl FnMut(&str) -> Option<String>,
) -> Option<(String, octocode_engine::security::Edits)> {
    let mut out = String::with_capacity(text.len());
    let mut replaced = Vec::new();
    let mut hid_any = false;
    let mut start = 0;
    for piece in text.split_inclusive('\n') {
        let (line, carriage) = match piece.strip_suffix('\n') {
            Some(body) => body
                .strip_suffix('\r')
                .map_or((body, false), |line| (line, true)),
            None => (piece, false),
        };
        let end = start + line.len();
        if let Some(hidden) = hide(line) {
            hid_any = true;
            replaced.push((start..end, hidden.len()));
            out.push_str(&hidden);
        } else {
            out.push_str(line);
        }
        if carriage {
            replaced.push((end..end + 1, 0));
        }
        if piece.ends_with('\n') {
            out.push('\n');
        }
        start += piece.len();
    }
    hid_any.then(|| (out, octocode_engine::security::Edits::of_pass(replaced)))
}

/// Placeholder emitted for a redacted private-key fragment. Shared by the
/// full-file block guard and the localSearch match guard so redactions read
/// identically wherever a key body is stripped.
pub(crate) fn key_fragment_placeholder() -> String {
    octocode_engine::security::redaction_placeholder(SPLIT_KEY_SECRET)
}

/// Cheap trigger: does this search-match snippet contain any line shaped like a
/// private-key boundary or base64 body? Used by localSearch to decide whether a
/// file is worth a full-file scan; innocent base64 merely triggers a scan that
/// finds no block and redacts nothing. A plain body line always satisfies
/// [`is_key_body_line`]. A short, quoted or commented one may not: the source
/// guard every shown row passes tracks blocks over the whole file regardless.
pub(crate) fn snippet_may_hold_key_material(snippet: &str) -> bool {
    snippet
        .lines()
        .any(|line| is_private_key_boundary(line) || is_key_body_line(line))
}

/// 1-based inclusive line ranges of private-key blocks in
/// `content`. Empty when none. Lets localSearch redact only the matches whose
/// window intersects a real key block. A range holds hidden lines only, so a
/// block with a line of prose in its body is two ranges.
pub(crate) fn private_key_block_line_ranges(content: &str) -> Vec<(u32, u32)> {
    if !may_hold_key_marker(content) {
        return Vec::new();
    }
    let mut tracker = KeyBlockTracker::default();
    for line in content.lines() {
        tracker.push(line);
    }
    tracker.finish()
}

/// Redact private-key material from a single sanitized leaf when a key boundary
/// marker survived full-block redaction — the signal that a bounded read or
/// search-context window split the key across its BEGIN/END boundary, so the
/// anchored built-in patterns (which need a complete block) could not match.
/// Returns `None` when no boundary marker is present, so whole-file reads
/// (already collapsed to one marker) and non-key content stay byte-identical.
/// Redaction is line-for-line, preserving the leaf's line count so the tool's
/// reported line ranges remain accurate.
fn redact_split_private_key(text: &str, file_path: Option<&Path>) -> Option<String> {
    split_private_key_hidden(text, file_path).map(|(text, _)| text)
}

/// [`redact_split_private_key`] with what it replaced of `text`, by byte
/// position.
fn split_private_key_hidden(
    text: &str,
    file_path: Option<&Path>,
) -> Option<(String, octocode_engine::security::Edits)> {
    let has_boundary = text.lines().any(is_private_key_boundary);
    if !has_boundary && !is_private_key_path(file_path) {
        return None;
    }
    hide_lines(text, |line| {
        (is_private_key_boundary(line) || is_key_body_line(line) || opens_with_body_run(line))
            .then(|| hide_key_line(line))
    })
}

/// 1-based inclusive line ranges of `text` that a row cut from it cannot be
/// judged alone: its key blocks, and the lines a whole-file scan redacts when it
/// names a pattern whose context can lie outside any row. Nothing on those lines
/// tells the output gate what they are.
pub(crate) fn lines_a_row_cannot_judge(text: &str, path: &Path) -> Vec<(u32, u32)> {
    use octocode_engine::security::WholeSourcePatterns;
    let mut lines = private_key_block_line_ranges(text);
    let spans = WholeSourcePatterns::for_path(&path.to_string_lossy())
        .is_some_and(|patterns| patterns.named_by(text))
        || WholeSourcePatterns::any_file().named_by(text);
    if spans {
        let scan = ContentSecurity::new().sanitize_text(text, Some(path));
        lines.extend(
            text.lines()
                .zip(scan.content.lines())
                .enumerate()
                .filter(|(_, (raw, clean))| raw != clean)
                .map(|(at, _)| {
                    let line = octocode_engine::num::saturating_u32(at + 1);
                    (line, line)
                }),
        );
    }
    lines
}

/// The content policy every reader is given, with the source sizes it judges at.
#[derive(Clone, Debug, Default)]
pub struct ContentSecurity {
    limits: super::SourceLimits,
}

/// Conservative email shape for opt-in masking of gh outputs. Local-part
/// and domain are both masked; the match is intentionally simple (no
/// quoted local-parts) — commit metadata uses plain addresses.
fn email_pattern() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+")
            .expect("static email pattern compiles")
    })
}

#[expect(
    clippy::unused_self,
    reason = "ContentSecurity is the stateless content-policy handle every reader is given; its checks stay methods on it rather than splitting the API by which ones read no state"
)]
impl ContentSecurity {
    pub fn new() -> Self {
        Self::default()
    }

    /// A policy judging sources at `limits` instead of [`super::SourceLimits::REAL`].
    pub fn with_limits(limits: super::SourceLimits) -> Self {
        Self { limits }
    }

    /// The source sizes this policy judges at.
    pub fn limits(&self) -> super::SourceLimits {
        self.limits
    }

    /// Opt-in: mask every email address in `text`. Callers gate this on
    /// `output.redactEmails` / `OCTOCODE_REDACT_EMAILS`.
    pub(crate) fn redact_emails(&self, text: &str) -> String {
        email_pattern()
            .replace_all(text, "[REDACTED-EMAIL]")
            .into_owned()
    }

    pub(crate) fn sanitize_text<'a>(
        &self,
        content: &'a str,
        file_path: Option<&Path>,
    ) -> SanitizationResult<'a> {
        let path = file_path.map(|path| path.to_string_lossy());
        let native = octocode_engine::security::sanitizer::sanitize_content_within(
            content,
            path.as_deref(),
            self.limits.whole_source,
        );
        self.guard_native(native, content.len(), file_path)
    }

    /// Sanitize one string leaf of a tool result about the file at
    /// `source_path` (`None`: no known file, the pathless output net): its
    /// redacted text, or `None` when nothing in it is redacted. The path only
    /// says what language the text is in, so source code is not mistaken for
    /// configuration; every credential pattern still applies.
    pub(crate) fn sanitize_leaf(&self, content: &str, source_path: Option<&str>) -> Option<String> {
        let redacted = octocode_engine::security::sanitizer::sanitize_leaf(content, source_path)
            .unwrap_or_else(|_| {
                Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
            });
        // The split-key guard of [`Self::guard_native`], over the leaf as
        // the detector left it.
        redact_split_private_key(redacted.as_deref().unwrap_or(content), None).or(redacted)
    }

    /// [`Self::sanitize_leaf`] of a leaf about to be kept: its text when any
    /// read could redact it, whichever file that read names. `source_path` is
    /// the file the leaf is best known to be about.
    pub(crate) fn sanitize_stored_leaf(
        &self,
        content: &str,
        source_path: Option<&str>,
    ) -> Option<String> {
        let redacted =
            octocode_engine::security::sanitizer::sanitize_stored_leaf(content, source_path)
                .unwrap_or_else(|_| {
                    Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
                });
        redact_split_private_key(
            redacted.as_deref().unwrap_or(content),
            source_path.map(Path::new),
        )
        .or(redacted)
    }

    /// A clasify row's own text names no file and leaves for a provider, so it
    /// is judged as kept text is: whatever a read of some file would redact.
    /// The fields of `row` that held any.
    pub(super) fn judge_evidence(
        &self,
        row: &mut serde_json::Map<String, serde_json::Value>,
    ) -> Vec<String> {
        let mut fields = Vec::new();
        for field in ["content", "questions", "context"] {
            let Some(value) = row.get_mut(field) else {
                continue;
            };
            let redacted = super::sanitize_json(value, &mut |text: &str, _: Option<&str>| {
                Ok::<_, std::convert::Infallible>(self.sanitize_stored_leaf(text, None))
            })
            .unwrap_or_else(|never| match never {});
            if redacted {
                fields.push(field.to_owned());
            }
        }
        fields
    }

    /// Text data at an external provider boundary has no trusted source path.
    /// Structured member context and every file-context pattern apply. Image
    /// assets and authentication headers are handled separately by their owner.
    pub(crate) fn sanitize_data(&self, value: &mut serde_json::Value) {
        let _ = super::sanitize_json(value, &mut |text: &str, _: Option<&str>| {
            Ok::<_, std::convert::Infallible>(self.sanitize_stored_leaf(text, None))
        });
    }

    /// `native`, the scan of a text of `len` bytes, with the split-key guard
    /// applied after it. A failed scan hides the whole text.
    fn guard_native<'a>(
        &self,
        native: octocode_engine::error::Result<SanitizationResult<'a>>,
        len: usize,
        file_path: Option<&Path>,
    ) -> SanitizationResult<'a> {
        let native = native.unwrap_or_else(|error| SanitizationResult {
            content: octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.into(),
            has_secrets: true,
            secrets_detected: vec!["sanitizer-failure".to_owned()],
            warnings: vec![error.to_string()],
            edits: octocode_engine::security::Edits::of_pass([(
                0..len,
                octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.len(),
            )]),
        });
        // Guard against a multi-line private key that a bounded read/search
        // window split across its BEGIN/END boundary (the anchored built-in
        // patterns only match a complete block, so a single-boundary window
        // would otherwise leak the body).
        if let Some((guarded, guard)) = split_private_key_hidden(&native.content, file_path) {
            let mut secrets = native.secrets_detected;
            secrets.push(SPLIT_KEY_SECRET.to_owned());
            return SanitizationResult {
                content: guarded.into(),
                has_secrets: true,
                warnings: vec![format!("{} secret(s) redacted", secrets.len())],
                secrets_detected: secrets,
                edits: native.edits.then(&guard),
            };
        }
        native
    }

    pub(crate) fn validate_text_bytes(
        &self,
        bytes: &[u8],
        file_path: Option<&Path>,
        max_bytes: usize,
    ) -> Result<SanitizationResult<'static>, PolicyError> {
        let text = self.decode_source_bytes(bytes, max_bytes)?;
        Ok(self.sanitize_text(&text, file_path).into_owned())
    }

    /// Bounded, non-binary source text for a parser, NOT redacted: redacting
    /// before parsing shifts columns and can break the syntax a pattern
    /// matches. Every string a tool returns is still redacted by the
    /// response stage, as for directory scans that read files raw.
    pub(crate) fn decode_source_bytes<'b>(
        &self,
        bytes: &'b [u8],
        max_bytes: usize,
    ) -> Result<std::borrow::Cow<'b, str>, PolicyError> {
        if bytes.len() > max_bytes {
            return Err(PolicyError::new(
                PolicyErrorCode::InputTooLarge,
                format!("Content exceeds maximum length ({max_bytes} bytes)"),
            ));
        }
        // The engine's one source admission: binary refused, any 8-bit
        // text decoded as the reader tools show it.
        octocode_engine::text::admit(bytes, octocode_engine::text::Decoding::Lenient)
            .map(|decoded| decoded.text)
            .map_err(|_| {
                PolicyError::new(
                    PolicyErrorCode::BinaryContent,
                    "Binary content is not allowed for text output",
                )
            })
    }
}

#[cfg(test)]
pub(crate) mod tests;
