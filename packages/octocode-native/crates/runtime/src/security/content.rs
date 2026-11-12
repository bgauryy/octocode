mod key_blocks;
mod key_markers;
pub(crate) use key_blocks::{
    KEY_LINE_HEAD_BYTES, KeyBlockState, KeyBlockTracker, LineShapeScan, hide_key_line,
    key_line_bytes, key_line_lead, match_window_intersects_key_block,
};
use key_blocks::{holds_body_run, opens_with_body_run};
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

/// Full-file guard: redact the key text of each *private-key* block in any
/// format, line-for-line: every body line, and on its BEGIN and END armor
/// lines only key text written beside the armor ([`key_line_bytes`]). The
/// armor names the block and holds none of the key, so it reads as written.
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
        block.push_line(line).then(|| hidden_key(line)).flatten()
    }) {
        Some((text, edits)) => (text.into(), edits),
        None => (content.into(), octocode_engine::security::Edits::default()),
    }
}

/// [`private_key_blocks_hidden`] for a leaf, a piece of a text (a match, a
/// hover, a window): every line of a block its END closes is hidden as a
/// read hides it, one placeholder per line; in a block the piece never
/// closes, only a line shaped as key text is, so a word the piece shows past
/// a body it cut reads as written (as [`split_private_key_hidden`] reads it).
fn leaf_key_blocks_hidden(
    content: &str,
) -> (std::borrow::Cow<'_, str>, octocode_engine::security::Edits) {
    if !may_hold_key_marker(content) {
        return (content.into(), octocode_engine::security::Edits::default());
    }
    let last_end = content
        .lines()
        .enumerate()
        .filter(|(_, line)| line_markers(line.as_bytes()).ends)
        .map(|(index, _)| index)
        .last();
    let mut block = KeyBlockState::default();
    let mut at = 0_usize;
    match hide_lines(content, |line| {
        let closed = last_end.is_some_and(|end| at <= end);
        at += 1;
        let in_block = block.push_line(line);
        let keyed = closed
            || is_private_key_boundary(line)
            || is_key_body_line(line)
            || opens_with_body_run(line)
            || holds_body_run(line);
        (in_block && keyed).then(|| hidden_key(line)).flatten()
    }) {
        Some((text, edits)) => (text.into(), edits),
        None => (content.into(), octocode_engine::security::Edits::default()),
    }
}

/// The key's bytes of a line of a key block, and what replaces them; `None`
/// for a line that holds no key text (an armor line alone).
fn hidden_key(line: &str) -> Option<(std::ops::Range<usize>, String)> {
    Some((key_line_bytes(line)?, key_fragment_placeholder()))
}

