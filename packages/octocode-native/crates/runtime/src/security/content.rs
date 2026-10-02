use std::path::Path;

use octocode_engine::security::types::SanitizationResult;
use serde_json::{Map, Value};

use crate::policy::{PolicyError, PolicyErrorCode};

const MAX_STRING_LENGTH: usize = 10_000;
/// clasify `context.value` is evidence to judge, not a tool parameter; the
/// 4 MiB request cap bounds it, so its subtree gets an evidence-sized limit
/// (self-review drafts and supplied excerpts routinely exceed 10k chars).
/// Secret redaction still applies to every leaf.
const MAX_EVIDENCE_STRING_LENGTH: usize = 1_000_000;
/// Contract `limits.maxInputArrayItems`.
const MAX_ARRAY_LENGTH: usize = crate::tools::id::MAX_INPUT_ARRAY_ITEMS;
const MAX_DEPTH: usize = 20;

/// Secret label recorded when the split-private-key window guard fires.
const SPLIT_KEY_SECRET: &str = "privateKeyFragment";

/// A PEM/OpenSSH/PGP *private-key* boundary line (BEGIN or END). Public
/// certificates (`-----BEGIN CERTIFICATE-----`) are intentionally excluded —
/// they are not secret.
fn is_private_key_boundary(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with("-----")
        && trimmed.contains("PRIVATE KEY")
        && (trimmed.starts_with("-----BEGIN") || trimmed.starts_with("-----END"))
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
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(extension.as_str(), "pem" | "key" | "p8" | "pk8" | "ppk")
        || matches!(
            file_name.as_str(),
            "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519"
        )
}

/// PEM/OpenSSH/PGP *private-key* BEGIN marker (`-----BEGIN … PRIVATE KEY-----`).
fn is_private_key_begin(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("-----BEGIN")
        && trimmed.ends_with("-----")
        && trimmed.contains("PRIVATE KEY")
}

