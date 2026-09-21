use std::time::Instant;

use regex::Regex;
use serde::Deserialize;
use tree_sitter::Node;

use crate::structural::language::AgLanguage;

use super::matching::{
    CandidatePlan, CaptureEnv, ExecutionError, SECONDARY_CAPTURE, named_children, node_text,
    raw_range,
};
use super::pattern::CompiledPattern;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuleDocument {
    rule: RawRule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::structural) struct RawRule {
    kind: Option<String>,
    pub(in crate::structural) pattern: Option<String>,
    regex: Option<String>,
    pub(in crate::structural) has: Option<Box<Self>>,
    pub(in crate::structural) inside: Option<Box<Self>>,
    pub(in crate::structural) all: Option<Vec<Self>>,
    pub(in crate::structural) any: Option<Vec<Self>>,
    not: Option<Box<Self>>,
    #[serde(rename = "stopBy")]
    stop_by: Option<RawStopBy>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum RawStopBy {
    End,
}

pub(super) struct CompiledRule {
    kind: Option<String>,
    pattern: Option<CompiledPattern>,
    regex: Option<Regex>,
    has: Option<Box<Self>>,
    inside: Option<Box<Self>>,
    all: Vec<Self>,
    any: Vec<Self>,
    not: Option<Box<Self>>,
    stop_by_end: bool,
    pub(super) candidate_plan: CandidatePlan,
}

pub(in crate::structural) fn parse_rule(rule: &str) -> Result<RawRule, String> {
    #[cfg(test)]
    RULE_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    if rule.len() > 64_000 {
        return Err("structural rule exceeds 64000 byte limit".to_owned());
    }
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(rule).map_err(|err| format!("invalid rule YAML: {err}"))?;
    let wrapped = value
        .as_mapping()
        .is_some_and(|mapping| mapping.contains_key("rule"));
    let raw: RawRule = if wrapped {
        serde_yaml_ng::from_value::<RawRuleDocument>(value)
            .map_err(|err| format!("invalid rule YAML: {err}"))?
            .rule
    } else {
        serde_yaml_ng::from_value(value).map_err(|err| format!("invalid rule YAML: {err}"))?
    };
    validate_rule_depth(&raw, 0)?;
    Ok(raw)
}

#[cfg(test)]
thread_local! {
    pub(super) static RULE_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn validate_rule_depth(rule: &RawRule, depth: usize) -> Result<(), String> {
    if depth >= 64 {
        return Err("structural rule exceeds 64 nesting levels".to_owned());
    }
    for child in rule
        .all
        .iter()
        .flatten()
        .chain(rule.any.iter().flatten())
        .chain(rule.has.iter().map(Box::as_ref))
        .chain(rule.inside.iter().map(Box::as_ref))
        .chain(rule.not.iter().map(Box::as_ref))
    {
        validate_rule_depth(child, depth + 1)?;
    }
    Ok(())
}

impl CompiledRule {
    /// Accepts both the wrapped document form (`rule:\n  kind: ...`) and a bare
    /// rule (`kind: ...`). A top-level `rule` key is unambiguous: `RawRule` has
    /// no such field, so a bare rule can never contain one.
    #[cfg(test)]
    pub(super) fn new(lang: &AgLanguage, rule: &str) -> Result<Self, String> {
        let raw = parse_rule(rule)?;
        Self::compile(lang, &raw)
    }

    pub(super) fn compile(lang: &AgLanguage, raw: &RawRule) -> Result<Self, String> {
        if let Some(kind) = raw.kind.as_deref() {
            let language = lang.tree_sitter_language();
            // ERROR is a built-in recovery node, outside the grammar's symbol table.
            if kind != "ERROR"
                && !(0..language.node_kind_count())
                    .any(|id| language.node_kind_for_id(id as u16) == Some(kind))
            {
                return Err(format!(
                    "unknown node kind '{kind}' for this language grammar"
                ));
            }
        }
        let pattern = raw
            .pattern
            .as_deref()
            .map(|pattern| CompiledPattern::new(lang, pattern))
            .transpose()?;
        let regex = raw
            .regex
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|err| format!("invalid rule regex: {err}"))?;
        let has = raw
            .has
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;
        let inside = raw
            .inside
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;
        let all = raw
            .all
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let any = raw
            .any
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let not = raw
            .not
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;

        let mut compiled = Self {
            kind: raw.kind.clone(),
            pattern,
            regex,
            has,
            inside,
            all,
            any,
            not,
            stop_by_end: raw.stop_by == Some(RawStopBy::End),
            candidate_plan: CandidatePlan::Any,
        };
        if compiled.is_empty() {
            return Err("invalid rule: rule must contain at least one matcher".to_string());
        }
        compiled.candidate_plan = compiled.compute_candidate_plan();
        Ok(compiled)
    }

    fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.pattern.is_none()
            && self.regex.is_none()
            && self.has.is_none()
            && self.inside.is_none()
            && self.all.is_empty()
            && self.any.is_empty()
            && self.not.is_none()
    }

    pub(super) fn simple_kind(&self) -> Option<&str> {
        let kind = self.kind.as_deref()?;
        (self.pattern.is_none()
            && self.regex.is_none()
            && self.has.is_none()
            && self.inside.is_none()
            && self.all.is_empty()
            && self.any.is_empty()
            && self.not.is_none())
        .then_some(kind)
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

    pub(super) fn matches(
        &self,
        candidate: Node<'_>,
        document: &Document<'_>,
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
        if let Some(pattern) = &self.pattern
            && !pattern.matches(candidate, document.content, captures, document.deadline)?
        {
            return Ok(false);
        }
        if let Some(regex) = &self.regex
            && !regex.is_match(node_text(candidate, document.content))
        {
            return Ok(false);
        }
        if let Some(rule) = &self.has {
            let mut branch = captures.clone();
            if !matches_descendant(rule, candidate, document, &mut branch, 0)? {
                return Ok(false);
            }
            *captures = branch;
        }
        if let Some(rule) = &self.inside {
            let mut branch = captures.clone();
            if !matches_ancestor(rule, candidate, document, &mut branch)? {
                return Ok(false);
            }
            *captures = branch;
        }
        for rule in &self.all {
            let mut branch = captures.clone();
            if !rule.matches(candidate, document, &mut branch)? {
                return Ok(false);
            }
            *captures = branch;
        }
        if !self.any.is_empty() {
            let mut matched = None;
            for rule in &self.any {
                let mut branch = captures.clone();
                if rule.matches(candidate, document, &mut branch)? {
                    matched = Some(branch);
                    break;
                }
            }
            let Some(branch) = matched else {
                return Ok(false);
            };
            *captures = branch;
        }
        if let Some(rule) = &self.not {
            let mut branch = captures.clone();
            if rule.matches(candidate, document, &mut branch)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub(super) struct Document<'a> {
    pub(super) content: &'a str,
    pub(super) deadline: Instant,
}

fn matches_descendant(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
    _depth: usize,
) -> Result<bool, ExecutionError> {
    let mut stack = named_children(candidate);
    stack.reverse();
    while let Some(child) = stack.pop() {
        ExecutionError::check(document.deadline)?;
        if rule.matches_candidate(child) {
            let mut branch = captures.clone();
            if rule.matches(child, document, &mut branch)? {
                branch.capture_replace(
                    SECONDARY_CAPTURE,
                    node_text(child, document.content).to_owned(),
                    raw_range(child),
                );
                *captures = branch;
                return Ok(true);
            }
        }
        if rule.stop_by_end {
            stack.extend(named_children(child).into_iter().rev());
        }
    }
    Ok(false)
}

fn matches_ancestor(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
) -> Result<bool, ExecutionError> {
    let mut parent = candidate.parent();
    while let Some(node) = parent {
        ExecutionError::check(document.deadline)?;
        if rule.matches_candidate(node) {
            let mut branch = captures.clone();
            if rule.matches(node, document, &mut branch)? {
                branch.capture_replace(
                    SECONDARY_CAPTURE,
                    node_text(node, document.content).to_owned(),
                    raw_range(node),
                );
                *captures = branch;
                return Ok(true);
            }
        }
        if !rule.stop_by_end {
            return Ok(false);
        }
        parent = node.parent();
    }
    Ok(false)
}
