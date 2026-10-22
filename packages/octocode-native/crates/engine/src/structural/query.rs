use super::octo::CompileError;
use super::types::{StructuralDiagnostic, StructuralQueryExplanation};
use std::sync::OnceLock;

/// Describes how the text pre-filter is applied before AST parsing.
#[derive(Debug, PartialEq)]
pub(super) enum Prefilter {
    /// No safe literal anchor — must parse all candidate files.
    None,
    /// Single literal anchor; searched as a fixed string (fastest path).
    Single(String),
    /// Union of literals from `any:` branches; searched as a regex alternation.
    /// A file must contain at least one to match any alternative — sound prefilter.
    Union(Vec<String>),
}

pub(super) struct StructuralQuery<'a> {
    pattern: Option<&'a str>,
    rule: Option<&'a str>,
    /// Share one language-independent YAML parse (including errors) for this request.
    parsed_rule: OnceLock<Result<super::octo::RawRule, CompileError>>,
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
            (Some(_), None) | (None, Some(_)) => Ok(Self {
                pattern,
                rule,
                parsed_rule: OnceLock::new(),
            }),
            (Some(_), Some(_)) => Err("provide either `pattern` or `rule`, not both".to_string()),
            (None, None) => Err("structural search requires `pattern` or `rule`".to_string()),
        }
    }

    pub(super) fn parts(&self) -> (Option<&'a str>, Option<&'a str>) {
        (self.pattern, self.rule)
    }

    pub(super) fn is_rule(&self) -> bool {
        self.rule.is_some()
    }

    /// Returns the full prefilter descriptor for the text candidate-selection step.
    pub(super) fn prefilter(&self) -> Prefilter {
        self.anchors(AnchorUse::Prefilter)
    }

    /// The literals a match must contain, read to find where it could sit
    /// ([`AnchorUse::Recovery`]).
    pub(super) fn recovery_anchors(&self) -> Option<Vec<String>> {
        match self.anchors(AnchorUse::Recovery) {
            Prefilter::None => None,
            Prefilter::Single(anchor) => Some(vec![anchor]),
            Prefilter::Union(anchors) => Some(anchors),
        }
    }

    fn anchors(&self, anchor_use: AnchorUse) -> Prefilter {
        match (self.pattern, self.rule) {
            (Some(pattern), _) => match derive_literal_anchor(pattern, anchor_use) {
                Some(anchor) => Prefilter::Single(anchor.to_owned()),
                None => Prefilter::None,
            },
            (_, Some(_)) => self
                .parsed_rule()
                .map_or(Prefilter::None, |raw| prefilter_for_rule(raw, anchor_use)),
            _ => Prefilter::None,
        }
    }

    pub(super) fn explanation(&self) -> StructuralQueryExplanation {
        let prefilter = self.prefilter();
        self.explanation_with_prefilter(&prefilter)
    }

    pub(super) fn explanation_with_prefilter(
        &self,
        prefilter: &Prefilter,
    ) -> StructuralQueryExplanation {
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
        let (literal_anchor, pre_filter) = match prefilter {
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

    pub(super) fn parsed_rule(&self) -> Result<&super::octo::RawRule, CompileError> {
        self.parsed_rule
            .get_or_init(|| {
                // `parsed_rule` is only reached for rule queries, which always
                // carry `self.rule` — the invariant is upheld by the caller.
                #[expect(
                    clippy::expect_used,
                    reason = "only rule queries reach this compile path"
                )]
                super::octo::parse_rule(self.rule.expect("only rule queries compile a rule"))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn source(&self) -> Option<&'a str> {
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
        diagnostics: vec![invalid_query_diagnostic(message.to_owned())],
    }
}

/// The code of a query that is not exactly one non-empty pattern or rule.
pub const QUERY_INVALID: &str = "structural.query.invalid";

/// The error a query that is not exactly one non-empty pattern or rule
/// reports.
pub(super) fn invalid_query_diagnostic(message: String) -> StructuralDiagnostic {
    StructuralDiagnostic::new(QUERY_INVALID, "error", "match", message)
        .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.")
}

/// What the literals of a query are read for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AnchorUse {
    /// Choosing files to parse: only a run with a lower-case letter is safe
    /// (an all-upper-case run is a constant or keyword a grammar may
    /// normalize), as a wrong anchor drops a file's matches.
    Prefilter,
    /// Finding where a match sits in a file already parsed (see
    /// `octo::recovery`): a macro such as `JSON_ASSERT` anchors too, as a
    /// wrong anchor only changes a disclosure.
    Recovery,
}

