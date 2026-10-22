use std::path::Path;

use octocode_engine::security::types::SanitizationResult;
use serde_json::{Map, Value};

use crate::policy::{PolicyError, PolicyErrorCode};

const MAX_STRING_LENGTH: usize = 10_000;
/// Contract `limits.maxInputArrayItems`.
const MAX_ARRAY_LENGTH: usize = crate::id::MAX_INPUT_ARRAY_ITEMS;
const MAX_DEPTH: usize = 20;

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

/// Key blocks one source may report separately. A source with more is
/// treated as key material from the block that exceeds the cap to its end,
/// so tracker state stays bounded however large the streamed source is.
pub(crate) const MAX_KEY_BLOCK_RANGES: usize = 4096;

/// What a line inside a private-key block looks like, after stripping the
/// quoting and concatenation a key pasted into source carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineShape {
    /// Empty, or only quotes and punctuation.
    Blank,
    /// Base64 (`A-Za-z0-9+/=`), optionally quoted and ended with an escaped
    /// newline, `+`, `,` or `;`: `"MIIE…\n" +`.
    Body,
    /// Anything else.
    Other,
}

/// Incremental [`LineShape`] classifier: a line is leading decoration, a
/// base64 core, then trailing decoration. Fed byte pieces so a streamed
/// source never buffers a whole line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineShapeScan {
    stage: ShapeStage,
    core: bool,
    escape: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeStage {
    Lead,
    Core,
    Trail,
    Bad,
}

impl Default for LineShapeScan {
    fn default() -> Self {
        Self {
            stage: ShapeStage::Lead,
            core: false,
            escape: false,
        }
    }
}

impl LineShapeScan {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.stage == ShapeStage::Bad {
                return;
            }
            if self.escape {
                // Only an escaped newline, return or tab is decoration.
                self.escape = false;
                if !matches!(byte, b'n' | b'r' | b't') {
                    self.stage = ShapeStage::Bad;
                }
                continue;
            }
            let base64 = byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=');
            let decoration = byte.is_ascii_whitespace()
                || matches!(
                    byte,
                    b'"' | b'\'' | b'`' | b',' | b';' | b'.' | b'(' | b')' | b'[' | b']' | b'}'
                );
            self.stage = match (self.stage, byte) {
                (stage, b'\\') => {
                    self.escape = true;
                    if stage == ShapeStage::Core {
                        ShapeStage::Trail
                    } else {
                        stage
                    }
                }
                (ShapeStage::Lead, _) if base64 => {
                    self.core = true;
                    ShapeStage::Core
                }
                (ShapeStage::Lead, _) if decoration => ShapeStage::Lead,
                (ShapeStage::Core, _) if base64 => ShapeStage::Core,
                (ShapeStage::Core | ShapeStage::Trail, _) if decoration => ShapeStage::Trail,
                (ShapeStage::Trail, b'+') => ShapeStage::Trail,
                _ => ShapeStage::Bad,
            };
        }
    }

    pub(crate) fn finish(self) -> LineShape {
        match self.stage {
            ShapeStage::Bad => LineShape::Other,
            _ if self.core => LineShape::Body,
            _ => LineShape::Blank,
        }
    }
}

/// `Name: value` armor header (`Proc-Type: 4,ENCRYPTED`, `Version: …`),
/// judged from a line's leading bytes.
pub(crate) fn is_key_header_prefix(head: &[u8]) -> bool {
    let head = head.trim_ascii_start();
    let name = head
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'-')
        .count();
    name > 0 && head[0].is_ascii_alphabetic() && head.get(name) == Some(&b':')
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BlockPhase {
    #[default]
    Outside,
    /// After the BEGIN marker, before the first body line: armor headers and
    /// blank lines are allowed.
    Header,
    Body,
}

