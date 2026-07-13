use super::types::{StructuralDiagnostic, StructuralQueryExplanation};

/// Describes how the ripgrep pre-filter is applied before AST parsing.
#[derive(Debug, PartialEq)]
pub(super) enum Prefilter<'a> {
    /// No safe literal anchor — must parse all candidate files.
    None,
    /// Single literal anchor; ripgrep uses `--fixed-strings` for fastest path.
    Single(&'a str),
    /// Union of literals from `any:` branches; ripgrep uses regex alternation.
    /// A file must contain at least one to match any alternative — sound prefilter.
    Union(Vec<&'a str>),
}

#[derive(Clone, Copy)]
pub(super) struct StructuralQuery<'a> {
    pattern: Option<&'a str>,
    rule: Option<&'a str>,
}

impl<'a> StructuralQuery<'a> {
    pub(super) fn new(pattern: Option<&'a str>, rule: Option<&'a str>) -> Result<Self, String> {
        match (pattern, rule) {
            (Some(pattern), None) if pattern.trim().is_empty() => {
                Err("pattern must not be empty".to_string())
            }
            (None, Some(rule)) if rule.trim().is_empty() => {
                Err("rule must not be empty".to_string())
            }
            (Some(_), None) | (None, Some(_)) => Ok(Self { pattern, rule }),
            (Some(_), Some(_)) => Err("provide either `pattern` or `rule`, not both".to_string()),
            (None, None) => Err("structural search requires `pattern` or `rule`".to_string()),
        }
    }

    pub(super) fn parts(self) -> (Option<&'a str>, Option<&'a str>) {
        (self.pattern, self.rule)
    }

    pub(super) fn is_rule(self) -> bool {
        self.rule.is_some()
    }

    /// Returns the full prefilter descriptor for the ripgrep candidate-selection step.
    pub(super) fn prefilter(self) -> Prefilter<'a> {
        match (self.pattern, self.rule) {
            (Some(pattern), _) => match derive_literal_anchor(pattern) {
                Some(anchor) => Prefilter::Single(anchor),
                None => Prefilter::None,
            },
            (_, Some(rule)) => derive_rule_prefilter(rule),
            _ => Prefilter::None,
        }
    }

    /// Returns the single literal anchor if any, for backward-compatible callers.
    /// Returns `None` for union prefilters; use `prefilter()` for the full descriptor.
    #[cfg(test)]
    pub(super) fn literal_anchor(self) -> Option<&'a str> {
        match self.prefilter() {
            Prefilter::Single(s) => Some(s),
            _ => None,
        }
    }

    pub(super) fn explanation(self) -> StructuralQueryExplanation {
        let prefilter = self.prefilter();
        let unsafe_reason = self.unsafe_prefilter_reason().map(str::to_owned);
        let mut diagnostics = Vec::new();
        if let Some(reason) = unsafe_reason.as_deref() {
            diagnostics.push(
                StructuralDiagnostic::new(
                    "structural.prefilter.disabled",
                    "info",
                    "scan",
                    format!("Literal prefilter disabled: {reason}."),
                )
                .with_recovery("The engine will parse candidate files instead of trusting a single text anchor."),
            );
        }
        let (literal_anchor, pre_filter) = match &prefilter {
            Prefilter::None => (None, "disabled".to_owned()),
            Prefilter::Single(s) => (Some((*s).to_owned()), "literal-anchor".to_owned()),
            Prefilter::Union(anchors) => (Some(anchors.join("|")), "union-anchor".to_owned()),
        };

        StructuralQueryExplanation {
            kind: if self.is_rule() { "rule" } else { "pattern" }.to_owned(),
            source: self.source().unwrap_or_default().to_owned(),
            literal_anchor,
            pre_filter,
            unsafe_reason,
            diagnostics,
        }
    }

    fn source(self) -> Option<&'a str> {
        self.pattern.or(self.rule)
    }

    fn unsafe_prefilter_reason(self) -> Option<&'static str> {
        let rule = self.rule?;
        // `not:` + `any:` together: a file without any listed literal could still
        // match the `not:` arm, so no single anchor (or union) is sound.
        // `not:` alone is safe — the top-level positive `pattern:` anchor still
        // implies the file must contain it; `not:` only narrows matches further.
        // `any:` alone is handled by the union prefilter; no reason to disable.
        if rule.contains("not:") && rule.contains("any:") {
            return Some("`not:` combined with `any:` makes a single anchor unsound");
        }
        None
    }
}