fn derive_literal_anchor(pattern: &str, anchor_use: AnchorUse) -> Option<&str> {
    literal_anchor_candidates(pattern, anchor_use)
        .into_iter()
        .max_by_key(|token| token.len())
}

fn literal_anchor_candidates(pattern: &str, anchor_use: AnchorUse) -> Vec<&str> {
    let mut candidates = Vec::new();
    let mut token_start = None;
    let mut chars = pattern.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '$' {
            push_anchor_candidate(
                pattern,
                &mut candidates,
                &mut token_start,
                index,
                anchor_use,
            );
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
            push_anchor_candidate(
                pattern,
                &mut candidates,
                &mut token_start,
                index,
                anchor_use,
            );
        }
    }

    push_anchor_candidate(
        pattern,
        &mut candidates,
        &mut token_start,
        pattern.len(),
        anchor_use,
    );
    candidates
}

fn push_anchor_candidate<'a>(
    pattern: &'a str,
    candidates: &mut Vec<&'a str>,
    token_start: &mut Option<usize>,
    end: usize,
    anchor_use: AnchorUse,
) {
    let Some(start) = token_start.take() else {
        return;
    };
    let token = &pattern[start..end];
    if is_safe_anchor_token(token, anchor_use) {
        candidates.push(token);
    }
}