/// Per-line private-key block state shared by the whole-file redaction, the
/// range tracker and the streamed read. A block opens at a BEGIN marker and
/// ends at its END marker (inclusive) or, with no END, before the first line
/// that is not part of a key: armor headers and blank lines straight after
/// BEGIN, then base64 body lines. Everything inside is key material; a
/// literal marker string in ordinary code therefore hides only its own line.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyBlockState {
    phase: BlockPhase,
}

/// How a line reads once it is read inside an open block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockLine {
    Header,
    Shape(LineShape),
}

impl KeyBlockState {
    /// Advance over one line; true when it is inside a block (key material).
    /// `inside` is only called while a block is open and the line is not an
    /// END marker.
    pub(crate) fn advance(
        &mut self,
        begins: bool,
        ends: bool,
        inside: impl FnOnce() -> BlockLine,
    ) -> bool {
        if self.phase != BlockPhase::Outside {
            if ends {
                self.phase = BlockPhase::Outside;
                return true;
            }
            match (self.phase, inside()) {
                (_, BlockLine::Shape(LineShape::Body)) => {
                    self.phase = BlockPhase::Body;
                    return true;
                }
                (BlockPhase::Header, BlockLine::Header | BlockLine::Shape(LineShape::Blank)) => {
                    return true;
                }
                _ => self.phase = BlockPhase::Outside,
            }
        }
        if begins {
            if !ends {
                self.phase = BlockPhase::Header;
            }
            return true;
        }
        false
    }

    /// True while a block is open: the next line may still be key material.
    pub(crate) fn is_open(&self) -> bool {
        self.phase != BlockPhase::Outside
    }

    pub(crate) fn push_line(&mut self, line: &str) -> bool {
        self.advance(is_private_key_begin(line), is_private_key_end(line), || {
            let mut scan = LineShapeScan::default();
            scan.feed(line.as_bytes());
            match scan.finish() {
                LineShape::Other if is_key_header_prefix(line.as_bytes()) => BlockLine::Header,
                shape => BlockLine::Shape(shape),
            }
        })
    }
}

/// [`private_key_block_line_ranges`] fed one line at a time, so a caller can
/// stream a source without retaining it. Lines are pushed in order without
/// terminators; [`KeyBlockTracker::finish`] closes an open block at its last
/// key line. Past [`MAX_KEY_BLOCK_RANGES`] blocks the last range is
/// open-ended (`u32::MAX`), redacting every later line.
#[derive(Default)]
pub(crate) struct KeyBlockTracker {
    ranges: Vec<(u32, u32)>,
    block: KeyBlockState,
    start: Option<u32>,
    last_key: u32,
    last: u32,
    saturated: bool,
}

impl KeyBlockTracker {
    pub(crate) fn push(&mut self, line: &str) {
        self.last = self.last.saturating_add(1);
        if self.saturated {
            return;
        }
        let n = self.last;
        if self.block.push_line(line) {
            if self.start.is_none() {
                if self.ranges.len() >= MAX_KEY_BLOCK_RANGES {
                    self.ranges.push((n, u32::MAX));
                    self.saturated = true;
                    return;
                }
                self.start = Some(n);
            }
            self.last_key = n;
            if !self.block.is_open()
                && let Some(start) = self.start.take()
            {
                self.ranges.push((start, n));
            }
        } else if let Some(start) = self.start.take() {
            self.ranges.push((start, self.last_key));
        }
    }

