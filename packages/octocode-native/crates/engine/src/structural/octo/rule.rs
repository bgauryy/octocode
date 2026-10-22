use std::time::Instant;

use regex::Regex;
use serde::Deserialize;
use serde::de::IgnoredAny;
use tree_sitter::Node;

use crate::structural::kinds::named_kind_id;
use crate::structural::language::AgLanguage;

use super::error::CompileError;
use super::matching::{
    CandidatePlan, CaptureEnv, ExecutionError, SECONDARY_CAPTURE, ancestors_from_root,
    named_children, node_text, raw_range,
};
use super::pattern::CompiledPattern;
use super::position::{ExactRange, Ordinal, RawOrdinal};

/// A wrapped rule document. Besides `rule`, it tolerates the metadata keys
/// of an ast-grep rule file (`id`, `language`, `severity`, …) so a pasted
/// rule file runs as-is; none of them affects matching (the query's `language` selects
/// the grammar). Any other key, like an unsupported `constraints`/`fix`,
/// still fails with the list of supported fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuleDocument {
    rule: RawRule,
    #[serde(rename = "id")]
    _id: Option<IgnoredAny>,
    #[serde(rename = "language")]
    _language: Option<IgnoredAny>,
    #[serde(rename = "severity")]
    _severity: Option<IgnoredAny>,
    #[serde(rename = "message")]
    _message: Option<IgnoredAny>,
    #[serde(rename = "note")]
    _note: Option<IgnoredAny>,
    #[serde(rename = "url")]
    _url: Option<IgnoredAny>,
    #[serde(rename = "metadata")]
    _metadata: Option<IgnoredAny>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::structural) struct RawRule {
    kind: Option<String>,
    #[serde(rename = "nthChild")]
    nth_child: Option<RawOrdinal>,
    range: Option<ExactRange>,
    pub(in crate::structural) pattern: Option<RawPattern>,
    regex: Option<String>,
    pub(in crate::structural) has: Option<Box<Self>>,
    pub(in crate::structural) inside: Option<Box<Self>>,
    precedes: Option<Box<Self>>,
    follows: Option<Box<Self>>,
    field: Option<String>,
    pub(in crate::structural) all: Option<Vec<Self>>,
    pub(in crate::structural) any: Option<Vec<Self>>,
    not: Option<Box<Self>>,
    #[serde(rename = "stopBy")]
    stop_by: Option<RawStopBy>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(in crate::structural) enum RawPattern {
    Text(String),
    Context(ContextPattern),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::structural) struct ContextPattern {
    context: String,
    selector: String,
}

impl RawPattern {
    pub(in crate::structural) fn literal(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            // The wrapper is not required in the searched source.
            Self::Context(_) => None,
        }
    }

    fn compile(&self, lang: &AgLanguage) -> Result<CompiledPattern, CompileError> {
        match self {
            Self::Text(text) => CompiledPattern::new(lang, text),
            Self::Context(pattern) => {
                CompiledPattern::contextual(lang, &pattern.context, &pattern.selector)
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawStopBy {
    Kind(StopKind),
    Rule(Box<RawRule>),
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StopKind {
    #[default]
    Neighbor,
    End,
}

#[derive(Clone, Copy)]
enum Direction {
    Descendant,
    Ancestor,
    Next,
    Previous,
}

struct Relation {
    direction: Direction,
    rule: CompiledRule,
    field: Option<std::num::NonZeroU16>,
    stop: Stop,
}

enum Stop {
    Neighbor,
    End,
    Rule(Box<CompiledRule>),
}

impl RawRule {
    fn relations(&self) -> impl Iterator<Item = (Direction, &Self)> {
        [
            (Direction::Descendant, self.has.as_deref()),
            (Direction::Ancestor, self.inside.as_deref()),
            (Direction::Next, self.precedes.as_deref()),
            (Direction::Previous, self.follows.as_deref()),
        ]
        .into_iter()
        .filter_map(|(direction, rule)| rule.map(|rule| (direction, rule)))
    }

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.all
            .iter()
            .flatten()
            .chain(self.any.iter().flatten())
            .chain(self.relations().map(|(_, rule)| rule))
            .chain(self.not.iter().map(Box::as_ref))
            .chain(self.nth_child.as_ref().and_then(RawOrdinal::filter))
            .chain(match self.stop_by.as_ref() {
                Some(RawStopBy::Rule(rule)) => Some(rule.as_ref()),
                _ => None,
            })
    }
}

pub(super) struct CompiledRule {
    kind: Option<String>,
    nth_child: Option<Ordinal>,
    range: Option<ExactRange>,
    pattern: Option<CompiledPattern>,
    regex: Option<Regex>,
    relations: Vec<Relation>,
    all: Vec<Self>,
    any: Vec<Self>,
    not: Option<Box<Self>>,
    pub(super) candidate_plan: CandidatePlan,
}

/// The engine tag of a rule that does not parse as a rule (malformed YAML,
/// an unknown key, no matcher, a bad regex); a rule naming what this
/// grammar cannot compile is `structural.query.compileFailed`.
pub const RULE_INVALID: &str = "structural.rule.invalid";

pub(in crate::structural) fn parse_rule(rule: &str) -> Result<RawRule, CompileError> {
    #[cfg(test)]
    RULE_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    if rule.len() > 64_000 {
        return Err(CompileError::rule(
            "structural rule exceeds 64000 byte limit",
        ));
    }
    let value = crate::text::yaml_utils::yaml_to_json(rule).map_err(|err| yaml_error(&err))?;
    let mut raw = parse_rule_value(value)?;
    normalize_selectors(&mut raw, 0)?;
    validate_rule_depth(&raw, 0)?;
    Ok(raw)
}

pub fn validate_rule_shape(value: &serde_json::Value) -> Result<(), String> {
    parse_rule_value(value.clone())
        .map(|_| ())
        .map_err(|error| error.diagnostic().message)
}

fn parse_rule_value(value: serde_json::Value) -> Result<RawRule, CompileError> {
    let wrapped = value
        .as_object()
        .is_some_and(|mapping| mapping.contains_key("rule"));
    let raw: RawRule = if wrapped {
        serde_json::from_value::<RawRuleDocument>(value)
            .map_err(|err| yaml_error(&err))?
            .rule
    } else {
        serde_json::from_value(value).map_err(|err| yaml_error(&err))?
    };
    validate_rule_depth(&raw, 0)?;
    Ok(raw)
}

fn normalize_selectors(raw: &mut RawRule, depth: usize) -> Result<(), CompileError> {
    if depth >= 64 {
        return Err(CompileError::rule(
            "structural rule exceeds 64 nesting levels",
        ));
    }
    if let Some(kind) = &raw.kind
        && (kind.contains(['>', '+', '~', ',', ':']) || kind.split_whitespace().count() > 1)
    {
        let selector = super::selector::parse(kind)?;
        if selector.get("kind").and_then(serde_json::Value::as_str) != Some(kind) {
            let selected: RawRule =
                serde_json::from_value(selector).map_err(|err| yaml_error(&err))?;
            let field = raw.field.take();
            let stop_by = raw.stop_by.take();
            raw.kind = None;
            let remainder = std::mem::take(raw);
            *raw = if remainder.has_matcher() {
                RawRule {
                    all: Some(vec![remainder, selected]),
                    ..RawRule::default()
                }
            } else {
                selected
            };
            raw.field = field;
            raw.stop_by = stop_by;
        }
    }
    for child in [
        &mut raw.has,
        &mut raw.inside,
        &mut raw.precedes,
        &mut raw.follows,
        &mut raw.not,
    ]
    .into_iter()
    .flatten()
    {
        normalize_selectors(child, depth + 1)?;
    }
    for list in [&mut raw.all, &mut raw.any].into_iter().flatten() {
        for child in list {
            normalize_selectors(child, depth + 1)?;
        }
    }
    if let Some(RawStopBy::Rule(child)) = &mut raw.stop_by {
        normalize_selectors(child, depth + 1)?;
    }
    if let Some(child) = raw.nth_child.as_mut().and_then(RawOrdinal::filter_mut) {
        normalize_selectors(child, depth + 1)?;
    }
    Ok(())
}

impl RawRule {
    fn has_matcher(&self) -> bool {
        self.kind.is_some()
            || self.pattern.is_some()
            || self.regex.is_some()
            || self.nth_child.is_some()
            || self.range.is_some()
            || self.relations().next().is_some()
            || self.all.is_some()
            || self.any.is_some()
            || self.not.is_some()
    }
}

/// One valid rule object: how a relational key (`has`, `inside`) combines with
/// `kind` and `stopBy`. Every rule error that lists the accepted keys shows it.
pub const EXAMPLE_RULE: &str =
    r#"{"kind":"class_declaration","has":{"kind":"extends_clause","stopBy":"end"}}"#;

/// A rule that does not deserialize. An unknown key lists the keys the rule
/// takes; the example shows how they combine, since the usual mistake is a
/// key from another matcher (`field`, `constraints`) that has no equivalent.
fn yaml_error(err: &impl std::fmt::Display) -> CompileError {
    let message = format!("invalid rule YAML: {err}");
    // Neither serde-saphyr nor serde_json has a typed accessor for an
    // unknown-field error; serde's own wording is the one thing to match, and
    // this is the only such site.
    if message.contains("unknown field") {
        CompileError::rule(format!("{message}. Example rule: {EXAMPLE_RULE}"))
    } else {
        CompileError::rule(message)
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static RULE_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn validate_rule_depth(rule: &RawRule, depth: usize) -> Result<(), CompileError> {
    validate_rule_position(rule, depth, false)
}

fn validate_rule_position(
    rule: &RawRule,
    depth: usize,
    relation: bool,
) -> Result<(), CompileError> {
    if depth >= 64 {
        return Err(CompileError::rule(
            "structural rule exceeds 64 nesting levels",
        ));
    }
    if !rule.has_matcher() {
        return Err(CompileError::rule("rule must contain at least one matcher"));
    }
    if let Some(ordinal) = &rule.nth_child {
        ordinal.position().compile()?;
        if let Some(filter) = ordinal.filter() {
            validate_rule_position(filter, depth + 1, false)?;
        }
    }
    if let Some(range) = &rule.range {
        range.validate()?;
    }
    if !relation && (rule.field.is_some() || rule.stop_by.is_some()) {
        return Err(CompileError::rule(
            "field and stopBy are only valid on a relation",
        ));
    }
    for rules in [rule.all.as_ref(), rule.any.as_ref()].into_iter().flatten() {
        if rules.is_empty() || rules.len() > 50 {
            return Err(CompileError::rule(
                "all and any require between 1 and 50 rules",
            ));
        }
        for child in rules {
            validate_rule_position(child, depth + 1, false)?;
        }
    }
    for (direction, child) in rule.relations() {
        if matches!(direction, Direction::Next | Direction::Previous) && child.field.is_some() {
            return Err(CompileError::rule(
                "field is only supported in has and inside relations",
            ));
        }
        validate_rule_position(child, depth + 1, true)?;
    }
    if let Some(child) = &rule.not {
        validate_rule_position(child, depth + 1, false)?;
    }
    if let Some(RawStopBy::Rule(child)) = &rule.stop_by {
        validate_rule_position(child, depth + 1, false)?;
    }
    Ok(())
}

/// Every `kind` in `rule` and its sub-rules names a node of `language`.
fn check_kinds(language: &tree_sitter::Language, rule: &RawRule) -> Result<(), CompileError> {
    if let Some(kind) = rule.kind.as_deref()
        && named_kind_id(language, kind).is_none()
    {
        return Err(CompileError::grammar(format!(
            "unknown node kind '{kind}' for this language grammar"
        )));
    }
    rule.children()
        .try_for_each(|child| check_kinds(language, child))
}

impl CompiledRule {
    /// Accepts both the wrapped document form (`rule:\n  kind: ...`) and a bare
    /// rule (`kind: ...`). A top-level `rule` key is unambiguous: `RawRule` has
    /// no such field, so a bare rule can never contain one.
    #[cfg(test)]
    pub(super) fn new(lang: &AgLanguage, rule: &str) -> Result<Self, CompileError> {
        let raw = parse_rule(rule)?;
        Self::compile(lang, &raw)
    }

    /// Every `kind` of the rule, at any depth, is checked before any
    /// pattern parses: a kind this grammar lacks is named as itself, never
    /// hidden behind an error of a pattern beside it.
    pub(super) fn compile(lang: &AgLanguage, raw: &RawRule) -> Result<Self, CompileError> {
        check_kinds(&lang.tree_sitter_language(), raw)?;
        Self::compile_checked(lang, raw)
    }

    pub(super) fn compile_checked(lang: &AgLanguage, raw: &RawRule) -> Result<Self, CompileError> {
        let pattern = raw
            .pattern
            .as_ref()
            .map(|pattern| pattern.compile(lang))
            .transpose()?;
        let regex = raw
            .regex
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|err| CompileError::rule(format!("invalid rule regex: {err}")))?;
        let relations = raw
            .relations()
            .map(|(direction, rule)| Relation::compile(lang, direction, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let all = raw
            .all
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile_checked(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let any = raw
            .any
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile_checked(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let not = raw
            .not
            .as_deref()
            .map(|rule| Self::compile_checked(lang, rule).map(Box::new))
            .transpose()?;

        let mut compiled = Self {
            kind: raw.kind.clone(),
            nth_child: raw
                .nth_child
                .as_ref()
                .map(|raw| Ordinal::compile(lang, raw))
                .transpose()?,
            range: raw.range.clone(),
            pattern,
            regex,
            relations,
            all,
            any,
            not,
            candidate_plan: CandidatePlan::Any,
        };
        if compiled.is_empty() {
            return Err(CompileError::rule(
                "invalid rule: rule must contain at least one matcher",
            ));
        }
        compiled.candidate_plan = compiled.compute_candidate_plan();
        Ok(compiled)
    }

    fn is_empty(&self) -> bool {
        self.kind.is_none() && self.only_kind()
    }

    pub(super) fn simple_kind(&self) -> Option<&str> {
        let kind = self.kind.as_deref()?;
        self.only_kind().then_some(kind)
    }

    /// No matcher besides `kind` is set.
    fn only_kind(&self) -> bool {
        self.nth_child.is_none()
            && self.range.is_none()
            && self.pattern.is_none()
            && self.regex.is_none()
            && self.relations.is_empty()
            && self.all.is_empty()
            && self.any.is_empty()
            && self.not.is_none()
    }

    fn compute_candidate_plan(&self) -> CandidatePlan {
        let mut plan = CandidatePlan::Any;
        if let Some(kind) = &self.kind {
            plan = plan.intersect(CandidatePlan::from_kind(kind.clone()));
        }
        if let Some(pattern) = &self.pattern {
            plan = plan.intersect(pattern.candidate_plan().clone());
        }
        for rule in &self.all {
            plan = plan.intersect(rule.candidate_plan.clone());
        }
        if !self.any.is_empty() {
            let any_plan =
                CandidatePlan::union(self.any.iter().map(|rule| rule.candidate_plan.clone()));
            plan = plan.intersect(any_plan);
        }
        plan
    }

    pub(super) fn matches_candidate(&self, candidate: Node<'_>) -> bool {
        self.candidate_plan.matches(candidate)
    }

    /// `ancestors` is the candidate's ancestor chain (root first, parent
    /// last) when the caller already has it; `None` recomputes it once from
    /// the document root, and only if an `inside` matcher needs it.
    pub(super) fn matches<'t>(
        &self,
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        ExecutionError::check(document.deadline)?;
        if !self.matches_candidate(candidate) {
            return Ok(false);
        }
        if let Some(kind) = &self.kind
            && candidate.kind() != kind
        {
            return Ok(false);
        }
        // Every matcher binds into `captures` directly; on failure everything
        // bound since this checkpoint is rolled back, so a `false` result
        // always leaves the caller's environment unchanged.
        let checkpoint = captures.checkpoint();
        if self
            .range
            .as_ref()
            .is_some_and(|range| !range.matches(candidate, document))
        {
            return Ok(false);
        }
        if let Some(pattern) = &self.pattern
            && !pattern.matches(candidate, document.content, captures, document.deadline)?
        {
            captures.rollback(checkpoint);
            return Ok(false);
        }
        if let Some(regex) = &self.regex
            && !regex.is_match(node_text(candidate, document.content))
        {
            captures.rollback(checkpoint);
            return Ok(false);
        }
        if let Some(ordinal) = &self.nth_child
            && !ordinal.matches(candidate, ancestors, document, captures)?
        {
            captures.rollback(checkpoint);
            return Ok(false);
        }
        for relation in &self.relations {
            if !relation.matches(candidate, ancestors, document, captures)? {
                captures.rollback(checkpoint);
                return Ok(false);
            }
        }
        for rule in &self.all {
            if !rule.matches(candidate, ancestors, document, captures)? {
                captures.rollback(checkpoint);
                return Ok(false);
            }
        }
        if !self.any.is_empty() {
            let mut matched = false;
            for rule in &self.any {
                // A failed alternative rolls itself back (see above).
                if rule.matches(candidate, ancestors, document, captures)? {
                    matched = true;
                    break;
                }
            }
            if !matched {
                captures.rollback(checkpoint);
                return Ok(false);
            }
        }
        if let Some(rule) = &self.not {
            // `not` never contributes bindings, matched or not.
            let before_not = captures.checkpoint();
            let negated = rule.matches(candidate, ancestors, document, captures)?;
            captures.rollback(before_not);
            if negated {
                captures.rollback(checkpoint);
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub(super) struct Document<'a, 't> {
    pub(super) content: &'a str,
    pub(super) deadline: Instant,
    pub(super) root: Node<'t>,
    pub(super) line_index: &'a crate::text::utf8_offsets::LineIndex,
}

impl Relation {
    fn compile(
        lang: &AgLanguage,
        direction: Direction,
        raw: &RawRule,
    ) -> Result<Self, CompileError> {
        let language = lang.tree_sitter_language();
        let field = raw
            .field
            .as_deref()
            .map(|field| {
                if matches!(direction, Direction::Next | Direction::Previous) {
                    return Err(CompileError::rule(
                        "field is only supported in has and inside relations",
                    ));
                }
                language.field_id_for_name(field).ok_or_else(|| {
                    CompileError::grammar(format!(
                        "unknown field '{field}' for this language grammar"
                    ))
                })
            })
            .transpose()?;
        let stop = match raw.stop_by.as_ref() {
            None | Some(RawStopBy::Kind(StopKind::Neighbor)) => Stop::Neighbor,
            Some(RawStopBy::Kind(StopKind::End)) => Stop::End,
            Some(RawStopBy::Rule(rule)) => {
                Stop::Rule(Box::new(CompiledRule::compile_checked(lang, rule)?))
            }
        };
        Ok(Self {
            direction,
            rule: CompiledRule::compile_checked(lang, raw)?,
            field,
            stop,
        })
    }

    fn stops<'t>(
        &self,
        node: Node<'t>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        match &self.stop {
            Stop::Neighbor => Ok(true),
            Stop::End => Ok(false),
            Stop::Rule(rule) => {
                let checkpoint = captures.checkpoint();
                let matched = rule.matches(node, None, document, captures);
                captures.rollback(checkpoint);
                matched
            }
        }
    }

    fn accepts<'t>(
        &self,
        node: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        if !self.rule.matches(node, ancestors, document, captures)? {
            return Ok(false);
        }
        captures.capture_replace(
            SECONDARY_CAPTURE,
            node_text(node, document.content).to_owned(),
            raw_range(node),
        );
        Ok(true)
    }

    fn matches<'t>(
        &self,
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        match self.direction {
            Direction::Descendant => {
                let mut stack = if let Some(field) = self.field {
                    candidate
                        .children_by_field_id(field, &mut candidate.walk())
                        .filter(Node::is_named)
                        .collect::<Vec<_>>()
                } else {
                    named_children(candidate)
                };
                stack.reverse();
                while let Some(node) = stack.pop() {
                    ExecutionError::check(document.deadline)?;
                    if self.accepts(node, None, document, captures)? {
                        return Ok(true);
                    }
                    if !self.stops(node, document, captures)? {
                        stack.extend(named_children(node).into_iter().rev());
                    }
                }
            }
            Direction::Ancestor => {
                let owned;
                let chain = match ancestors {
                    Some(chain) => chain,
                    None => {
                        owned = ancestors_from_root(document.root, candidate, document.deadline)?;
                        owned.as_slice()
                    }
                };
                let mut branch = candidate;
                for index in (0..chain.len()).rev() {
                    ExecutionError::check(document.deadline)?;
                    let node = chain[index];
                    let in_field = self.field.is_none_or(|field| {
                        node.children_by_field_id(field, &mut node.walk())
                            .any(|child| child == branch)
                    });
                    if in_field && self.accepts(node, Some(&chain[..index]), document, captures)? {
                        return Ok(true);
                    }
                    if self.stops(node, document, captures)? {
                        break;
                    }
                    branch = node;
                }
            }
            Direction::Next | Direction::Previous => {
                // A cursor rooted at the candidate cannot leave its root.
                let owned;
                let chain = match ancestors {
                    Some(chain) => chain,
                    None => {
                        owned = ancestors_from_root(document.root, candidate, document.deadline)?;
                        owned.as_slice()
                    }
                };
                let Some(parent) = chain.last() else {
                    return Ok(false);
                };
                let mut cursor = parent.walk();
                if !cursor.goto_first_child() {
                    return Ok(false);
                }
                while cursor.node() != candidate {
                    ExecutionError::check(document.deadline)?;
                    if !cursor.goto_next_sibling() {
                        return Ok(false);
                    }
                }
                loop {
                    ExecutionError::check(document.deadline)?;
                    let advanced = match self.direction {
                        Direction::Next => cursor.goto_next_sibling(),
                        _ => cursor.goto_previous_sibling(),
                    };
                    if !advanced {
                        break;
                    }
                    let node = cursor.node();
                    if self.accepts(node, Some(chain), document, captures)? {
                        return Ok(true);
                    }
                    if self.stops(node, document, captures)? {
                        break;
                    }
                }
            }
        }
        Ok(false)
    }
}