/// `text` rewritten line by line as [`str::lines`] splits it, joined by `\n`
/// with its final line break kept, and what that replaced by byte position: a
/// line `hide` returns bytes of and a text for has those bytes replaced by
/// the text, the rest of it staying the source's own, and a `\r` the split
/// drops goes alone. `None` when `hide` changes no line.
fn hide_lines(
    text: &str,
    mut hide: impl FnMut(&str) -> Option<(std::ops::Range<usize>, String)>,
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
        if let Some((bytes, hidden)) = hide(line) {
            hid_any = true;
            replaced.push((start + bytes.start..start + bytes.end, hidden.len()));
            out.push_str(line.get(..bytes.start).unwrap_or_default());
            out.push_str(&hidden);
            out.push_str(line.get(bytes.end..).unwrap_or_default());
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

/// Whether `text`, a string of a request, holds the body of a private key:
/// a line a block hides ([`KeyBlockState`]) that is no armor line itself, or
/// a body run on the armor line (a block written on one line). The armor
/// line alone is no key text.
pub(crate) fn holds_private_key_body(text: &str) -> bool {
    if !may_hold_key_marker(text) {
        return false;
    }
    let mut block = KeyBlockState::default();
    text.lines().any(|line| {
        let hidden = block.push_line(line);
        let markers = line_markers(line.as_bytes());
        hidden && (!markers.any() || holds_body_run(line))
    })
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
/// position. The blocks of `text` are tracked as a whole file's are
/// ([`KeyBlockState`]), and inside one a line shaped as key text (a body
/// line, or one holding a body's run) is hidden; a window cut inside one hides the body it holds:
/// before its first edge when that is an END (the block opened above the
/// window), or everywhere in a key file's window that holds no edge. An
/// armor line shows its armor; only key text beside it is hidden
/// ([`key_line_bytes`]). A line outside every block (code before a BEGIN,
/// a hash after an END) is not key text.
fn split_private_key_hidden(
    text: &str,
    file_path: Option<&Path>,
) -> Option<(String, octocode_engine::security::Edits)> {
    let first_edge = text
        .lines()
        .map(|line| line_markers(line.as_bytes()))
        .find(|markers| markers.any());
    if first_edge.is_none() && !is_private_key_path(file_path) {
        return None;
    }
    let mut cut_top = first_edge.is_none_or(|markers| markers.ends && !markers.begins);
    let mut block = KeyBlockState::default();
    hide_lines(text, |line| {
        let edge = is_private_key_boundary(line);
        let in_block = block.push_line(line);
        // A window shows a few lines past a cut: only a line shaped as key
        // text is hidden, never a short word after the body.
        let keyed = is_key_body_line(line) || opens_with_body_run(line) || holds_body_run(line);
        let hides = if edge {
            cut_top = false;
            true
        } else if cut_top {
            is_key_body_line(line) || opens_with_body_run(line)
        } else {
            in_block && keyed
        };
        hides.then(|| hidden_key(line)).flatten()
    })
}

/// A leaf judged as a read judges its text: its key blocks hidden line by
/// line first ([`leaf_key_blocks_hidden`]), so every view shows a key in
/// the one form a read does; then the detector's `scan` of that text (a
/// failure fails closed to the placeholder), then the split-key guard of
/// [`ContentSecurity::guard_native`] over the leaf as the detector left it.
fn guard_leaf(
    content: &str,
    scan: impl FnOnce(&str) -> octocode_engine::error::Result<Option<String>>,
) -> Option<String> {
    let (keyed, keys) = leaf_key_blocks_hidden(content);
    let redacted = scan(&keyed)
        .unwrap_or_else(|_| {
            Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
        })
        .or_else(|| (!keys.is_empty()).then(|| keyed.into_owned()));
    redact_split_private_key(redacted.as_deref().unwrap_or(content), None).or(redacted)
}

/// The content policy every reader is given, with the source sizes it judges at.
#[derive(Clone, Debug, Default)]
pub struct ContentSecurity {
    limits: super::SourceLimits,
    /// The sources one request judged whole and the texts it showed of them
    /// ([`Self::for_request`]); `None` for the policy every request shares.
    judged: Option<std::sync::Arc<super::JudgedFiles>>,
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
        Self {
            limits,
            judged: None,
        }
    }

    /// This policy for one request, and what that request judges with it:
    /// each source judged whole once ([`Self::judge_source`]) and the texts
    /// shown of them ([`Self::show`]), whose leaves the output gate honors
    /// ([`super::JudgedFiles::leaves`]).
    pub(crate) fn for_request(&self) -> (Self, std::sync::Arc<super::JudgedFiles>) {
        let judged = std::sync::Arc::new(super::JudgedFiles::default());
        let policy = Self {
            limits: self.limits,
            judged: Some(std::sync::Arc::clone(&judged)),
        };
        (policy, judged)
    }

    /// `raw`, the decoded text of the file at `path`, judged whole
    /// ([`super::JudgedText`]): the one redaction decision every text a tool
    /// shows of that source is cut from. A request's policy
    /// ([`Self::for_request`]) judges the same text of a path once; `judge`
    /// makes the judgment of a text it has not judged, given the text's
    /// digest ([`super::JudgedText::judge`], or a pass kept of that text).
    pub(crate) fn judge_source(
        &self,
        raw: &str,
        path: &Path,
        judge: impl FnOnce(&[u8; 32]) -> super::JudgedText,
    ) -> std::sync::Arc<super::JudgedText> {
        match &self.judged {
            Some(files) => files.judge(raw, path, judge),
            None => std::sync::Arc::new(judge(&octocode_engine::digest::sha256_digest(
                raw.as_bytes(),
            ))),
        }
    }

    /// Notes that this request cuts the rows of the file at `path` from its
    /// whole-source judgment ([`super::JudgedFiles::judged_whole`]).
    pub(crate) fn note_judged_whole(&self, path: &Path) {
        if let Some(files) = &self.judged {
            files.judged_whole(path);
        }
    }

    /// Whether this request judged the file at `path` whole, so its rows
    /// are cuts of that judgment.
    pub(crate) fn judged_whole(&self, path: &Path) -> bool {
        self.judged
            .as_ref()
            .is_some_and(|files| files.is_judged(path))
    }

    /// Records `text` as a leaf this request shows of the source judged at
    /// `path` ([`super::JudgedFiles::show`]).
    pub(crate) fn show(&self, path: &Path, text: &str) {
        if let Some(files) = &self.judged {
            files.show(path, text);
        }
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
        let (path, limit) = (path.as_deref(), self.limits.whole_source);
        // Key blocks first, line by line, as every view hides them: the
        // detector then reads their bodies as hidden already.
        let (keyed, keys) = private_key_blocks_hidden(content);
        let native = if keys.is_empty() {
            octocode_engine::security::sanitizer::sanitize_content_within(content, path, limit)
        } else {
            octocode_engine::security::sanitizer::sanitize_content_within(&keyed, path, limit).map(
                |scanned| {
                    let mut secrets_detected = vec![SPLIT_KEY_SECRET.to_owned()];
                    secrets_detected.extend(
                        scanned
                            .secrets_detected
                            .into_iter()
                            .filter(|name| name != SPLIT_KEY_SECRET),
                    );
                    SanitizationResult {
                        content: scanned.content.into_owned().into(),
                        has_secrets: true,
                        warnings: vec![format!("{} secret(s) redacted", secrets_detected.len())],
                        secrets_detected,
                        edits: keys.then(&scanned.edits),
                    }
                },
            )
        };
        self.guard_native(native, content.len(), file_path)
    }

    /// Sanitize one string leaf of a tool result about the file at
    /// `source_path` (`None`: no known file, the pathless output net): its
    /// redacted text, or `None` when nothing in it is redacted. The path only
    /// says what language the text is in, so source code is not mistaken for
    /// configuration; every credential pattern still applies.
    pub(crate) fn sanitize_leaf(&self, content: &str, source_path: Option<&str>) -> Option<String> {
        guard_leaf(content, |text| {
            octocode_engine::security::sanitizer::sanitize_leaf(text, source_path)
        })
    }

    /// [`Self::sanitize_leaf`] of a leaf of a response to the rows `own` was
    /// admitted with: a redaction that replaces only bytes of a string the
    /// caller sent is undone ([`super::Admitted::restore`]), so a lead or an
    /// error that repeats a request value repeats it as sent. A leaf that
    /// repeats none is judged by [`Self::sanitize_leaf`] alone, and a scan
    /// that fails hides the whole leaf.
    pub(crate) fn sanitize_response_leaf(
        &self,
        content: &str,
        source_path: Option<&str>,
        own: &super::Admitted,
    ) -> Option<String> {
        if own.is_empty() || !own.occurs_in(content) {
            return self.sanitize_leaf(content, source_path);
        }
        // Key blocks first, line by line, as every view hides them.
        let (keyed, keys) = leaf_key_blocks_hidden(content);
        let Ok(scanned) =
            octocode_engine::security::sanitizer::sanitize_leaf_edits(&keyed, source_path)
        else {
            return Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned());
        };
        // The replacements, each undone where it took only the caller's
        // bytes; then the split-key guard over that text, which shows every
        // armor line and hides every line of key text.
        let judged = match scanned {
            Some((clean, edits)) => own.restore(content, &clean, &keys.then(&edits)),
            None if !keys.is_empty() => own.restore(content, &keyed, &keys),
            None => None,
        };
        let guarded = split_private_key_hidden(judged.as_deref().unwrap_or(content), None);
        guarded.map(|(text, _)| text).or(judged)
    }

    /// [`Self::sanitize_leaf`] of a leaf cut from text the whole-source pass
    /// judged at `judged_path` ([`super::JudgedLeaves`]): the patterns that
    /// pass did not apply, and the split-key guard.
    pub(crate) fn sanitize_judged_leaf(
        &self,
        content: &str,
        source_path: Option<&str>,
        judged_path: &str,
    ) -> Option<String> {
        guard_leaf(content, |text| {
            octocode_engine::security::sanitizer::sanitize_judged_leaf(
                text,
                source_path,
                judged_path,
            )
        })
    }

    /// [`Self::sanitize_leaf`] of a leaf about to be kept: its text when any
    /// read could redact it, whichever file that read names. `source_path` is
    /// the file the leaf is best known to be about.
    pub(crate) fn sanitize_stored_leaf(
        &self,
        content: &str,
        source_path: Option<&str>,
    ) -> Option<String> {
        let (keyed, keys) = leaf_key_blocks_hidden(content);
        let redacted =
            octocode_engine::security::sanitizer::sanitize_stored_leaf(&keyed, source_path)
                .unwrap_or_else(|_| {
                    Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
                })
                .or_else(|| (!keys.is_empty()).then(|| keyed.into_owned()));
        redact_split_private_key(
            redacted.as_deref().unwrap_or(content),
            source_path.map(Path::new),
        )
        .or(redacted)
    }

    /// A clasify row's own text names no file and leaves for a provider. Its
    /// `evidence` is judged as a read judges text of no known file
    /// ([`Self::sanitize_text`] with no path, the judgement localSearch and
    /// localFetch give such text and the input gate refuses by), so the
    /// provider reads what every view shows; the caller's words beside it
    /// lose what a read of some other file would redact
    /// ([`Self::sanitize_request_words`]). Whether any of `row` was.
    pub(super) fn hide_provider_text(
        &self,
        row: &mut serde_json::Map<String, serde_json::Value>,
        evidence: &str,
    ) -> bool {
        let mut hidden = false;
        for field in [evidence, "questions", "context"] {
            let Some(value) = row.get_mut(field) else {
                continue;
            };
            hidden |= super::sanitize_json(value, &mut |text: &str, _: Option<&str>| {
                Ok::<_, std::convert::Infallible>(if field == evidence {
                    match self.sanitize_text(text, None).content {
                        std::borrow::Cow::Owned(clean) => Some(clean),
                        std::borrow::Cow::Borrowed(_) => None,
                    }
                } else {
                    self.sanitize_request_words(text)
                })
            })
            .unwrap_or_else(|never| match never {});
        }
        hidden
    }

    /// The caller's own words on their way to a provider (a question, a
    /// task's context), which the input gate admitted as a request: the
    /// gate refused whatever the file judge hides in text of no known file,
    /// so what is hidden here is only what a read of some other file's kind
    /// would redact ([`Self::sanitize_stored_leaf`]); a key's armor line,
    /// which holds no part of any key, stays. Text that holds a key's body,
    /// which the gate refuses, is hidden as any kept text is. `None` when
    /// nothing is hidden.
    pub(crate) fn sanitize_request_words(&self, text: &str) -> Option<String> {
        if holds_private_key_body(text) {
            return self.sanitize_stored_leaf(text, None);
        }
        match octocode_engine::security::sanitizer::sanitize_stored_leaf_edits(text, None) {
            Ok(scanned) => scanned.map(|(clean, _)| clean),
            Err(_) => Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned()),
        }
    }

    /// [`Self::sanitize_data`] of the caller's own words
    /// ([`Self::sanitize_request_words`]).
    pub(crate) fn sanitize_request_data(&self, value: &mut serde_json::Value) {
        let _ = super::sanitize_json(value, &mut |text: &str, _: Option<&str>| {
            Ok::<_, std::convert::Infallible>(self.sanitize_request_words(text))
        });
    }

    /// Text data at an external provider boundary is judged as every view
    /// judges it, so the provider reads what a view shows, with the same
    /// placeholders: a leaf about a file (a row naming its `path`) by that
    /// file's leaf rules, a text of no known file (clasify's inline
    /// `content`) as a read of such text is ([`Self::sanitize_leaf`] with no
    /// path), never by the rules of some other file's class (a `.properties`
    /// rule over `apiKey: null,`). Structured member context applies. Image
    /// assets and authentication headers are handled separately by their
    /// owner.
    ///
    /// A captured source's state (`{"source": {"path": …}, "text": …}`) is
    /// the exception: its `text` is the view the source's reader showed of
    /// that file, so it is judged by the rules of that file's path
    /// ([`Self::sanitize_text`]), the rules the reader applied. The text then
    /// reaches the provider as the reader showed it: the placeholders the
    /// reader wrote stay, and no rule of another file's class re-reads it
    /// (a `.properties` rule over a `.txt` line).
    pub(crate) fn sanitize_data(&self, value: &mut serde_json::Value) {
        let stored = &mut |text: &str, path: Option<&str>| {
            Ok::<_, std::convert::Infallible>(self.sanitize_leaf(text, path))
        };
        let source_path = value
            .pointer("/source/path")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let (Some(path), Some(fields)) = (source_path, value.as_object_mut())
            && let Some(serde_json::Value::String(text)) = fields.get_mut("text")
        {
            if let std::borrow::Cow::Owned(clean) =
                self.sanitize_text(text, Some(Path::new(&path))).content
            {
                *text = clean;
            }
            for (name, field) in fields.iter_mut() {
                if name != "text" {
                    let _ = super::sanitize_json(field, stored);
                }
            }
            return;
        }
        let _ = super::sanitize_json(value, stored);
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