    pub(crate) fn finish(mut self) -> Vec<(u32, u32)> {
        if let Some(start) = self.start.take() {
            self.ranges.push((start, self.last_key));
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

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ValidationResult {
    pub(crate) sanitized_params: Map<String, Value>,
    pub(crate) is_valid: bool,
    pub(crate) has_secrets: bool,
    pub warnings: Vec<String>,
    /// Dotted paths of the string leaves whose value sanitization rewrote
    /// (`matchString`, `keywords[]`, `outer.key`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) secret_fields: Vec<String>,
}

impl ValidationResult {
    /// Fold a nested object's verdict into this one, naming its secret
    /// fields with `field` and its warnings with `warning`; returns its
    /// sanitized object.
    fn absorb(
        &mut self,
        nested: ValidationResult,
        field: impl Fn(&str) -> String,
        warning: impl Fn(&str) -> String,
    ) -> Map<String, Value> {
        self.has_secrets |= nested.has_secrets;
        self.is_valid &= nested.is_valid;
        self.secret_fields
            .extend(nested.secret_fields.iter().map(|name| field(name)));
        self.warnings
            .extend(nested.warnings.iter().map(|text| warning(text)));
        nested.sanitized_params
    }
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
        if octocode_engine::text::sniff(bytes) == octocode_engine::text::BinaryVerdict::Binary {
            return Err(PolicyError::new(
                PolicyErrorCode::BinaryContent,
                "Binary content is not allowed for text output",
            ));
        }
        Ok(String::from_utf8_lossy(bytes))
    }

    pub(crate) fn validate_input_parameters(&self, params: &Value) -> ValidationResult {
        self.validate_parameters(params, None)
    }

    /// Allow the caller's declared serialized-state bound only for the root
    /// state. Other fields keep their ordinary limits and every leaf is scanned.
    pub(crate) fn validate_state_parameters(
        &self,
        params: &Value,
        state_limit: usize,
    ) -> ValidationResult {
        self.validate_parameters(params, Some(state_limit))
    }

    fn validate_parameters(&self, params: &Value, state_limit: Option<usize>) -> ValidationResult {
        let Some(object) = params.as_object() else {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec!["Invalid parameters: must be an object".to_owned()],
                secret_fields: Vec::new(),
            };
        };
        if let (Some(limit), Some(state)) = (state_limit, object.get("state"))
            && state.to_string().chars().count() > limit
        {
            return ValidationResult {
                sanitized_params: Map::new(),
                is_valid: false,
                has_secrets: false,
                warnings: vec![format!(
                    "Parameter state exceeds maximum serialized length ({limit} characters)"
                )],
                secret_fields: Vec::new(),
            };
        }
        self.validate_object(object, 0, MAX_STRING_LENGTH, state_limit)
    }

    /// A state allowance applies at the declared root only; its already bounded
    /// subtree inherits it without granting it to unrelated nested `state` keys.
    fn validate_object(
        &self,
        object: &Map<String, Value>,
        depth: usize,
        limit: usize,
        state_limit: Option<usize>,
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
        let mut out = ValidationResult {
            sanitized_params: Map::new(),
            is_valid: true,
            has_secrets: false,
            warnings: Vec::new(),
            secret_fields: Vec::new(),
        };
        for (key, value) in object {
            // Keys also become provider input and diagnostic field paths.
            // Reject credentials here; renaming keys would change the data.
            if self.sanitize_text(key, None).has_secrets {
                out.warnings.push(
                    "A parameter key contains a credential; remove it before retrying".into(),
                );
                out.is_valid = false;
                out.has_secrets = true;
                continue;
            }
            // Ordinary parameter limits count UTF-16 units. The complete state
            // was bounded above in Unicode scalar values; allow both UTF-16
            // units of an astral character without rejecting valid evidence.
            let limit = if key == "state" {
                state_limit.map_or(limit, |bound| bound.saturating_mul(2))
            } else {
                limit
            };
            if key.trim().is_empty() {
                out.warnings.push(format!("Invalid parameter key: {key}"));
                out.is_valid = false;
                continue;
            }
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                out.warnings
                    .push(format!("Dangerous parameter key blocked: {key}"));
                out.is_valid = false;
                continue;
            }
            match value {
                Value::String(text) => {
                    if text.encode_utf16().count() > limit {
                        out.warnings.push(format!(
                            "Parameter {key} exceeds maximum length ({limit} characters)"
                        ));
                        out.is_valid = false;
                        continue;
                    }
                    let result = self.sanitize_text(text, None);
                    if result.has_secrets {
                        out.has_secrets = true;
                        out.secret_fields.push(key.clone());
                        for secret in result.secrets_detected {
                            out.warnings
                                .push(format!("Secrets detected in {key}: {secret}"));
                        }
                    }
                    out.sanitized_params
                        .insert(key.clone(), Value::String(result.content));
                }
                Value::Array(values) => {
                    if let Some(array) = self.validate_array(key, values, depth, limit, &mut out) {
                        out.sanitized_params
                            .insert(key.clone(), Value::Array(array));
                    }
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1, limit, None);
                    let nested = out.absorb(
                        result,
                        |field| format!("{key}.{field}"),
                        |warning| format!("Invalid nested object in parameter {key}: {warning}"),
                    );
                    out.sanitized_params
                        .insert(key.clone(), Value::Object(nested));
                }
                _ => {
                    out.sanitized_params.insert(key.clone(), value.clone());
                }
            }
        }
        out.warnings.sort();
        out.warnings.dedup();
        out.secret_fields.sort();
        out.secret_fields.dedup();
        out
    }

    /// The sanitized items of parameter `key`'s array, or `None` (with a
    /// warning in `out`) when it holds too many items.
    fn validate_array(
        &self,
        key: &str,
        values: &[Value],
        depth: usize,
        limit: usize,
        out: &mut ValidationResult,
    ) -> Option<Vec<Value>> {
        if depth > MAX_DEPTH {
            out.warnings
                .push("Maximum nesting depth exceeded".to_owned());
            out.is_valid = false;
            return None;
        }
        if values.len() > MAX_ARRAY_LENGTH {
            out.warnings.push(format!(
                "Parameter {key} array exceeds maximum length (100 items)"
            ));
            out.is_valid = false;
            return None;
        }
        let mut array = Vec::new();
        let mut item_secrets = false;
        for item in values {
            match item {
                Value::String(text) if text.encode_utf16().count() > limit => {
                    out.warnings.push(format!(
                        "Parameter {key}[] exceeds maximum length ({limit} characters)"
                    ));
                    out.is_valid = false;
                }
                Value::String(text) => {
                    let result = self.sanitize_text(text, None);
                    item_secrets |= result.has_secrets;
                    array.push(Value::String(result.content));
                }
                Value::Object(nested) => {
                    let result = self.validate_object(nested, depth + 1, limit, None);
                    let nested = out.absorb(
                        result,
                        |field| format!("{key}[].{field}"),
                        |warning| format!("{key}[]: {warning}"),
                    );
                    array.push(Value::Object(nested));
                }
                // Nested arrays would otherwise be cloned verbatim,
                // leaving `{"x":[["ghp_…"]]}` unscanned. Recurse so
                // string leaves at any array depth are sanitized.
                Value::Array(inner) => {
                    if let Some(nested) = self.validate_array(key, inner, depth + 1, limit, out) {
                        array.push(Value::Array(nested));
                    }
                }
                _ => array.push(item.clone()),
            }
        }
        if item_secrets {
            out.has_secrets = true;
            out.secret_fields.push(format!("{key}[]"));
        }
        Some(array)
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
    fn markers_inside_string_literals_open_and_close_a_key_block() {
        // A key pasted into source as a template string or triple-quoted
        // string: the marker follows code on its line and the END marker is
        // followed by the closing quote.
        let body = "MIIEpQIBAAKCAQEAinlineKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for (open, close) in [
            (
                "const key = `-----BEGIN RSA PRIVATE KEY-----",
                "-----END RSA PRIVATE KEY-----`;",
            ),
            (
                "KEY = \"\"\"-----BEGIN PRIVATE KEY-----",
                "-----END PRIVATE KEY-----\"\"\"",
            ),
        ] {
            let file = format!("use x;\n{open}\n{body}\n{close}\nlet done = true;\n");
            let (out, changed) = redact_private_key_blocks(&file);
            assert!(changed, "inline marker not recognised: {open}");
            assert!(!out.contains(body), "key body leaked: {out}");
            assert!(
                out.contains("let done = true;"),
                "trailing code dropped: {out}"
            );
            assert_eq!(out.lines().count(), file.lines().count());
        }
        assert!(is_private_key_boundary(
            "  key = `-----BEGIN OPENSSH PRIVATE KEY-----`"
        ));
        assert!(!is_private_key_boundary("-----BEGIN CERTIFICATE-----"));
        assert!(!is_private_key_boundary(
            "PRIVATE KEY----- is how a marker ends"
        ));
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
    fn unterminated_marker_redacts_only_the_base64_run_after_it() {
        // A literal marker string in ordinary source opens a block that never
        // closes: only the marker line is key material, not the code after it.
        let literal = "let a = 1;\nconst PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet b = compute(a);\nfn main() {}\n";
        assert_eq!(private_key_block_line_ranges(literal), vec![(2, 2)]);
        let (out, changed) = redact_private_key_blocks(literal);
        assert!(changed);
        assert!(out.contains("let b = compute(a);"), "{out}");
        assert!(out.contains("fn main() {}"), "{out}");
        assert!(!out.contains("BEGIN RSA"), "{out}");
        assert_eq!(out.lines().count(), literal.lines().count());
        // A BEGIN literal and a far-away END literal do not make the code
        // between them a key body.
        let far = "a\n\"-----BEGIN X PRIVATE KEY-----\"\nlet x = 1;\nlet y = 2;\n\"-----END X PRIVATE KEY-----\"\n";
        assert_eq!(private_key_block_line_ranges(far), vec![(2, 2)]);
    }

    #[test]
    fn unterminated_key_body_stays_redacted_through_its_base64_run() {
        let body = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let file = format!(
            "ok\n-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,0123ABCD\n\n{body}\n{body}\nQUJD==\nfn main() {{}}\n"
        );
        assert_eq!(private_key_block_line_ranges(&file), vec![(2, 8)]);
        let (out, changed) = redact_private_key_blocks(&file);
        assert!(changed);
        assert!(!out.contains(body) && !out.contains("QUJD"), "{out}");
        assert!(!out.contains("DEK-Info"), "{out}");
        assert!(out.contains("fn main() {}"), "{out}");
        assert_eq!(out.lines().count(), file.lines().count());
    }

    #[test]
    fn quoted_and_concatenated_key_bodies_stay_inside_the_block() {
        let body = "MIIEpQIBAAKCAQEAquotedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let terminated = format!(
            "x\nconst k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"{body}\\n\" +\n  \"-----END RSA PRIVATE KEY-----\";\nlet done = true;\n"
        );
        assert_eq!(private_key_block_line_ranges(&terminated), vec![(2, 5)]);
        let unterminated = format!(
            "x\nconst k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"{body}\\n\";\nlet done = true;\n"
        );
        assert_eq!(private_key_block_line_ranges(&unterminated), vec![(2, 4)]);
        let (out, _) = redact_private_key_blocks(&unterminated);
        assert!(!out.contains(body), "{out}");
        assert!(out.contains("let done = true;"), "{out}");
    }

    #[test]
    fn key_block_tracker_streams_the_same_ranges_as_the_whole_file_scan() {
        let body = "MIIEpQIBAAKCAQEAstreamedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for file in [
            format!("a\n-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\nb\n"),
            format!("a\n\"-----BEGIN PRIVATE KEY-----\"\nlet b = 2;\n{body}\n"),
            format!(
                "a\n-----BEGIN PRIVATE KEY-----\n{body}\nlet b = 2;\n-----END PRIVATE KEY-----\n"
            ),
        ] {
            let mut tracker = KeyBlockTracker::default();
            for line in file.lines() {
                tracker.push(line);
            }
            assert_eq!(tracker.finish(), private_key_block_line_ranges(&file));
        }
    }

    #[test]
    fn credential_keys_are_rejected_without_echoing_or_renaming_them() {
        let token = format!("ghp_{}", "a".repeat(36));
        let params = serde_json::json!({"state":[{"nested":{token.clone():"ordinary value"}}]});
        let checked = ContentSecurity::new().validate_state_parameters(&params, 32_000);
        assert!(!checked.is_valid);
        assert!(checked.has_secrets);
        assert!(!format!("{checked:?}").contains(&token));
    }

    #[test]
    fn only_declared_state_inherits_the_current_evidence_limit() {
        let policy = ContentSecurity::new();
        let evidence = serde_json::json!({"state":{"draft":"x".repeat(20_000)}});
        assert!(policy.validate_state_parameters(&evidence, 32_000).is_valid);
        assert!(!policy.validate_input_parameters(&evidence).is_valid);
        for other in [
            serde_json::json!({"matchString":"x".repeat(20_000)}),
            serde_json::json!({"resources":[{"value":"x".repeat(20_000)}]}),
            serde_json::json!({"filter":{"state":"x".repeat(20_000)}}),
        ] {
            assert!(!policy.validate_state_parameters(&other, 32_000).is_valid);
        }
    }

    #[test]
    fn state_limit_counts_complete_unicode_evidence_and_nested_arrays() {
        let policy = ContentSecurity::new();
        let unicode = serde_json::json!({"state":[[{"text":"😀".repeat(20_000)}]]});
        assert!(policy.validate_state_parameters(&unicode, 32_000).is_valid);
        let over = serde_json::json!({"state":["a".repeat(17_000),"b".repeat(17_000)]});
        assert!(!policy.validate_state_parameters(&over, 32_000).is_valid);
        let long = serde_json::json!({"state":"x".repeat(40_000)});
        assert!(!policy.validate_state_parameters(&long, 32_000).is_valid);
        assert!(policy.validate_state_parameters(&long, 256_000).is_valid);
        let token = format!("ghp_{}", "a".repeat(37));
        let value =
            serde_json::json!({"state":[[{"text":format!("{} {token}","a".repeat(20_000))}]]});
        let checked = policy.validate_state_parameters(&value, 32_000);
        assert!(checked.is_valid && checked.has_secrets);
        assert!(
            !serde_json::to_string(&checked.sanitized_params)
                .unwrap()
                .contains(&token)
        );
    }

    #[test]
    fn nested_arrays_cannot_bypass_parameter_bounds() {
        let policy = ContentSecurity::new();
        assert!(
            !policy
                .validate_input_parameters(&serde_json::json!({"x":[["x".repeat(10_001)]]}))
                .is_valid
        );
        assert!(
            !policy
                .validate_input_parameters(
                    &serde_json::json!({"x":[[(0..101).collect::<Vec<_>>()]]})
                )
                .is_valid
        );
        let mut deep = serde_json::json!("safe");
        for _ in 0..25 {
            deep = serde_json::json!([deep]);
        }
        assert!(
            !policy
                .validate_input_parameters(&serde_json::json!({"x":deep}))
                .is_valid
        );
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
            let expected = octocode_engine::security::sanitizer::sanitize_content(
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
            "matchString": token,
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
                "matchString",
                "outer.key",
                "rows[].inner"
            ]
        );
        let clean = policy.validate_input_parameters(&serde_json::json!({"matchString": "needle"}));
        assert!(clean.secret_fields.is_empty());
    }

    #[test]
    fn continuation_shaped_values_are_not_credentials() {
        let policy = ContentSecurity::new();
        for value in [
            "lexical-live-v2:845e0d1f0b51e9d91d398dac8fdc9d3a0b88cb954e6434e78a56ff2b753a9a16",
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
