//! A text past what is held of a source at once, judged whole with a budget: only a
//! secret's own bytes go, and the rest stays, however long.
//!
//! A text already in memory needs no window: a pattern runs over all of it in linear
//! time and no memory past its own cache, so no value can sit across an edge. What
//! could still grow with the text is a walk's structure, so a document walk keeps to
//! the memory budget here, and past it every value of what the text names goes.
use super::document::Form;
use super::every_value::every_value_spans;
use super::source_names::{Named, SecretSpan, SecretSpans, SourceNames, SourceWalk};
use super::{
    DetectResult, KEEPS_NON_CREDENTIAL_VALUES, PATTERNS, Scope, applicable_patterns,
    assigns_a_credential, pattern_regex_for,
};
use std::ops::Range;

/// The spans of one pattern in `text`: a `secret` group's when it names one, else
/// its whole match, past what `redacts` turns away.
fn pattern_spans(idx: usize, text: &str, out: &mut Vec<(Range<usize>, usize)>) {
    pattern_matches(idx, text, false, |_, secret| out.push((secret, idx)));
}

/// Canonical capture and value exclusions, shared by text and structured data.
/// Context-only callers need a named secret group, not a standalone token.
pub(super) fn pattern_matches(
    idx: usize,
    text: &str,
    contextual_only: bool,
    mut found_secret: impl FnMut(Range<usize>, Range<usize>),
) {
    let regex = pattern_regex_for(idx, text);
    let groups = regex
        .capture_names()
        .enumerate()
        .filter_map(|(at, name)| name.filter(|name| name.starts_with("secret")).map(|_| at))
        .collect::<Vec<_>>();
    if contextual_only && groups.is_empty() {
        return;
    }
    let keeps_values = KEEPS_NON_CREDENTIAL_VALUES[idx];
    let (mut line_start, mut searched_to) = (0, 0);
    for found in regex.captures_iter(text) {
        let Some(whole) = found.get(0) else {
            continue;
        };
        let secret = groups.iter().find_map(|&at| found.get(at)).unwrap_or(whole);
        if let Some(at) = text[searched_to..whole.start()].rfind('\n') {
            line_start = searched_to + at + 1;
        }
        searched_to = whole.start();
        if !keeps_values || assigns_a_credential(&text[line_start..secret.start()], secret.as_str())
        {
            found_secret(whole.range(), secret.range());
        }
    }
}

/// The document walks of `text`, as `named` says it needs. Past `budget`, every
/// value of what it names.
fn document_spans(text: &str, named: Named, budget: usize) -> Vec<SecretSpan> {
    let mut walk = SourceWalk::new(named, budget);
    walk.feed(text.as_bytes());
    match walk.finish() {
        SecretSpans::Exact(spans) => spans,
        SecretSpans::TooMany => every_value_spans(text.as_bytes(), false, named),
    }
}

/// Every span `scope` says is secret in `text`, in order, none overlapping; each
/// with the pattern index it goes under.
pub(super) fn secret_spans(text: &str, scope: Scope<'_>) -> Vec<(Range<usize>, usize)> {
    let candidates = applicable_patterns(text, scope);
    let mut spans = Vec::new();
    for idx in candidates.iter() {
        if Form::of(idx).is_none() {
            pattern_spans(idx, text, &mut spans);
        }
    }
    let mut names = SourceNames::for_path("");
    names.feed(text.as_bytes());
    let named = Named {
        objects: names.finish().objects,
        yaml: candidates.iter().any(|idx| {
            Form::of(idx) == Some(Form::SecretInYaml) && pattern_regex_for(idx, text).is_match(text)
        }),
    };
    if named.any() {
        let budget = crate::memory::MEMORY.limit();
        for span in document_spans(text, named, budget) {
            let idx = PATTERNS
                .iter()
                .position(|pattern| pattern.name == span.form.pattern());
            if let Some(idx) = idx {
                spans.push((span.range, idx));
            }
        }
    }
    spans.sort_unstable_by_key(|(range, _)| range.start);
    let mut apart: Vec<(Range<usize>, usize)> = Vec::with_capacity(spans.len());
    for (mut range, idx) in spans {
        if let Some((last, _)) = apart.last() {
            if range.end <= last.end {
                continue;
            }
            range.start = range.start.max(last.end);
        }
        apart.push((range, idx));
    }
    apart
}

/// `text` with each of `spans` replaced by its pattern's placeholder, a line break
/// in one kept so no line moves; `None` when none hides anything.
pub(super) fn with_spans_hidden(
    text: &str,
    spans: &[(Range<usize>, usize)],
) -> Option<DetectResult> {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    let mut secrets_detected: Vec<String> = Vec::new();
    let mut replaced = Vec::new();
    for (range, idx) in spans {
        if text[range.clone()].trim().is_empty() {
            continue;
        }
        out.push_str(&text[from..range.start]);
        let shown_from = out.len();
        out.push_str(super::replacement_for(*idx));
        let breaks = text[range.clone()]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        out.extend(std::iter::repeat_n('\n', breaks));
        replaced.push((range.clone(), out.len() - shown_from));
        from = range.end;
        let name = PATTERNS[*idx].name;
        if !secrets_detected.iter().any(|seen| seen == name) {
            secrets_detected.push(name.to_owned());
        }
    }
    if secrets_detected.is_empty() {
        return None;
    }
    out.push_str(&text[from..]);
    Some(DetectResult {
        sanitized: out,
        secrets_detected,
        edits: crate::security::Edits::of_pass(replaced),
    })
}

/// [`super::redact`] of a text past [`super::MAX_CONTENT_SIZE`]: one pass, no copy
/// per pattern, and a walk within the memory budget.
pub(super) fn redact_long(text: &str, scope: Scope<'_>) -> Option<DetectResult> {
    with_spans_hidden(text, &secret_spans(text, scope))
}
