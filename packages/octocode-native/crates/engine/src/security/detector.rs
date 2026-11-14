//! Secret detection by the known-format list ([`PATTERNS`]): every match of a
//! pattern hides its secret part, and nothing else is judged.
//!
//! One literal prescan ([`prescan`]) picks the patterns a text can match, so a
//! text that holds none of their literals runs no regex. Each candidate then
//! runs over the whole text once, in linear time, however long the text is:
//! no window edge can cut a value. The matched secrets of every pattern are
//! joined into one ordered set of byte ranges ([`secret_spans`]), and each
//! range becomes its pattern's placeholder; the text around it stays byte for
//! byte.

use super::patterns::PATTERNS;
use std::ops::Range;
use std::sync::LazyLock;

mod long_line;
mod prescan;
mod regex_forms;
mod request;

/// The prescan gate `build.rs` derives from [`PATTERNS`]: `GATE_LITERALS`
/// (lowercased literal, pattern index) and the `FALLBACK` pattern indices
/// ([`prescan`]).
mod gates {
    include!(concat!(env!("OUT_DIR"), "/secret_gates.rs"));
}

pub use gates::LONGEST_BOUNDED_MATCH;
pub use long_line::{LONG_LINE_BYTES, LongLine};
pub(crate) use request::{holds_credential_as_request, literal_form};

use prescan::prescan_candidates;
use regex_forms::pattern_regex_for;

/// The largest source a read holds whole.
pub const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;

/// Each pattern's placeholder, by pattern index.
pub(super) static REPLACEMENTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|p| super::redaction_placeholder(p.name))
        .collect()
});

fn replacement_for(idx: usize) -> &'static str {
    &REPLACEMENTS[idx]
}

/// Capture group name prefix marking the secret part of a match that also
/// spells its non-secret context (an assignment's name, a URL's host).
const SECRET_GROUP: &str = "secret";

/// The capture groups of `regex` that name a match's secret: one per branch
/// of an alternation (`secret`, `secret_…`).
fn secret_groups(regex: &regex::Regex) -> Vec<usize> {
    regex
        .capture_names()
        .enumerate()
        .filter(|(_, name)| name.is_some_and(|name| name.starts_with(SECRET_GROUP)))
        .map(|(index, _)| index)
        .collect()
}

/// Capture group name marking a branch whose value only stands for a secret
/// (an interpolation, a template placeholder): a match through it hides
/// nothing.
const TEMPLATE_GROUP: &str = "template";

/// The secret of one match of a pattern whose secret groups are `groups`:
/// the first of them that took part, else the whole match; none when the
/// match went through its `template` branch.
fn secret_of<'h>(captures: &regex::Captures<'h>, groups: &[usize]) -> Option<regex::Match<'h>> {
    if captures.name(TEMPLATE_GROUP).is_some() {
        return None;
    }
    groups
        .iter()
        .find_map(|&group| captures.get(group))
        .or_else(|| captures.get(0))
}

/// One secret a pattern found: its bytes in the text, and the pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretSpan {
    pub range: Range<usize>,
    pub pattern: usize,
}

impl SecretSpan {
    /// The placeholder that replaces this secret.
    #[must_use]
    pub fn placeholder(&self) -> &'static str {
        replacement_for(self.pattern)
    }
}

/// Whether `secret` is hidden already: it holds a placeholder, and nothing
/// but placeholders and blanks (a key body whose every line a placeholder
/// took). Such a match is the tool's own text, never redacted again.
fn is_hidden(secret: &str) -> bool {
    let placeholders = super::placeholder_regex();
    placeholders.is_match(secret)
        && placeholders
            .split(secret)
            .all(|rest| rest.bytes().all(|byte| byte.is_ascii_whitespace()))
}

/// The secrets of `text`, ascending and disjoint: every match of every
/// pattern, its `secret` group's bytes when it names one, else the whole
/// match ([`secret_of`]). Overlapping secrets join into one span, named by the pattern whose
/// secret starts first (the earlier pattern on a tie).
#[must_use]
pub fn secret_spans(text: &str) -> Vec<SecretSpan> {
    let mut found: Vec<SecretSpan> = Vec::new();
    for idx in prescan_candidates(text).iter() {
        let regex = pattern_regex_for(idx, text);
        let groups = secret_groups(regex);
        let mut keep = |secret: regex::Match<'_>| {
            if !secret.is_empty() && !is_hidden(secret.as_str()) {
                found.push(SecretSpan {
                    range: secret.range(),
                    pattern: idx,
                });
            }
        };
        if groups.is_empty() {
            regex.find_iter(text).for_each(&mut keep);
        } else {
            for captures in regex.captures_iter(text) {
                if let Some(secret) = secret_of(&captures, &groups) {
                    keep(secret);
                }
            }
        }
    }
    found.sort_by_key(|span| (span.range.start, span.pattern));
    let mut joined: Vec<SecretSpan> = Vec::with_capacity(found.len());
    for span in found {
        match joined.last_mut() {
            Some(last) if span.range.start < last.range.end => {
                last.range.end = last.range.end.max(span.range.end);
            }
            _ => joined.push(span),
        }
    }
    joined
}

/// The line breaks a redacted `secret` spanned, each as it was and with the
/// blanks that indent the line it starts, so its replacement keeps the
/// source's lines, line ends and indentation: a key body hides its text, not
/// its layout, and every later line keeps its number.
fn kept_line_breaks(secret: &str) -> String {
    if memchr::memchr2(b'\n', b'\r', secret.as_bytes()).is_none() {
        return String::new();
    }
    let bytes = secret.as_bytes();
    let mut kept = String::new();
    let mut at = 0;
    while at < bytes.len() {
        let len = match bytes[at] {
            b'\r' if bytes.get(at + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                at += 1;
                continue;
            }
        };
        kept.push_str(&secret[at..at + len]);
        at += len;
        let blanks = bytes[at..]
            .iter()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        kept.push_str(&secret[at..at + blanks]);
        at += blanks;
    }
    kept
}

pub(crate) struct DetectResult {
    pub sanitized: String,
    pub secrets_detected: Vec<String>,
    /// What `sanitized` replaced of the content, by content byte position.
    pub edits: super::Edits,
}

/// `content` with every secret of [`secret_spans`] replaced by its pattern's
/// placeholder (and the line breaks it spanned); `None` when it holds none,
/// so clean content is never copied.
pub(crate) fn redact(content: &str) -> Option<DetectResult> {
    let spans = secret_spans(content);
    if spans.is_empty() {
        return None;
    }
    let mut sanitized = String::with_capacity(content.len());
    let mut replaced = Vec::with_capacity(spans.len());
    let mut patterns: Vec<usize> = Vec::new();
    let mut from = 0;
    for span in &spans {
        let secret = &content[span.range.clone()];
        let line_breaks = kept_line_breaks(secret);
        let placeholder = span.placeholder();
        sanitized.push_str(&content[from..span.range.start]);
        sanitized.push_str(placeholder);
        sanitized.push_str(&line_breaks);
        replaced.push((span.range.clone(), placeholder.len() + line_breaks.len()));
        from = span.range.end;
        if !patterns.contains(&span.pattern) {
            patterns.push(span.pattern);
        }
    }
    sanitized.push_str(&content[from..]);
    patterns.sort_unstable();
    Some(DetectResult {
        sanitized,
        secrets_detected: patterns
            .into_iter()
            .map(|idx| PATTERNS[idx].name.to_owned())
            .collect(),
        edits: super::Edits::of_pass(replaced),
    })
}

#[cfg(test)]
mod tests;
