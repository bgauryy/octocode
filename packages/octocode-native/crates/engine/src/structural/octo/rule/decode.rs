//! Decoding a rule object: deserialize, normalize selectors and check its
//! shape and kinds before anything compiles.

use serde::Deserialize;

use crate::structural::kinds::named_kind_id;
use crate::structural::language::AgLanguage;

use crate::structural::octo::error::CompileError;
use crate::structural::octo::pattern::CompiledPattern;
use crate::structural::octo::position::RawOrdinal;

use super::relation::Direction;
use super::{EXAMPLE_RULE, RawPattern, RawRule, RawStopBy};

impl RawPattern {
    pub(in crate::structural) fn literal(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            // The wrapper is not required in the searched source.
            Self::Context(_) => None,
        }
    }

    pub(super) fn compile(&self, lang: &AgLanguage) -> Result<CompiledPattern, CompileError> {
        match self {
            Self::Text(text) => CompiledPattern::new(lang, text),
            Self::Context(pattern) => {
                CompiledPattern::contextual(lang, &pattern.context, &pattern.selector)
            }
        }
    }
}

impl RawRule {
    pub(super) fn relations(&self) -> impl Iterator<Item = (Direction, &Self)> {
        [
            (Direction::Descendant, self.has.as_deref()),
            (Direction::Ancestor, self.inside.as_deref()),
            (Direction::Next, self.precedes.as_deref()),
            (Direction::Previous, self.follows.as_deref()),
        ]
        .into_iter()
        .filter_map(|(direction, rule)| rule.map(|rule| (direction, rule)))
    }

    pub(super) fn children(&self) -> impl Iterator<Item = &Self> {
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

/// The one decode of a rule object: typed, selector-normalized and
/// depth-checked.
pub(in crate::structural) fn decode_rule(
    value: &serde_json::Value,
) -> Result<RawRule, CompileError> {
    #[cfg(test)]
    RULE_DECODE_COUNT.with(|count| count.set(count.get() + 1));
    let mut raw = RawRule::deserialize(value).map_err(|err| rule_error(&err))?;
    normalize_selectors(&mut raw, 0)?;
    validate_rule_position(&raw, 0, false)?;
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
        let selector = crate::structural::octo::selector::parse(kind)?;
        if selector.get("kind").and_then(serde_json::Value::as_str) != Some(kind) {
            let selected: RawRule =
                RawRule::deserialize(&selector).map_err(|err| rule_error(&err))?;
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
    pub(super) fn has_matcher(&self) -> bool {
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

/// A rule that does not deserialize. An unknown key lists the keys the rule
/// takes; the example shows how they combine, since the usual mistake is a
/// key from another matcher (`field`, `constraints`) that has no equivalent.
fn rule_error(err: &impl std::fmt::Display) -> CompileError {
    let message = format!("invalid rule: {err}");
    // serde_json has no typed accessor for an unknown-field error; serde's own
    // wording is the one thing to match, and this is the only such site.
    if message.contains("unknown field") {
        CompileError::rule(format!("{message}. Example rule: {EXAMPLE_RULE}"))
    } else {
        CompileError::rule(message)
    }
}

#[cfg(test)]
thread_local! {
    pub(in crate::structural::octo) static RULE_DECODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
pub(super) fn check_kinds(
    language: &tree_sitter::Language,
    rule: &RawRule,
) -> Result<(), CompileError> {
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
