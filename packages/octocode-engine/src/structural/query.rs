use super::types::{StructuralDiagnostic, StructuralQueryExplanation};

/// Describes how the ripgrep pre-filter is applied before AST parsing.
#[derive(Debug, PartialEq)]
pub(super) enum Prefilter {
    /// No safe literal anchor — must parse all candidate files.
    None,
    /// Single literal anchor; ripgrep uses `--fixed-strings` for fastest path.
    Single(String),
    /// Union of literals from `any:` branches; ripgrep uses regex alternation.
    /// A file must contain at least one to match any alternative — sound prefilter.
    Union(Vec<String>),
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
    pub(super) fn prefilter(self) -> Prefilter {
        match (self.pattern, self.rule) {
            (Some(pattern), _) => match derive_literal_anchor(pattern) {
                Some(anchor) => Prefilter::Single(anchor.to_owned()),
                None => Prefilter::None,
            },
            (_, Some(rule)) => derive_rule_prefilter(rule),
            _ => Prefilter::None,
        }
    }

    pub(super) fn explanation(self) -> StructuralQueryExplanation {
        let prefilter = self.prefilter();
        let unsafe_reason = (self.is_rule() && matches!(prefilter, Prefilter::None))
            .then(|| "no literal is required by every matching rule branch".to_owned());
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

/// Infer necessary file literals from the same parsed rule that compilation uses.
/// Conjunction may use any proven necessary condition; disjunction needs one
/// from every branch. A negated matcher never proves a positive file literal.
fn derive_rule_prefilter(rule: &str) -> Prefilter {
    let Ok(raw) = super::octo::parse_rule(rule) else {
        return Prefilter::None;
    };
    rule_anchors(&raw).map_or(Prefilter::None, |mut anchors| {
        anchors.sort();
        anchors.dedup();
        if anchors.len() == 1 {
            Prefilter::Single(anchors.remove(0))
        } else {
            Prefilter::Union(anchors)
        }
    })
}

fn rule_anchors(rule: &super::octo::RawRule) -> Option<Vec<String>> {
    let mut required = Vec::new();
    if let Some(anchor) = rule.pattern.as_deref().and_then(derive_literal_anchor) {
        required.push(vec![anchor.to_owned()]);
    }
    for child in rule
        .all
        .iter()
        .flatten()
        .chain(rule.has.iter().map(Box::as_ref))
        .chain(rule.inside.iter().map(Box::as_ref))
    {
        if let Some(anchors) = rule_anchors(child) {
            required.push(anchors);
        }
    }
    if let Some(any) = rule.any.as_ref().filter(|rules| !rules.is_empty()) {
        if let Some(branches) = any.iter().map(rule_anchors).collect::<Option<Vec<_>>>() {
            required.push(branches.into_iter().flatten().collect());
        }
    }
    required.into_iter().min_by_key(Vec::len)
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
    fn parsed_yaml_literals_and_composition_keep_required_anchors() {
        for rule in [
            "pattern: |\n  foo($X)\n",
            "pattern: \"\\u0066oo($X)\"",
            "all: [{any: [{pattern: foo($X)}, {kind: call_expression}]}, {pattern: foo($Y)}]",
            "has: {pattern: foo($X), stopBy: end}",
        ] {
            assert_eq!(
                derive_rule_prefilter(rule),
                Prefilter::Single("foo".to_owned()),
                "{rule}"
            );
        }
        for rule in [
            "not: {pattern: foo($X)}",
            "any: [{pattern: foo($X)}, {pattern: $X}]",
            "any: [{pattern: foo($X)}, {not: {pattern: bar($X)}}]",
            "unknown: {pattern: foo($X)}",
        ] {
            assert_eq!(derive_rule_prefilter(rule), Prefilter::None, "{rule}");
        }
    }

    #[test]
    fn prefilter_covers_simple_rule_pattern() {
        assert_eq!(
            StructuralQuery::new(None, Some("rule:\n  pattern: await $C\n"))
                .expect("valid query")
                .prefilter(),
            Prefilter::Single("await".to_owned())
        );
    }

    #[test]
    fn prefilter_uses_positive_anchor_when_not_present_without_any() {
        // `not:` alone is safe — the top-level positive `pattern:` implies the
        // file must contain the literal; `not:` only filters what's found.
        // Only the parsed positive pattern supplies the required anchor.
        let rule = "rule:\n  pattern: await $C\n  not:\n    pattern: bar\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Single("await".to_owned()));
    }

    #[test]
    fn prefilter_any_with_single_anchor_uses_single() {
        let rule = "rule:\n  any:\n    - pattern: foo($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Single("foo".to_owned()));
    }

    #[test]
    fn prefilter_any_with_multiple_anchors_uses_union() {
        let rule = "rule:\n  any:\n    - pattern: foo($X)\n    - pattern: bar($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(
            q.prefilter(),
            Prefilter::Union(vec!["bar".to_owned(), "foo".to_owned()])
        );
    }

    #[test]
    fn prefilter_not_does_not_hide_required_any_anchor() {
        let rule = "rule:\n  not:\n    pattern: bar\n  any:\n    - pattern: foo($X)\n";
        let q = StructuralQuery::new(None, Some(rule)).expect("valid query");
        assert_eq!(q.prefilter(), Prefilter::Single("foo".to_owned()));
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
                .prefilter(),
            Prefilter::Single("&&".to_owned())
        );
    }

    #[test]
    fn literal_anchor_skips_metavars_and_prefers_identifier_literals() {
        assert_eq!(
            StructuralQuery::new(Some("console.log($$$ARGS)"), None)
                .expect("valid query")
                .prefilter(),
            Prefilter::Single("console".to_owned())
        );
        assert_eq!(
            StructuralQuery::new(Some("foo($X)"), None)
                .expect("valid query")
                .prefilter(),
            Prefilter::Single("foo".to_owned())
        );
    }
}