pub(super) fn invalid_query_explanation(
    pattern: Option<&str>,
    rule: Option<&str>,
    message: &str,
) -> StructuralQueryExplanation {
    StructuralQueryExplanation {
        kind: "invalid".to_owned(),
        source: pattern.or(rule).unwrap_or_default().to_owned(),
        literal_anchor: None,
        pre_filter: "unavailable".to_owned(),
        unsafe_reason: None,
        diagnostics: vec![StructuralDiagnostic::new(
            "structural.query.invalid",
            "error",
            "match",
            message.to_owned(),
        )
        .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.")],
    }
}

fn derive_literal_anchor(pattern: &str) -> Option<&str> {
    literal_anchor_candidates(pattern)
        .into_iter()
        .max_by_key(|token| {
            (
                token.chars().any(|ch| ch.is_ascii_alphanumeric()),
                token.len(),
            )
        })
}

fn literal_anchor_candidates(pattern: &str) -> Vec<&str> {
    let mut candidates = Vec::new();
    let mut token_start = None;
    let mut chars = pattern.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '$' {
            push_anchor_candidate(pattern, &mut candidates, &mut token_start, index);
            while let Some((_, next)) = chars.peek() {
                if *next == '$'
                    || *next == '_'
                    || next.is_ascii_uppercase()
                    || next.is_ascii_digit()
                {
                    chars.next();
                } else {
                    break;
                }
            }
            continue;
        }

        if is_anchor_char(ch) {
            token_start.get_or_insert(index);
        } else {
            push_anchor_candidate(pattern, &mut candidates, &mut token_start, index);
        }
    }

    push_anchor_candidate(pattern, &mut candidates, &mut token_start, pattern.len());
    candidates
}

fn push_anchor_candidate<'a>(
    pattern: &'a str,
    candidates: &mut Vec<&'a str>,
    token_start: &mut Option<usize>,
    end: usize,
) {
    let Some(start) = token_start.take() else {
        return;
    };
    let token = &pattern[start..end];
    if is_safe_anchor_token(token) {
        candidates.push(token);
    }
}

fn is_anchor_char(ch: char) -> bool {
    ch == '_'
        || ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '&' | '|' | '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '?' | ':'
        )
}

fn is_safe_anchor_token(token: &str) -> bool {
    if token.len() >= 3
        && token.chars().any(|ch| ch.is_ascii_lowercase())
        && token
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return true;
    }

    token.len() >= 2
        && token
            .chars()
            .all(|ch| !ch.is_ascii_alphanumeric() && !ch.is_whitespace())
}

