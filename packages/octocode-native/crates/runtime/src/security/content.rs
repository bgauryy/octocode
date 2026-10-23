mod key_blocks;
pub(crate) use key_blocks::{
    BlockLine, KeyBlockState, KeyBlockTracker, LineShape, LineShapeScan, is_key_header_prefix,
    match_window_intersects_key_block,
};
use std::path::Path;

use crate::policy::{PolicyError, PolicyErrorCode};
use octocode_engine::security::types::SanitizationResult;

/// Secret label recorded when the split-private-key window guard fires.
const SPLIT_KEY_SECRET: &str = "privateKeyFragment";

/// A PEM/OpenSSH/PGP *private-key* boundary line (BEGIN or END). Public
/// certificates (`-----BEGIN CERTIFICATE-----`) are intentionally excluded —
/// they are not secret.
fn is_private_key_boundary(line: &str) -> bool {
    is_private_key_begin(line) || is_private_key_end(line)
}

/// `<prefix> … PRIVATE KEY-----` anywhere in the line: a marker inside a
/// string literal (`` const key = `-----BEGIN RSA PRIVATE KEY-----` ``) opens
/// a block as much as one that starts the line.
fn has_private_key_marker(line: &str, prefix: &str) -> bool {
    line.match_indices(prefix).any(|(start, _)| {
        let rest = &line[start + prefix.len()..];
        rest.find("-----")
            .is_some_and(|end| rest[..end].contains("PRIVATE KEY"))
    })
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

/// PEM/OpenSSH/PGP *private-key* BEGIN marker (`-----BEGIN … PRIVATE KEY-----`).
fn is_private_key_begin(line: &str) -> bool {
    has_private_key_marker(line, "-----BEGIN")
}

/// PEM/OpenSSH/PGP *private-key* END marker (`-----END … PRIVATE KEY-----`).
fn is_private_key_end(line: &str) -> bool {
    has_private_key_marker(line, "-----END")
}

/// Full-file guard: redact every line of each PEM/OpenSSH/PGP *private-key* block
/// (from its BEGIN marker through its END marker, inclusive), line-for-line.
///
/// This runs on the **whole file** before any bounded read/search window is cut,
/// which is the gap the anchored built-in patterns leave: those match only a
/// complete BEGIN…END block in a single view, so a window that selects interior
/// body lines (no marker) leaks the key. Scanning the full file lets us poison
/// exactly the block's source lines so any later window of them is already safe.
///
/// Line count and the trailing newline are preserved so the tool's reported
/// source-line ranges stay accurate. Content with no `PRIVATE KEY` marker is
/// returned byte-identical — ordinary base64 (config blobs, hashes, minified
/// assets) and public `CERTIFICATE` blocks are never touched. A block ends at
/// its END marker or, when it has none, at the first line that is not
/// key-shaped (see [`KeyBlockState`]): code after a literal marker string stays
/// readable.
pub(crate) fn redact_private_key_blocks(content: &str) -> (String, bool) {
    if !content.contains("PRIVATE KEY") {
        return (content.to_owned(), false);
    }
    let placeholder = key_fragment_placeholder();
    let mut out = String::with_capacity(content.len());
    let mut block = KeyBlockState::default();
    let mut changed = false;
    for (i, line) in content.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if block.push_line(line) {
            changed = true;
            out.push_str(&placeholder);
        } else {
            out.push_str(line);
        }
    }
    if !changed {
        return (content.to_owned(), false);
    }
    if content.ends_with('\n') {
        out.push('\n');
    }
    (out, true)
}

/// Placeholder emitted for a redacted private-key fragment. Shared by the
/// full-file block guard and the localSearch match guard so redactions read
/// identically wherever a key body is stripped.
pub(crate) fn key_fragment_placeholder() -> String {
    octocode_engine::security::redaction_placeholder(SPLIT_KEY_SECRET)
}

/// Cheap trigger: does this search-match snippet contain any line shaped like a
/// private-key boundary or base64 body? Used by localSearch to decide whether a
/// file is worth a full-file scan. It has **no false negatives** for real key
/// body lines (a base64 body line always satisfies [`is_key_body_line`]), so a
/// match that could expose key material always triggers the scan; innocent
/// base64 merely triggers a scan that finds no block and redacts nothing.
pub(crate) fn snippet_may_hold_key_material(snippet: &str) -> bool {
    snippet
        .lines()
        .any(|line| is_private_key_boundary(line) || is_key_body_line(line))
}

