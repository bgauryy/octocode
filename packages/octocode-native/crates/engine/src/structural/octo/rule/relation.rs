//! Relational rules (`has`, `inside`, `precedes`, `follows`): their
//! direction, field, stop rule and matching.

use tree_sitter::Node;

use crate::structural::language::AgLanguage;

use crate::structural::octo::error::CompileError;
use crate::structural::octo::matching::{
    CaptureEnv, ExecutionError, SECONDARY_CAPTURE, ancestors_from_root, named_children, node_text,
    raw_range,
};

use super::{CompiledRule, Document, RawRule, RawStopBy, StopKind};

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Descendant,
    Ancestor,
    Next,
    Previous,
}

pub(super) struct Relation {
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

impl Relation {
    pub(super) fn compile(
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

    pub(super) fn matches<'t>(
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