/// Derive a prefilter from a rule's `pattern:` declaration(s).
///
/// - `any:` without `not:`: extract one anchor per `pattern:` branch; the union
///   prefilter is sound (a file must contain ≥1 literal to match any alternative).
/// - `not:` alone: use the top-level positive `pattern:` anchor — `not:` only
///   narrows matches found by the positive arm, so the file must still contain it.
/// - `not:` + `any:` together: bail. A file could satisfy the `not:` arm without
///   containing any of the `any:` anchors, so no anchor set is sound.
/// - Neither: use the first `pattern:` line anchor.
fn derive_rule_prefilter(rule: &str) -> Prefilter<'_> {
    let has_not = rule.contains("not:");
    let has_any = rule.contains("any:");

    if has_not && has_any {
        return Prefilter::None;
    }

    if has_any {
        // Collect an anchor from every `pattern:` line inside the `any:` block.
        // All branches are candidates for the union prefilter.
        let mut anchors: Vec<&str> = rule
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                // Accept both `pattern: value` and `- pattern: value` (YAML list item).
                trimmed
                    .strip_prefix("- pattern:")
                    .or_else(|| trimmed.strip_prefix("pattern:"))
                    .map(|rest| rest.trim().trim_matches(['\'', '"']))
            })
            .filter_map(derive_literal_anchor)
            .collect();
        return match anchors.len() {
            0 => Prefilter::None,
            // `remove(0)` is infallible here (len == 1) and keeps this
            // clippy-clean under `deny(clippy::unwrap_used)`.
            1 => Prefilter::Single(anchors.remove(0)),
            _ => Prefilter::Union(anchors),
        };
    }

    // Simple rule (possibly with `not:` but no `any:`): use the first positive pattern.
    for line in rule.lines() {
        // Skip lines that are inside a `not:` block heuristically — they start
        // with `not:` or belong to an indented sub-key. Since we iterate top-down,
        // we stop at the first `pattern:` that isn't under `not:`.
        let trimmed = line.trim_start();
        if trimmed.starts_with("not:") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("pattern:") {
            let value = rest.trim().trim_matches(['\'', '"']);
            if let Some(anchor) = derive_literal_anchor(value) {
                return Prefilter::Single(anchor);
            }
        }
    }
    Prefilter::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_both_or_neither_query_sources() {
        assert!(StructuralQuery::new(Some("a"), Some("b")).is_err());
        assert!(StructuralQuery::new(None, None).is_err());
    }

    #[test]
    fn new_rejects_empty_query_text() {
        assert!(StructuralQuery::new(Some("   "), None).is_err());
        assert!(StructuralQuery::new(None, Some("   ")).is_err());
    }

    #[test]
    fn prefilter_covers_simple_rule_pattern() {
        assert_eq!(
            StructuralQuery::new(None, Some("rule:\n  pattern: await $C\n"))
                .expect("valid query")
                .prefilter(),
            Prefilter::Single("await")
        );
    }

    #[test]
    fn prefilter_uses_positive_anchor_when_not_present_without_any() {
        // `not:` alone is safe — the top-level positive `pattern:` implies the
        // file must contain the literal; `not:` only filters what's found.
        // The heuristic skips the `not:` line and picks the `pattern:` line.
        let rule = "rule:\n  pattern: await $C\n  not:\n    pattern: bar\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Single("await"));
    }

    #[test]
    fn prefilter_any_with_single_anchor_uses_single() {
        let rule = "rule:\n  any:\n    - pattern: foo($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Single("foo"));
    }

    #[test]
    fn prefilter_any_with_multiple_anchors_uses_union() {
        let rule = "rule:\n  any:\n    - pattern: foo($X)\n    - pattern: bar($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Union(vec!["foo", "bar"]));
    }

    #[test]
    fn prefilter_not_plus_any_bails() {
        let rule = "rule:\n  not:\n    pattern: bar\n  any:\n    - pattern: foo($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::None);
    }

    #[test]
    fn prefilter_any_without_extractable_anchors_is_none() {
        // Patterns with only metavars produce no safe literal anchor.
        let rule = "rule:\n  any:\n    - pattern: $X\n    - pattern: $Y\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::None);
    }

    #[test]
    fn literal_anchor_uses_operator_when_pattern_has_no_identifier_anchor() {
        assert_eq!(
            StructuralQuery::new(Some("$A && $A()"), None)
                .expect("valid query")
                .literal_anchor(),
            Some("&&")
        );
    }

    #[test]
    fn literal_anchor_skips_metavars_and_prefers_identifier_literals() {
        assert_eq!(
            StructuralQuery::new(Some("console.log($$$ARGS)"), None)
                .expect("valid query")
                .literal_anchor(),
            Some("console")
        );
        assert_eq!(
            StructuralQuery::new(Some("foo($X)"), None)
                .expect("valid query")
                .literal_anchor(),
            Some("foo")
        );
    }
}