/// PEM/OpenSSH/PGP *private-key* END marker (`-----END … PRIVATE KEY-----`).
fn is_private_key_end(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("-----END") && trimmed.ends_with("-----") && trimmed.contains("PRIVATE KEY")
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
/// assets) and public `CERTIFICATE` blocks are never touched. An unterminated
/// block (a BEGIN with no matching END) is redacted through end-of-file.
pub(crate) fn redact_private_key_blocks(content: &str) -> (String, bool) {
    if !content.contains("PRIVATE KEY") {
        return (content.to_owned(), false);
    }
    let placeholder = format!("[REDACTED-{}]", SPLIT_KEY_SECRET.to_uppercase());
    let mut out = String::with_capacity(content.len());
    let mut inside = false;
    let mut changed = false;
    for (i, line) in content.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if is_private_key_begin(line) {
            inside = true;
        }
        if inside {
            changed = true;
            out.push_str(&placeholder);
        } else {
            out.push_str(line);
        }
        if is_private_key_end(line) {
            inside = false;
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
    format!("[REDACTED-{}]", SPLIT_KEY_SECRET.to_uppercase())
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

/// Key blocks one source may report separately. A source with more is
/// treated as key material from the block that exceeds the cap to its end,
/// so tracker state stays bounded however large the streamed source is.
pub(crate) const MAX_KEY_BLOCK_RANGES: usize = 4096;

/// [`private_key_block_line_ranges`] fed one line at a time, so a caller can
/// stream a source without retaining it. Lines are pushed in order without
/// terminators; [`KeyBlockTracker::finish`] closes an unterminated block at
/// the last pushed line. Past [`MAX_KEY_BLOCK_RANGES`] blocks the last range
/// is open-ended (`u32::MAX`), redacting every later line.
#[derive(Default)]
pub(crate) struct KeyBlockTracker {
    ranges: Vec<(u32, u32)>,
    start: Option<u32>,
    last: u32,
    saturated: bool,
}

impl KeyBlockTracker {
    pub(crate) fn push(&mut self, line: &str) {
        self.last = self.last.saturating_add(1);
        let n = self.last;
        if self.saturated || !line.contains("PRIVATE KEY") {
            return;
        }
        if self.start.is_none() && is_private_key_begin(line) {
            if self.ranges.len() >= MAX_KEY_BLOCK_RANGES {
                self.ranges.push((n, u32::MAX));
                self.saturated = true;
                return;
            }
            self.start = Some(n);
        }
        if is_private_key_end(line)
            && let Some(s) = self.start.take()
        {
            self.ranges.push((s, n));
        }
    }

    pub(crate) fn finish(mut self) -> Vec<(u32, u32)> {
        if let Some(s) = self.start.take() {
            self.ranges.push((s, self.last.max(s)));
        }
        self.ranges
    }
}

/// True when a search match's rendered window (its primary `line` plus the
/// lines its `value` spans, extended both ways to cover context) intersects any
/// private-key block range. Conservative on purpose: it may redact a match
/// adjacent to a block, but only within a file that actually contains a private
/// key.
pub(crate) fn match_window_intersects_key_block(
    line: u32,
    value: &str,
    ranges: &[(u32, u32)],
) -> bool {
    let span = value.lines().count().max(1) as u32;
    let start = line.saturating_sub(span);
    let end = line.saturating_add(span);
    ranges.iter().any(|&(s, e)| start <= e && s <= end)
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
                format!("[REDACTED-{}]", SPLIT_KEY_SECRET.to_uppercase())
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

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ValidationResult {
    pub sanitized_params: Map<String, Value>,
    pub is_valid: bool,
    pub has_secrets: bool,
    pub warnings: Vec<String>,
    /// Dotted paths of the string leaves whose value sanitization rewrote
    /// (`searchText`, `keywords[]`, `outer.key`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_fields: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ContentSecurity;

/// Conservative email shape for opt-in masking of gh outputs. Local-part
/// and domain are both masked; the match is intentionally simple (no
/// quoted local-parts) — commit metadata uses plain addresses.
fn email_pattern() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        #[allow(clippy::expect_used)]
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
    pub fn redact_emails(&self, text: &str) -> String {
        email_pattern()
            .replace_all(text, "[REDACTED-EMAIL]")
            .into_owned()
    }

    pub fn sanitize_text(&self, content: &str, file_path: Option<&Path>) -> SanitizationResult {
        let path = file_path.map(|path| path.to_string_lossy());
        let native = octocode_engine::portable::sanitize_content(content, path.as_deref())
            .unwrap_or_else(|error| SanitizationResult {
                content: "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned(),
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

    pub fn validate_text_bytes(
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
    pub fn decode_source_bytes<'b>(
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
        if bytes.contains(&0) {
            return Err(PolicyError::new(
                PolicyErrorCode::BinaryContent,
                "Binary content is not allowed for text output",
            ));
        }
        Ok(String::from_utf8_lossy(bytes))
    }

    pub fn validate_input_parameters(&self, params: &Value) -> ValidationResult {
        let Some(object) = params.as_object() else {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec!["Invalid parameters: must be an object".to_owned()],
                secret_fields: Vec::new(),
            };
        };
        self.validate_object(object, 0, MAX_STRING_LENGTH, false)
    }

    /// `in_context` is true only for an object held under a `context` key
    /// (clasify's `resources[].context`); its `value` child is evidence and
    /// gets the evidence-sized limit. A `value` key anywhere else keeps the
    /// parameter limit.
    fn validate_object(
        &self,
        object: &Map<String, Value>,
        depth: usize,
        limit: usize,
        in_context: bool,
    ) -> ValidationResult {
        if depth > MAX_DEPTH {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec!["Maximum nesting depth exceeded".to_owned()],
                secret_fields: Vec::new(),
            };
        }
        let mut sanitized = Map::new();
        let mut warnings = Vec::new();
        let mut valid = true;
        let mut has_secrets = false;
        let mut secret_fields = Vec::new();
        for (key, value) in object {
            let limit = if in_context && key == "value" {
                MAX_EVIDENCE_STRING_LENGTH
            } else {
                limit
            };
            if key.trim().is_empty() {
                warnings.push(format!("Invalid parameter key: {key}"));
                valid = false;
                continue;
            }
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                warnings.push(format!("Dangerous parameter key blocked: {key}"));
                valid = false;
                continue;
            }
            match value {
                Value::String(text) => {
                    if text.encode_utf16().count() > limit {
                        warnings.push(format!(
                            "Parameter {key} exceeds maximum length ({limit} characters)"
                        ));
                        valid = false;
                        continue;
                    }
                    let result = self.sanitize_text(text, None);
                    if result.has_secrets {
                        has_secrets = true;
                        secret_fields.push(key.clone());
                        for secret in result.secrets_detected {
                            warnings.push(format!("Secrets detected in {key}: {secret}"));
                        }
                    }
                    sanitized.insert(key.clone(), Value::String(result.content));
                }
                Value::Array(values) => {
                    if values.len() > MAX_ARRAY_LENGTH {
                        warnings.push(format!(
                            "Parameter {key} array exceeds maximum length (100 items)"
                        ));
                        valid = false;
                        continue;
                    }
                    let mut array = Vec::new();
                    let mut item_secrets = false;
                    for item in values {
                        match item {
                            Value::String(text) if text.encode_utf16().count() > limit => {
                                warnings.push(format!(
                                    "Parameter {key}[] exceeds maximum length ({limit} characters)"
                                ));
                                valid = false;
                            }
                            Value::String(text) => {
                                let result = self.sanitize_text(text, None);
                                item_secrets |= result.has_secrets;
                                array.push(Value::String(result.content));
                            }
                            Value::Object(nested) => {
                                let result = self.validate_object(nested, depth + 1, limit, false);
                                has_secrets |= result.has_secrets;
                                secret_fields.extend(
                                    result
                                        .secret_fields
                                        .iter()
                                        .map(|field| format!("{key}[].{field}")),
                                );
                                valid &= result.is_valid;
                                warnings.extend(
                                    result
                                        .warnings
                                        .into_iter()
                                        .map(|warning| format!("{key}[]: {warning}")),
                                );
                                array.push(Value::Object(result.sanitized_params));
                            }
                            // Nested arrays would otherwise be cloned verbatim,
                            // leaving `{"x":[["ghp_…"]]}` unscanned. Recurse so
                            // string leaves at any array depth are sanitized.
                            Value::Array(inner) => {
                                array.push(Value::Array(self.sanitize_nested_array(
                                    inner,
                                    depth + 1,
                                    &mut item_secrets,
                                )));
                            }
                            _ => array.push(item.clone()),
                        }
                    }
                    if item_secrets {
                        has_secrets = true;
                        secret_fields.push(format!("{key}[]"));
                    }
                    sanitized.insert(key.clone(), Value::Array(array));
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1, limit, key == "context");
                    has_secrets |= result.has_secrets;
                    secret_fields.extend(
                        result
                            .secret_fields
                            .iter()
                            .map(|field| format!("{key}.{field}")),
                    );
                    valid &= result.is_valid;
                    warnings.extend(result.warnings.iter().map(|warning| {
                        format!("Invalid nested object in parameter {key}: {warning}")
                    }));
                    sanitized.insert(key.clone(), Value::Object(result.sanitized_params));
                }
                _ => {
                    sanitized.insert(key.clone(), value.clone());
                }
            }
        }
        warnings.sort();
        warnings.dedup();
        secret_fields.sort();
        secret_fields.dedup();
        ValidationResult {
            sanitized_params: sanitized,
            is_valid: valid,
            has_secrets,
            warnings,
            secret_fields,
        }
    }

    /// Recursively sanitize the string leaves of a (possibly deeply) nested
    /// array so no credential survives at any array depth. Objects delegate to
    /// [`validate_object`]; scalars are passed through unchanged. Depth is
    /// bounded by `MAX_DEPTH` to match the object walk and cap recursion.
    fn sanitize_nested_array(
        &self,
        values: &[Value],
        depth: usize,
        has_secrets: &mut bool,
    ) -> Vec<Value> {
        if depth > MAX_DEPTH {
            return values.to_vec();
        }
        values
            .iter()
            .map(|item| match item {
                Value::String(text) => {
                    let result = self.sanitize_text(text, None);
                    *has_secrets |= result.has_secrets;
                    Value::String(result.content)
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1, MAX_STRING_LENGTH, false);
                    *has_secrets |= result.has_secrets;
                    Value::Object(result.sanitized_params)
                }
                Value::Array(inner) => {
                    Value::Array(self.sanitize_nested_array(inner, depth + 1, has_secrets))
                }
                _ => item.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_block_ranges_stay_bounded_and_fail_closed_past_the_cap() {
        let block = "-----BEGIN PRIVATE KEY-----\nQUJDQUJD\n-----END PRIVATE KEY-----\n";
        let blocks = MAX_KEY_BLOCK_RANGES + 50;
        let content = format!("{}tail line\n", block.repeat(blocks));
        let ranges = private_key_block_line_ranges(&content);
        assert_eq!(ranges.len(), MAX_KEY_BLOCK_RANGES + 1);
        assert_eq!(ranges[0], (1, 3));
        let saturated_at = u32::try_from(MAX_KEY_BLOCK_RANGES * 3 + 1).expect("small");
        assert_eq!(ranges.last(), Some(&(saturated_at, u32::MAX)));
        // Every later line, key or not, reads as key material.
        let last_line = u32::try_from(blocks * 3 + 1).expect("small");
        assert!(match_window_intersects_key_block(
            last_line,
            "tail line",
            &ranges
        ));
        // Under the cap, ranges stay exact.
        let exact = private_key_block_line_ranges(&format!("{}tail line\n", block.repeat(3)));
        assert_eq!(exact, vec![(1, 3), (4, 6), (7, 9)]);
        assert!(!match_window_intersects_key_block(11, "tail line", &exact));
    }
    #[test]
    fn split_private_key_guard_targets_only_key_markers() {
        // Body-only base64 with no key marker → guard is a no-op (must not
        // redact ordinary base64 that happens to appear in source).
        assert!(redact_split_private_key("aGVsbG8gd29ybGQ=\nc29tZSBkYXRhIGhlcmU=", None).is_none());
        // A window holding only the BEGIN boundary + body (END is in the next
        // window) → the body must be redacted.
        let out = redact_split_private_key(
            "config header line\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrSt",
            None,
        )
        .expect("split private key must be redacted");
        assert!(!out.contains("MIIEpQIB"), "key body leaked: {out}");
        assert!(
            out.contains("config header line"),
            "non-secret context dropped"
        );
        assert_eq!(out.lines().count(), 3, "line count must be preserved");
        // A public certificate boundary is not a private key → no-op.
        assert!(
            redact_split_private_key("-----BEGIN CERTIFICATE-----\nMIIBpayload", None).is_none()
        );
    }

    #[test]
    fn full_file_key_block_redaction_preserves_lines_and_surrounding_code() {
        let body = "MIIEpQIBAAKCAQEAsplitKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let body2 = "c3BsaXRLZXlCb2R5VHdvQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC";
        let file = format!(
            "fn main() {{}}\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body2}\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
        );
        let (out, changed) = redact_private_key_blocks(&file);
        assert!(changed, "a private-key block must be redacted");
        assert!(
            !out.contains(body) && !out.contains(body2),
            "key body leaked: {out}"
        );
        assert!(out.contains("fn main() {}"), "leading code dropped: {out}");
        assert!(
            out.contains("let done = true;"),
            "trailing code dropped: {out}"
        );
        assert_eq!(
            out.lines().count(),
            file.lines().count(),
            "line count must be preserved for accurate source-line ranges"
        );
        assert!(out.ends_with('\n'), "trailing newline must be preserved");
    }

    #[test]
    fn full_file_redaction_leaves_innocent_and_certificate_content_byte_identical() {
        // Ordinary base64 with no private-key marker — must not be touched.
        let blob = "header\naGVsbG8gd29ybGQgbG9uZyBiYXNlNjQgYmxvYiBoZXJlIQ==\nfooter\n";
        let (out, changed) = redact_private_key_blocks(blob);
        assert!(
            !changed && out == blob,
            "innocent base64 was altered: {out}"
        );
        // A public certificate is not a private key — untouched.
        let cert = "-----BEGIN CERTIFICATE-----\nMIIBpublicCertBody\n-----END CERTIFICATE-----\n";
        let (out, changed) = redact_private_key_blocks(cert);
        assert!(
            !changed && out == cert,
            "certificate was wrongly redacted: {out}"
        );
    }

    #[test]
    fn full_file_redaction_covers_unterminated_block_through_eof() {
        // A BEGIN with no matching END must still redact the trailing body.
        let body = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let file = format!("ok\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n");
        let (out, changed) = redact_private_key_blocks(&file);
        assert!(changed, "unterminated block must redact");
        assert!(!out.contains(body), "unterminated key body leaked: {out}");
        assert!(out.contains("ok"), "pre-block content dropped: {out}");
    }

    #[test]
    fn clasify_evidence_values_may_exceed_the_parameter_length_cap() {
        let policy = ContentSecurity::new();
        let long = "x".repeat(20_000);
        let evidence = serde_json::json!({"resources":[{"context":{"value":{"draft":long}}}]});
        assert!(policy.validate_input_parameters(&evidence).is_valid);
        let parameter = serde_json::json!({"searchText":"y".repeat(20_000)});
        assert!(!policy.validate_input_parameters(&parameter).is_valid);
        // A `value` key outside `context` keeps the parameter cap.
        let stray = serde_json::json!({"filter":{"value":"z".repeat(20_000)}});
        assert!(!policy.validate_input_parameters(&stray).is_valid);
    }

    #[test]
    fn split_key_window_does_not_leak_through_sanitize_text() {
        let policy = ContentSecurity::new();
        let window = "config header line\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrStUvWxYz0123456789AbCdEfGhIjKlMn";
        let result = policy.sanitize_text(window, Some(Path::new("secrets/key.pem")));
        assert!(result.has_secrets, "split key window must be flagged");
        assert!(
            !result.content.contains("MIIEpQIB"),
            "key body leaked from a split search/read window: {}",
            result.content
        );
    }

    #[test]
    fn body_only_pem_window_does_not_leak_through_sanitize_text() {
        let policy = ContentSecurity::new();
        let key_body = "MIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrStUvWxYz0123456789AbCdEfGhIjKlMn";
        let result = policy.sanitize_text(key_body, Some(Path::new("secrets/deploy-key.pem")));
        assert!(result.has_secrets, "body-only PEM window must be flagged");
        assert!(
            !result.content.contains(key_body),
            "key body leaked from a body-only search/read window: {}",
            result.content
        );
        assert_eq!(
            result.content.lines().count(),
            key_body.lines().count(),
            "redaction must preserve source-line mapping"
        );
    }

    #[test]
    fn malformed_utf8_is_lossily_decoded_like_node_reads() {
        let policy = ContentSecurity::new();
        let result = policy
            .validate_text_bytes(&[b'a', 0xff, b'b'], None, 3)
            .expect("security test setup should succeed");
        assert_eq!(result.content, "a�b");
    }

    #[test]
    fn builtin_sanitization_result_is_lossless_through_policy_wrapper() {
        let policy = ContentSecurity::new();
        let corpus = [
            (format!("const token = \"ghp_{}\";", "a".repeat(37)), None),
            (
                "requestId = 550e8400-e29b-41d4-a716-446655440000".to_owned(),
                None,
            ),
            (
                "apiVersion: v1\nkind: Secret\ndata:\n  password: c3VwZXJzZWNyZXQ=".to_owned(),
                Some(Path::new("k8s/secret.yaml")),
            ),
        ];
        for (content, path) in corpus {
            let expected = octocode_engine::portable::sanitize_content(
                &content,
                path.map(|path| path.to_string_lossy()).as_deref(),
            )
            .expect("portable sanitizer");
            let actual = policy.sanitize_text(&content, path);
            assert_eq!(actual.content, expected.content);
            assert_eq!(actual.has_secrets, expected.has_secrets);
            assert_eq!(actual.secrets_detected, expected.secrets_detected);
            assert_eq!(actual.warnings, expected.warnings);
        }
    }
    #[test]
    fn nested_array_string_leaves_are_sanitized() {
        let policy = ContentSecurity::new();
        let token = format!("ghp_{}", "a".repeat(37));
        let result = policy.validate_input_parameters(&serde_json::json!({
            "x": [[token]]
        }));
        assert!(result.has_secrets, "nested-array secret must be detected");
        assert!(
            !serde_json::to_string(&result.sanitized_params)
                .expect("serializable sanitized map")
                .contains("ghp_"),
            "token leaked from a nested array"
        );
    }

    #[test]
    fn parameters_reject_dangerous_keys_and_keep_safe_partial_data() {
        let policy = ContentSecurity::new();
        let result =
            policy.validate_input_parameters(&serde_json::json!({"ok":"x", "prototype": {}}));
        assert!(!result.is_valid);
        assert_eq!(result.sanitized_params["ok"], "x");
        assert!(!result.sanitized_params.contains_key("prototype"));
    }

    #[test]
    fn secret_fields_name_every_rewritten_leaf() {
        let policy = ContentSecurity::new();
        let token = format!("ghp_{}", "a".repeat(36));
        let result = policy.validate_input_parameters(&serde_json::json!({
            "searchText": token,
            "keywords": ["safe", token],
            "outer": {"key": token},
            "grid": [[token]],
            "rows": [{"inner": token}],
            "clean": "needle",
        }));
        assert!(result.has_secrets);
        assert_eq!(
            result.secret_fields,
            [
                "grid[]",
                "keywords[]",
                "outer.key",
                "rows[].inner",
                "searchText"
            ]
        );
        let clean = policy.validate_input_parameters(&serde_json::json!({"searchText": "needle"}));
        assert!(clean.secret_fields.is_empty());
    }

    #[test]
    fn continuation_shaped_values_are_not_credentials() {
        let policy = ContentSecurity::new();
        for value in [
            "lexical-live-v1:845e0d1f0b51e9d91d398dac8fdc9d3a0b88cb954e6434e78a56ff2b753a9a16",
            "824df86de3bc3a3ff0c6201874d42955e8b5b12a0",
            "3bc3a3ff0",
            "MAX_STRING_LENGTH",
            "fn validate_input_parameters",
        ] {
            let result = policy.validate_input_parameters(&serde_json::json!({"field": value}));
            assert!(
                result.secret_fields.is_empty(),
                "{value} flagged as a secret"
            );
        }
    }

    #[test]
    fn frozen_input_bounds_and_nested_secret_projection_match_reference() {
        let policy = ContentSecurity::new();
        let oversized = serde_json::json!({"text": "x".repeat(10_001)});
        let result = policy.validate_input_parameters(&oversized);
        assert!(!result.is_valid);
        assert!(!result.sanitized_params.contains_key("text"));

        let oversized_array = serde_json::json!({"items": (0..101).collect::<Vec<_>>()});
        let result = policy.validate_input_parameters(&oversized_array);
        assert!(!result.is_valid);
        assert!(!result.sanitized_params.contains_key("items"));

        let token = format!("ghp_{}", "a".repeat(37));
        let nested = serde_json::json!({"outer": {"key": token}});
        let result = policy.validate_input_parameters(&nested);
        assert!(result.has_secrets);
        assert!(result.is_valid);
        assert!(
            !serde_json::to_string(&result.sanitized_params)
                .expect("serializable sanitized map")
                .contains("ghp_")
        );
    }
}
