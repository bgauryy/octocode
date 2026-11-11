//! A text past what is held of a source at once, judged whole with a budget: only a
//! secret's own bytes go, and the rest stays, however long.
//!
//! A text already in memory needs no window: a pattern runs over all of it in linear
//! time and no memory past its own cache, so no value can sit across an edge. What
//! could still grow with the text is a walk's structure, so a document walk keeps to
//! the memory budget here, and past it every value of what the text names goes.
use super::document::Form;
use super::every_value::every_value_spans;
use super::source_names::{Named, SecretForm, SecretSpan, SecretSpans, SourceNames, SourceWalk};
use super::yaml_secret;
use super::{
    CodeSyntax, DetectResult, Judge, PATTERNS, Scope, SourceSyntax, applicable_patterns,
    pattern_regex_for, secret_groups,
};
use std::ops::Range;

/// The spans of one pattern in `text`: a `secret` group's when it names one, else
/// its whole match, past what the pattern's value test turns away. `source` is
/// the syntax of `text`, parsed once for every pattern that judges it.
fn pattern_spans(
    idx: usize,
    text: &str,
    scope: Scope<'_>,
    source: Option<SourceSyntax<'_>>,
    out: &mut Vec<(Range<usize>, usize)>,
) {
    let judge = Judge::of(idx, scope, text, source);
    judge.secrets(pattern_regex_for(idx, text), text, |_, secret| {
        out.push((secret, idx));
    });
}

/// Canonical capture and value exclusions, shared by text and structured data:
/// [`accepted_secrets`], the judgment the in-memory pass makes too.
/// Context-only callers need a named secret group, not a standalone token.
pub(super) fn pattern_matches(
    idx: usize,
    text: &str,
    scope: Scope<'_>,
    contextual_only: bool,
    found_secret: impl FnMut(Range<usize>, Range<usize>),
) {
    let regex = pattern_regex_for(idx, text);
    if contextual_only && secret_groups(regex).is_empty() {
        return;
    }
    let judge = Judge::of(idx, scope, text, None);
    judge.secrets(regex, text, found_secret);
}

/// The document walks of `text`, as `named` says it needs, and whether the walk
/// outgrew `budget`: then every value of what it names, secret or not.
fn document_spans(text: &str, named: Named, budget: usize) -> (Vec<SecretSpan>, bool) {
    let mut walk = SourceWalk::new(named, budget);
    walk.feed(text.as_bytes());
    match walk.finish() {
        SecretSpans::Exact(spans) => (yaml_values(text, spans), false),
        SecretSpans::TooMany => (every_value_spans(text.as_bytes(), named), true),
    }
}

/// `spans` with each block of a Secret in YAML as the values its lines hold
/// ([`yaml_secret::block_values`]): a text in hand shows its names.
fn yaml_values(text: &str, spans: Vec<SecretSpan>) -> Vec<SecretSpan> {
    let mut out = Vec::with_capacity(spans.len());
    let mut values = Vec::new();
    for span in spans {
        if span.form != SecretForm::SecretInYaml {
            out.push(span);
            continue;
        }
        yaml_secret::block_values(text, span.range, &mut values);
        out.extend(values.drain(..).map(|range| SecretSpan {
            range,
            form: SecretForm::SecretInYaml,
        }));
    }
    out
}

/// The walk budgets a test set, by the directory whose files they judge at.
#[cfg(feature = "test-hooks")]
static WALK_BUDGETS: std::sync::Mutex<Vec<(std::path::PathBuf, usize)>> =
    std::sync::Mutex::new(Vec::new());

/// Judge the long texts of files under `dir` at walk budget `budget` (`None`:
/// the process's), in place of the memory budget. Scoped to a directory so a
/// test's override reaches no other test's files.
#[cfg(feature = "test-hooks")]
pub fn set_walk_budget(dir: &std::path::Path, budget: Option<usize>) {
    let mut set = WALK_BUDGETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set.retain(|(held, _)| held != dir);
    if let Some(budget) = budget {
        set.push((dir.to_owned(), budget));
    }
}

/// The memory a walk of a text's structure may hold before it falls back to
/// hiding every value.
fn walk_budget(scope: Scope<'_>) -> usize {
    #[cfg(feature = "test-hooks")]
    if let Some(path) = scope.source_path() {
        let set = WALK_BUDGETS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, budget)) = set
            .iter()
            .find(|(dir, _)| std::path::Path::new(path).starts_with(dir))
        {
            return *budget;
        }
    }
    #[cfg(not(feature = "test-hooks"))]
    let _ = scope;
    crate::memory::MEMORY.limit()
}

/// What judging a long text found: the spans to hide, and whether they are every
/// value of a named structure because its walk outgrew the memory budget.
pub(super) struct Judged {
    pub(super) spans: Vec<(Range<usize>, usize)>,
    pub(super) every_value: bool,
}

/// Every span `scope` says is secret in `text`, in order, none overlapping; each
/// with the pattern index it goes under.
pub(super) fn secret_spans(text: &str, scope: Scope<'_>) -> Judged {
    secret_spans_within(text, scope, walk_budget(scope))
}

/// [`secret_spans`] with a document walk held to `budget`.
pub(super) fn secret_spans_within(text: &str, scope: Scope<'_>, budget: usize) -> Judged {
    let candidates = applicable_patterns(text, scope);
    let mut spans = Vec::new();
    // One parse for every judge: each pattern reads the same text, so its
    // syntax is the source's, through no edit, as in the in-memory pass.
    let syntax = scope
        .source_path()
        .and_then(|path| CodeSyntax::of(path, text));
    let edits = crate::security::Edits::default();
    for idx in candidates.iter() {
        if Form::of(idx).is_none() {
            let source = syntax.as_ref().map(|syntax| SourceSyntax {
                syntax,
                edits: &edits,
            });
            pattern_spans(idx, text, scope, source, &mut spans);
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
    let mut every_value = false;
    if named.any() {
        let (found, outgrown) = document_spans(text, named, budget);
        every_value = outgrown;
        for span in found {
            let idx = PATTERNS
                .iter()
                .position(|pattern| pattern.name == span.form.pattern());
            if let Some(idx) = idx {
                spans.push((span.range, idx));
            }
        }
    }
    // Where two spans start together, the pattern that knows the value's
    // shape or its document names it, not the name beside it.
    let named = super::named_values::PATTERN;
    spans.sort_by_key(|(range, idx)| (range.start, PATTERNS[*idx].name == named));
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
    Judged {
        spans: apart,
        every_value,
    }
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
        out.push_str(&super::kept_line_breaks(&text[range.clone()]));
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
    with_spans_hidden(text, &secret_spans(text, scope).spans)
}
