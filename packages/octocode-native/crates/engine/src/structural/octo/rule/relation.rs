//! Relational rules (`has`, `inside`, `precedes`, `follows`): their
//! direction, field, stop rule and matching.

use std::num::NonZeroU16;

use tree_sitter::{Node, TreeCursor};

use crate::structural::kinds::near_fields;
use crate::structural::language::AgLanguage;

use crate::structural::octo::error::CompileError;
use crate::structural::octo::matching::{CaptureEnv, ExecutionError, ancestors_from_root};

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
    field: Option<NonZeroU16>,
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
                        "unknown field '{field}' for this language grammar{}",
                        near_fields(&language, field)
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

    pub(super) fn matches<'t>(
        &self,
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        match self.direction {
            Direction::Descendant => {
                return walk_descendants(candidate, self.field, |node| {
                    ExecutionError::check(document.deadline)?;
                    Ok(if self.rule.matches(node, None, document, captures)? {
                        Visit::Found
                    } else if self.stops(node, document, captures)? {
                        Visit::Skip
                    } else {
                        Visit::Descend
                    })
                });
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
                    if in_field
                        && self
                            .rule
                            .matches(node, Some(&chain[..index]), document, captures)?
                    {
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
                    if self.rule.matches(node, Some(chain), document, captures)? {
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

/// What [`walk_descendants`] does after visiting a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visit {
    /// The relation holds: end the walk.
    Found,
    /// Visit this node's named children next.
    Descend,
    /// Leave this node's subtree unvisited.
    Skip,
}

/// Visit `candidate`'s named descendants in pre-order (source order), the
/// first level limited to children in `field`. One cursor walks the whole
/// subtree, so no node list is allocated per visited node. Returns whether a
/// visit reported [`Visit::Found`].
fn walk_descendants<'t>(
    candidate: Node<'t>,
    field: Option<NonZeroU16>,
    mut visit: impl FnMut(Node<'t>) -> Result<Visit, ExecutionError>,
) -> Result<bool, ExecutionError> {
    // The cursor's depth counts from `candidate` (depth 0), so the walk never
    // leaves its subtree.
    let mut cursor = candidate.walk();
    if !goto_first_admitted_child(&mut cursor, field) {
        return Ok(false);
    }
    loop {
        match visit(cursor.node())? {
            Visit::Found => return Ok(true),
            Visit::Descend if goto_first_admitted_child(&mut cursor, field) => continue,
            Visit::Descend | Visit::Skip => {}
        }
        while !goto_next_admitted_sibling(&mut cursor, field) {
            if !cursor.goto_parent() || cursor.depth() == 0 {
                return Ok(false);
            }
        }
    }
}

/// Whether the walk visits the cursor's node: a named node, and on the first
/// level one in `field` when a field is set.
fn admitted(cursor: &TreeCursor<'_>, field: Option<NonZeroU16>) -> bool {
    cursor.node().is_named()
        && (cursor.depth() != 1 || field.is_none_or(|field| cursor.field_id() == Some(field)))
}

/// Move to the first admitted child; the cursor stays put when there is none.
fn goto_first_admitted_child(cursor: &mut TreeCursor<'_>, field: Option<NonZeroU16>) -> bool {
    if !cursor.goto_first_child() {
        return false;
    }
    if admitted(cursor, field) || goto_next_admitted_sibling(cursor, field) {
        return true;
    }
    cursor.goto_parent();
    false
}

/// Move to the next admitted sibling; on `false` the cursor is on some later
/// sibling of the same parent.
fn goto_next_admitted_sibling(cursor: &mut TreeCursor<'_>, field: Option<NonZeroU16>) -> bool {
    while cursor.goto_next_sibling() {
        if admitted(cursor, field) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
