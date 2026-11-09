use super::octo::CompileError;
use super::types::{StructuralDiagnostic, StructuralQueryExplanation};
use serde_json::Value;
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
    /// Groups a file must each hold a literal of (an AND of ORs): one per
    /// necessary literal of a pattern, one union per `any`. Two or more
    /// groups; one group is [`Self::Single`] or [`Self::Union`].
    All(Vec<Vec<String>>),
}

pub(super) struct StructuralQuery<'a> {
    pattern: Option<&'a str>,
    rule: Option<&'a Value>,
    /// Share one language-independent rule decode (including errors) for this request.
    parsed_rule: OnceLock<Result<super::octo::RawRule, CompileError>>,
}

impl<'a> StructuralQuery<'a> {
    pub(super) fn new(pattern: Option<&'a str>, rule: Option<&'a Value>) -> Result<Self, String> {
        match (pattern, rule) {
            (Some(pattern), None) if pattern.trim().is_empty() => {
                Err("pattern must not be empty".to_string())
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

    pub(super) fn parts(&self) -> (Option<&'a str>, Option<&'a Value>) {
        (self.pattern, self.rule)
    }

    pub(super) fn is_rule(&self) -> bool {
        self.rule.is_some()
    }

    /// Returns the full prefilter descriptor for the text candidate-selection
    /// step: every group of literals the query requires.
    pub(super) fn prefilter(&self) -> Prefilter {
        self.required(AnchorUse::Prefilter, Reach::File)
            .map_or(Prefilter::None, |required| prefilter_of(required.groups))
    }

    /// The literals a match must contain, read to find where it could sit
    /// ([`AnchorUse::Recovery`]).
    pub(super) fn recovery_anchors(&self) -> Option<Vec<String>> {
        self.narrowest(AnchorUse::Recovery)
    }

    /// The literals one of which a macro body must hold to be re-parsed
    /// ([`AnchorUse::Prefilter`] safety). A pattern gives its own required
    /// literals. A rule gives the [`Reach::Node`] literals of the rule
    /// itself (what a node's own text must spell, with no literal read from
    /// an `inside`, `follows` or `precedes` neighbour), plus those of every
    /// rule a `has` tests anywhere in it (`has_targets`: under
    /// `not`, `any`, `all`, another relation, a stop rule or a nested `has`),
    /// each taken on its own: a `has` target may match inside a body below a
    /// node matched outside it. The anchors are all those literals, sorted
    /// and deduplicated. `None` (every body is re-parsed) when the pattern,
    /// the rule or any one `has` target needs no literal, or when the query
    /// is neither a pattern nor a rule that decodes.
    pub(super) fn macro_anchors(&self) -> Option<Vec<String>> {
        let groups = match (self.pattern, self.rule) {
            (Some(pattern), _) => pattern_required(pattern, AnchorUse::Prefilter)?.groups,
            (_, Some(_)) => {
                let rule = self.parsed_rule().ok()?;
                let mut groups = Vec::new();
                for tested in std::iter::once(rule).chain(rule.has_targets()) {
                    groups.extend(rule_required(tested, AnchorUse::Prefilter, Reach::Node)?.groups);
                }
                groups
            }
            _ => return None,
        };
        let mut anchors: Vec<String> = groups.into_iter().flatten().collect();
        anchors.sort();
        anchors.dedup();
        Some(anchors)
    }

    /// The one group a reader looks for a match by: the narrowest
    /// requirement, sorted.
    fn narrowest(&self, anchor_use: AnchorUse) -> Option<Vec<String>> {
        self.required(anchor_use, Reach::File).map(|required| {
            let mut anchors = required.narrowest;
            anchors.sort();
            anchors.dedup();
            anchors
        })
    }

    fn required(&self, anchor_use: AnchorUse, reach: Reach) -> Option<Required> {
        match (self.pattern, self.rule) {
            (Some(pattern), _) => pattern_required(pattern, anchor_use),
            (_, Some(_)) => self
                .parsed_rule()
                .ok()
                .and_then(|raw| rule_required(raw, anchor_use, reach)),
            _ => None,
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
            Prefilter::All(groups) => (Some(groups_text(groups)), "all-anchors".to_owned()),
        };

        StructuralQueryExplanation {
            kind: if self.is_rule() { "rule" } else { "pattern" }.to_owned(),
            source: self.source(),
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
                super::octo::decode_rule(self.rule.expect("only rule queries compile a rule"))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn source(&self) -> String {
        source_text(self.pattern, self.rule)
    }
}

/// The query text an explanation quotes: the pattern, or the rule as JSON.
fn source_text(pattern: Option<&str>, rule: Option<&Value>) -> String {
    pattern
        .map(str::to_owned)
        .unwrap_or_else(|| rule.map(Value::to_string).unwrap_or_default())
}

pub(super) fn invalid_query_explanation(
    pattern: Option<&str>,
    rule: Option<&Value>,
    message: &str,
) -> StructuralQueryExplanation {
    StructuralQueryExplanation {
        kind: "invalid".to_owned(),
        source: source_text(pattern, rule),
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
        .with_recovery("Provide exactly one non-empty structural pattern or rule object.")
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

/// Where a requirement's literals sit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Anywhere in the file: every conjunct, an `inside` ancestor's too.
    File,
    /// Within the matched node's own text: an `inside` ancestor spans more
    /// than the match, so its literals may sit outside it.
    Node,
}

/// What every match of a query spells.
struct Required {
    /// Every group of literals a match holds one of.
    groups: Vec<Vec<String>>,
    /// The group with the fewest literals, first found; a pattern's longest
    /// literal. What a reader looks for one match by.
    narrowest: Vec<String>,
}

impl Required {
    /// A requirement of one group.
    fn group(anchors: Vec<String>) -> Self {
        Self {
            groups: vec![anchors.clone()],
            narrowest: anchors,
        }
    }

    /// Adds `other`'s groups; its narrowest replaces this one only when it
    /// has fewer literals.
    fn and(&mut self, other: Self) {
        self.groups.extend(other.groups);
        if other.narrowest.len() < self.narrowest.len() {
            self.narrowest = other.narrowest;
        }
    }
}

/// Each safe literal of `pattern` is necessary: one group per literal, and
/// the longest (the last of equal length) is the narrowest.
fn pattern_required(pattern: &str, anchor_use: AnchorUse) -> Option<Required> {
    let candidates = literal_anchor_candidates(pattern, anchor_use);
    let longest = candidates.iter().max_by_key(|token| token.len())?;
    Some(Required {
        narrowest: vec![(*longest).to_owned()],
        groups: candidates
            .iter()
            .map(|token| vec![(*token).to_owned()])
            .collect(),
    })
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

/// The prefilter of `groups`: sorted, deduplicated, and without a group
/// that holds every literal of another (the smaller one implies it).
fn prefilter_of(groups: Vec<Vec<String>>) -> Prefilter {
    let mut groups: Vec<Vec<String>> = groups
        .into_iter()
        .map(|mut group| {
            group.sort();
            group.dedup();
            group
        })
        .collect();
    groups.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    groups.dedup();
    let mut kept: Vec<Vec<String>> = Vec::with_capacity(groups.len());
    for group in groups {
        if !kept
            .iter()
            .any(|smaller| smaller.iter().all(|anchor| group.contains(anchor)))
        {
            kept.push(group);
        }
    }
    match kept.len() {
        0 => Prefilter::None,
        1 => {
            let mut group = kept.remove(0);
            if group.len() == 1 {
                Prefilter::Single(group.remove(0))
            } else {
                Prefilter::Union(group)
            }
        }
        _ => Prefilter::All(kept),
    }
}

/// `groups` as the explanation quotes them: `|` inside a group, ` & `
/// between groups.
pub(super) fn groups_text(groups: &[Vec<String>]) -> String {
    groups
        .iter()
        .map(|group| group.join("|"))
        .collect::<Vec<_>>()
        .join(" & ")
}

/// Infer necessary file literals from the same parsed rule that compilation
/// uses. A conjunction (the rule's own matchers, `all`, `has`, `inside`)
/// needs every child's literals; a disjunction (`any`) needs one from every
/// branch, so it adds one group: the union of each branch's narrowest. A
/// negated or positional matcher (`not`, `precedes`, `follows`) and a
/// `{context, selector}` pattern never prove a file literal. Under
/// [`Reach::Node`], `inside` adds nothing either.
fn rule_required(
    rule: &super::octo::RawRule,
    anchor_use: AnchorUse,
    reach: Reach,
) -> Option<Required> {
    let mut required: Option<Required> = None;
    let mut add = |found: Required| match required.as_mut() {
        Some(required) => required.and(found),
        None => required = Some(found),
    };
    if let Some(found) = rule
        .pattern
        .as_ref()
        .and_then(super::octo::RawPattern::literal)
        .and_then(|pattern| pattern_required(pattern, anchor_use))
    {
        add(found);
    }
    if let Some(anchor) = rule.regex.as_deref().and_then(literal_regex) {
        add(Required::group(vec![anchor.to_owned()]));
    }
    for child in rule
        .all
        .iter()
        .flatten()
        .chain(rule.has.iter().map(Box::as_ref))
        .chain(
            rule.inside
                .iter()
                .filter(|_| reach == Reach::File)
                .map(Box::as_ref),
        )
    {
        if let Some(found) = rule_required(child, anchor_use, reach) {
            add(found);
        }
    }
    if let Some(any) = rule.any.as_ref().filter(|rules| !rules.is_empty())
        && let Some(branches) = any
            .iter()
            .map(|rule| rule_required(rule, anchor_use, reach))
            .collect::<Option<Vec<_>>>()
    {
        add(Required::group(
            branches
                .into_iter()
                .flat_map(|branch| branch.narrowest)
                .collect(),
        ));
    }
    required
}

/// The text a rule `regex` that spells only a literal requires, bare or
/// between `^` and `$`: the node it tests holds that text with the same case
/// (`Regex::new` sets no flag), so the file holding the node does too. `None`
/// for any regex syntax beyond those two anchors.
fn literal_regex(regex: &str) -> Option<&str> {
    let regex = regex.strip_prefix('^').unwrap_or(regex);
    let literal = regex.strip_suffix('$').unwrap_or(regex);
    (!literal.is_empty() && regex::escape(literal) == literal).then_some(literal)
}

#[cfg(test)]
mod tests;