/// Only identifier-like runs can anchor: the AST ignores operator spacing, so
/// an operator run such as `&&!` in `$A&&!$B` is absent from `a && !b`.
fn is_anchor_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_safe_anchor_token(token: &str, anchor_use: AnchorUse) -> bool {
    token.len() >= 3
        && (anchor_use == AnchorUse::Recovery || token.chars().any(|ch| ch.is_ascii_lowercase()))
        && token
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Infer necessary file literals from the same parsed rule that compilation uses.
/// Conjunction may use any proven necessary condition; disjunction needs one
/// from every branch. A negated matcher never proves a positive file literal.
fn prefilter_for_rule(raw: &super::octo::RawRule, anchor_use: AnchorUse) -> Prefilter {
    rule_anchors(raw, anchor_use).map_or(Prefilter::None, |mut anchors| {
        anchors.sort();
        anchors.dedup();
        if anchors.len() == 1 {
            Prefilter::Single(anchors.remove(0))
        } else {
            Prefilter::Union(anchors)
        }
    })
}

fn rule_anchors(rule: &super::octo::RawRule, anchor_use: AnchorUse) -> Option<Vec<String>> {
    let mut required = Vec::new();
    if let Some(anchor) = rule
        .pattern
        .as_ref()
        .and_then(super::octo::RawPattern::literal)
        .and_then(|pattern| derive_literal_anchor(pattern, anchor_use))
    {
        required.push(vec![anchor.to_owned()]);
    }
    for child in rule
        .all
        .iter()
        .flatten()
        .chain(rule.has.iter().map(Box::as_ref))
        .chain(rule.inside.iter().map(Box::as_ref))
    {
        if let Some(anchors) = rule_anchors(child, anchor_use) {
            required.push(anchors);
        }
    }
    if let Some(any) = rule.any.as_ref().filter(|rules| !rules.is_empty())
        && let Some(branches) = any
            .iter()
            .map(|rule| rule_anchors(rule, anchor_use))
            .collect::<Option<Vec<_>>>()
    {
        required.push(branches.into_iter().flatten().collect());
    }
    required.into_iter().min_by_key(Vec::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derive_rule_prefilter(rule: &str) -> Prefilter {
        StructuralQuery::new(None, Some(rule)).unwrap().prefilter()
    }

    /// An all-upper-case macro cannot select files to parse, yet it still
    /// says where a match sits in a parsed file.
    #[test]
    fn an_upper_case_macro_anchors_recovery_but_not_the_prefilter() {
        let pattern = StructuralQuery::new(Some("JSON_ASSERT($$$A)"), None).unwrap();
        assert_eq!(pattern.prefilter(), Prefilter::None);
        assert_eq!(
            pattern.recovery_anchors(),
            Some(vec!["JSON_ASSERT".to_owned()])
        );
        let rule = StructuralQuery::new(
            None,
            Some("any: [{pattern: JSON_ASSERT($A)}, {pattern: foo($A)}]"),
        )
        .unwrap();
        assert_eq!(rule.prefilter(), Prefilter::None);
        assert_eq!(
            rule.recovery_anchors(),
            Some(vec!["JSON_ASSERT".to_owned(), "foo".to_owned()])
        );
        let anchorless = StructuralQuery::new(Some("$F($$$A)"), None).unwrap();
        assert_eq!(anchorless.recovery_anchors(), None);
    }

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

    /// The query prefilter keeps only literals every match must contain.
    #[test]
    fn rule_prefilter_keeps_only_required_anchors() {
        let single = |anchor: &str| Prefilter::Single(anchor.to_owned());
        let cases = [
            (
                "simple rule pattern",
                "rule:\n  pattern: await $C\n",
                single("await"),
            ),
            // `not:` alone is safe — the top-level positive `pattern:` implies the
            // file must contain the literal; `not:` only filters what's found.
            // Only the parsed positive pattern supplies the required anchor.
            (
                "positive anchor when not is present without any",
                "rule:\n  pattern: await $C\n  not:\n    pattern: bar\n",
                single("await"),
            ),
            (
                "any with a single anchor uses single",
                "rule:\n  any:\n    - pattern: foo($X)\n",
                single("foo"),
            ),
            (
                "any with multiple anchors uses union",
                "rule:\n  any:\n    - pattern: foo($X)\n    - pattern: bar($X)\n",
                Prefilter::Union(vec!["bar".to_owned(), "foo".to_owned()]),
            ),
            (
                "not does not hide the required any anchor",
                "rule:\n  not:\n    pattern: bar\n  any:\n    - pattern: foo($X)\n",
                single("foo"),
            ),
            // Patterns with only metavars produce no safe literal anchor.
            (
                "any without extractable anchors is none",
                "rule:\n  any:\n    - pattern: $X\n    - pattern: $Y\n",
                Prefilter::None,
            ),
        ];
        for (case, rule, expected) in cases {
            let query = StructuralQuery::new(None, Some(rule)).expect(case);
            assert_eq!(query.prefilter(), expected, "{case}");
        }
    }

    #[test]
    fn literal_anchor_never_uses_operator_runs_whose_spacing_the_ast_ignores() {
        // `$A&&!$B` must still match `a && !b`; operator text is not a
        // necessary file literal, so only identifier-like tokens anchor.
        for pattern in ["$A && $A()", "$A&&!$B", "$A=-1", "$A>>$B", "$A::$B"] {
            assert_eq!(
                StructuralQuery::new(Some(pattern), None)
                    .expect("valid query")
                    .prefilter(),
                Prefilter::None,
                "{pattern}"
            );
        }
        assert_eq!(
            StructuralQuery::new(Some("$A&&!foo($B)"), None)
                .expect("valid query")
                .prefilter(),
            Prefilter::Single("foo".to_owned())
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
