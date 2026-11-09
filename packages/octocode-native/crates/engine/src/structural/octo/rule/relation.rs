//! Relational rules (`has`, `inside`, `precedes`, `follows`): their
//! direction, field, stop rule and matching.

use std::num::NonZeroU16;

use tree_sitter::Node;

use crate::structural::kinds::near_fields;
use crate::structural::language::AgLanguage;

use crate::structural::octo::error::CompileError;
use crate::structural::octo::expanded::Visit;
use crate::structural::octo::matching::{CaptureEnv, ExecutionError};

use super::{CompiledRule, Document, RawRule, RawStopBy, RelationMemo, StopKind};

#[cfg(test)]
thread_local! {
    /// Ancestors an `inside` relation read in this thread (a deep-stack run
    /// adds its own): the work its answers cost, so a test asserts how it
    /// grows with depth instead of timing it.
    pub(in crate::structural::octo) static ANCESTOR_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn note_ancestor_steps(_steps: usize) {
    #[cfg(test)]
    ANCESTOR_STEPS.with(|total| total.set(total.get() + _steps));
}

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
    /// Whether its answer for a node is the same whoever asks, so a file's
    /// walk keeps it ([`RelationMemo`]): it looks through every level of a
    /// direction, and neither its rule nor its stop reads a capture.
    keeps_answers: bool,
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
        let rule = CompiledRule::compile_checked(lang, raw)?;
        let keeps_answers = matches!(direction, Direction::Descendant | Direction::Ancestor)
            && field.is_none()
            && matches!(stop, Stop::End)
            && rule.is_capture_free();
        Ok(Self {
            direction,
            rule,
            field,
            stop,
            keeps_answers,
        })
    }

    pub(super) fn is_capture_free(&self) -> bool {
        self.rule.is_capture_free()
            && match &self.stop {
                Stop::Rule(rule) => rule.is_capture_free(),
                Stop::Neighbor | Stop::End => true,
            }
    }

    /// The file's memo when this relation keeps its answers in it.
    fn memo<'d>(&self, document: &Document<'d, '_>) -> Option<&'d RelationMemo> {
        document.memo.filter(|_| self.keeps_answers)
    }

    fn key(&self) -> usize {
        std::ptr::from_ref(self) as usize
    }

    /// `candidate`'s ancestor chain followed by `candidate`: the chain a walk
    /// below it starts from.
    fn chain_through<'t>(
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
    ) -> Result<Vec<Node<'t>>, ExecutionError> {
        let mut chain = match ancestors {
            Some(chain) => chain.to_vec(),
            None => document.tree.ancestors(candidate, document.deadline)?,
        };
        chain.push(candidate);
        Ok(chain)
    }

    /// `has` through every level, from the answers the file's walk holds: a
    /// descendant already known to hold (or not) ends (or skips) the walk
    /// there, and what this walk learns is kept for every node it passed.
    fn has_below<'t>(
        &self,
        candidate: Node<'t>,
        ancestors: Option<&[Node<'t>]>,
        document: &Document<'_, 't>,
        memo: &RelationMemo,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        if let Some(known) = memo.get(self.key(), candidate) {
            return Ok(known);
        }
        let chain = Self::chain_through(candidate, ancestors, document)?;
        // The nodes from the candidate down to a visited node's parent sit
        // on its chain past `base`.
        let base = chain.len();
        let mut descended: Vec<Node<'t>> = Vec::new();
        let mut holding: Vec<Node<'t>> = Vec::new();
        let found = document.tree.walk_below(
            candidate,
            chain,
            None,
            document.deadline,
            |node, chain| {
                let path = &chain[base..];
                if self.rule.matches(node, Some(chain), document, captures)? {
                    holding.extend_from_slice(path);
                    return Ok(Visit::Found);
                }
                match memo.get(self.key(), node) {
                    Some(true) => {
                        holding.extend_from_slice(path);
                        holding.push(node);
                        Ok(Visit::Found)
                    }
                    Some(false) => Ok(Visit::Skip),
                    None => {
                        descended.push(node);
                        Ok(Visit::Descend)
                    }
                }
            },
        )?;
        memo.set(self.key(), candidate, found);
        let (nodes, holds) = if found {
            (holding, true)
        } else {
            (descended, false)
        };
        for node in nodes {
            memo.set(self.key(), node, holds);
        }
        Ok(found)
    }

    /// `inside` through every level, from the answers the file's walk holds:
    /// an ancestor already known settles every node below it on the chain.
    fn inside_above<'t>(
        &self,
        candidate: Node<'t>,
        chain: &[Node<'t>],
        document: &Document<'_, 't>,
        memo: &RelationMemo,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        if let Some(known) = memo.get(self.key(), candidate) {
            return Ok(known);
        }
        // The nodes below `settled` on the chain, and the candidate, share
        // the answer found at or above it.
        let mut settled = chain.len();
        let mut answer = false;
        for index in (0..chain.len()).rev() {
            ExecutionError::check(document.deadline)?;
            note_ancestor_steps(1);
            let node = chain[index];
            if self
                .rule
                .matches(node, Some(&chain[..index]), document, captures)?
            {
                answer = true;
                break;
            }
            if let Some(known) = memo.get(self.key(), node) {
                answer = known;
                break;
            }
            settled = index;
        }
        memo.set(self.key(), candidate, answer);
        for node in &chain[settled..] {
            memo.set(self.key(), *node, answer);
        }
        Ok(answer)
    }

    /// Whether the walk stops at `node`, whose ancestor chain is `chain`.
    fn stops<'t>(
        &self,
        node: Node<'t>,
        chain: &[Node<'t>],
        document: &Document<'_, 't>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        match &self.stop {
            Stop::Neighbor => Ok(true),
            Stop::End => Ok(false),
            Stop::Rule(rule) => {
                let checkpoint = captures.checkpoint();
                let matched = rule.matches(node, Some(chain), document, captures);
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
                if let Some(memo) = self.memo(document) {
                    return self.has_below(candidate, ancestors, document, memo, captures);
                }
                let chain = Self::chain_through(candidate, ancestors, document)?;
                return document.tree.walk_below(
                    candidate,
                    chain,
                    self.field,
                    document.deadline,
                    |node, chain| {
                        Ok(
                            if self.rule.matches(node, Some(chain), document, captures)? {
                                Visit::Found
                            } else if self.stops(node, chain, document, captures)? {
                                Visit::Skip
                            } else {
                                Visit::Descend
                            },
                        )
                    },
                );
            }
            Direction::Ancestor => {
                let owned;
                let chain = match ancestors {
                    Some(chain) => chain,
                    None => {
                        owned = document.tree.ancestors(candidate, document.deadline)?;
                        note_ancestor_steps(owned.len());
                        owned.as_slice()
                    }
                };
                if let Some(memo) = self.memo(document) {
                    return self.inside_above(candidate, chain, document, memo, captures);
                }
                let mut branch = candidate;
                for index in (0..chain.len()).rev() {
                    ExecutionError::check(document.deadline)?;
                    let node = chain[index];
                    let in_field = self.field.is_none_or(|field| {
                        node.children_by_field_id(field, &mut node.walk())
                            .any(|child| child == branch)
                    });
                    let above = &chain[..index];
                    if in_field && self.rule.matches(node, Some(above), document, captures)? {
                        return Ok(true);
                    }
                    if self.stops(node, above, document, captures)? {
                        break;
                    }
                    branch = node;
                }
            }
            Direction::Next | Direction::Previous => {
                let owned;
                let chain = match ancestors {
                    Some(chain) => chain,
                    None => {
                        owned = document.tree.ancestors(candidate, document.deadline)?;
                        owned.as_slice()
                    }
                };
                let Some(parent) = chain.last() else {
                    return Ok(false);
                };
                let siblings = document.tree.children(*parent);
                let Some(at) = siblings.iter().position(|sibling| *sibling == candidate) else {
                    return Ok(false);
                };
                let toward: Box<dyn Iterator<Item = &Node<'t>>> = match self.direction {
                    Direction::Next => Box::new(siblings[at + 1..].iter()),
                    _ => Box::new(siblings[..at].iter().rev()),
                };
                for node in toward {
                    ExecutionError::check(document.deadline)?;
                    if self.rule.matches(*node, Some(chain), document, captures)? {
                        return Ok(true);
                    }
                    if self.stops(*node, chain, document, captures)? {
                        break;
                    }
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