/// 1-based inclusive line ranges of PEM/OpenSSH/PGP private-key blocks in
/// `content`. Empty when none. Lets localSearch redact only the matches whose
/// window intersects a real key block. An unterminated block runs to EOF.
pub(crate) fn private_key_block_line_ranges(content: &str) -> Vec<(u32, u32)> {
    if !content.contains("PRIVATE KEY") {
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
/// Returns `None` when no boundary marker is present, so `fullContent` reads
/// (already collapsed to one marker) and non-key content stay byte-identical.
/// Redaction is line-for-line, preserving the leaf's line count so the tool's
/// reported line ranges remain accurate.
fn redact_split_private_key(text: &str, file_path: Option<&Path>) -> Option<String> {
    let has_boundary = text.lines().any(is_private_key_boundary);
    if !has_boundary && !is_private_key_path(file_path) {
        return None;
    }
    let mut changed = false;
    let redacted: Vec<String> = text
        .lines()
        .map(|line| {
            if is_private_key_boundary(line) || is_key_body_line(line) {
                changed = true;
                key_fragment_placeholder()
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !changed {
        return None;
    }
    let mut out = redacted.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[derive(Clone, Debug, Default)]
pub struct ContentSecurity;

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

impl ContentSecurity {
    pub fn new() -> Self {
        Self
    }

    /// Opt-in: mask every email address in `text`. Callers gate this on
    /// `output.redactEmails` / `OCTOCODE_REDACT_EMAILS`.
    pub(crate) fn redact_emails(&self, text: &str) -> String {
        email_pattern()
            .replace_all(text, "[REDACTED-EMAIL]")
            .into_owned()
    }

    pub(crate) fn sanitize_text(
        &self,
        content: &str,
        file_path: Option<&Path>,
    ) -> SanitizationResult {
        let path = file_path.map(|path| path.to_string_lossy());
        let native =
            octocode_engine::security::sanitizer::sanitize_content(content, path.as_deref());
        self.guard_native(native, file_path)
    }

    /// Sanitize one string leaf of a tool result about the file at
    /// `source_path` (`None`: no known file, the pathless output net). The
    /// path only says what language the text is in, so source code is not
    /// mistaken for configuration; every credential pattern still applies.
    pub(crate) fn sanitize_leaf(
        &self,
        content: &str,
        source_path: Option<&str>,
    ) -> SanitizationResult {
        let native = match source_path {
            Some(path) => octocode_engine::security::sanitizer::sanitize_leaf(content, path),
            None => octocode_engine::security::sanitizer::sanitize_content(content, None),
        };
        self.guard_native(native, None)
    }

    fn guard_native(
        &self,
        native: octocode_engine::error::Result<SanitizationResult>,
        file_path: Option<&Path>,
    ) -> SanitizationResult {
        let native = native.unwrap_or_else(|error| SanitizationResult {
            content: octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned(),
            has_secrets: true,
            secrets_detected: vec!["sanitizer-failure".to_owned()],
            warnings: vec![error.to_string()],
        });
        // Guard against a multi-line private key that a bounded read/search
        // window split across its BEGIN/END boundary (the anchored built-in
        // patterns only match a complete block, so a single-boundary window
        // would otherwise leak the body).
        if let Some(guarded) = redact_split_private_key(&native.content, file_path) {
            let mut secrets = native.secrets_detected;
            secrets.push(SPLIT_KEY_SECRET.to_owned());
            return SanitizationResult {
                content: guarded,
                has_secrets: true,
                warnings: vec![format!("{} secret(s) redacted", secrets.len())],
                secrets_detected: secrets,
            };
        }
        native
    }

    pub(crate) fn validate_text_bytes(
        &self,
        bytes: &[u8],
        file_path: Option<&Path>,
        max_bytes: usize,
    ) -> Result<SanitizationResult, PolicyError> {
        let text = self.decode_source_bytes(bytes, max_bytes)?;
        Ok(self.sanitize_text(&text, file_path))
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
mod tests;
