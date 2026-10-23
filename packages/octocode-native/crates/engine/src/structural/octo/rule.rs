use std::time::Instant;

use regex::Regex;
use serde::Deserialize;
use tree_sitter::Node;

use crate::structural::language::AgLanguage;

use super::error::CompileError;
use super::matching::{CandidatePlan, CaptureEnv, ExecutionError, node_text};
use super::pattern::CompiledPattern;
use super::position::{ExactRange, Ordinal, RawOrdinal};

mod decode;
mod relation;

#[cfg(test)]
pub(super) use decode::RULE_DECODE_COUNT;
use decode::check_kinds;
pub(in crate::structural) use decode::decode_rule;
pub use decode::validate_rule_shape;
use relation::Relation;

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

/// The engine tag of a rule that does not decode as a rule (an unknown key,
/// no matcher, a bad regex); a rule naming what this grammar cannot compile
/// is `structural.query.compileFailed`.
pub const RULE_INVALID: &str = "structural.rule.invalid";

/// One valid rule object: how a relational key (`has`, `inside`) combines with
/// `kind` and `stopBy`. Every rule error that lists the accepted keys shows it.
pub const EXAMPLE_RULE: &str =
    r#"{"kind":"class_declaration","has":{"kind":"extends_clause","stopBy":"end"}}"#;

impl CompiledRule {
    #[cfg(test)]
    pub(super) fn new(lang: &AgLanguage, rule: &serde_json::Value) -> Result<Self, CompileError> {
        let raw = decode_rule(rule)?;
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
