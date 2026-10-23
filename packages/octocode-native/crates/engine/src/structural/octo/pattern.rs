use std::collections::HashSet;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use super::line_index_support::to_structural_match_with_index;
use super::matching::{
    CandidatePlan, CaptureEnv, ExecutionError, Found, named_children, node_text,
    parse_tree_with_deadline, raw_range, visit_named,
};
use super::metavars::meta_from_node;
use super::recovery::Recovery;
use crate::structural::language::Expando;
use crate::text::utf8_offsets::LineIndex;

mod compile;
mod node_match;

use compile::effective_pattern_root;

/// Budget on `$$$` multi-metavar split attempts within a single top-level
/// child-list match. Each `$$$` tries every split point (`for take in
/// 0..=max_take`) and several `$$$` against a wide node are combinatorial; this
/// caps the total work so a crafted pattern + wide input can't stall for
/// seconds. Exhaustion is an explicit incomplete execution error.
const MAX_MULTI_CAPTURE_ATTEMPTS: usize = 10_000;

pub(super) struct MatchBudget {
    attempts: usize,
    deadline: Instant,
}

/// See [`CompiledPattern::declaration`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternDeclaration {
    /// The declaration's node kind (`function_item`).
    pub kind: String,
    /// The name node's kind and text, when the pattern spells a name.
    pub name: Option<(String, String)>,
}

pub(super) struct CompiledPattern {
    language: Language,
    expando: Expando,
    source: String,
    tree: Option<Tree>,
    special: Option<SpecialPattern>,
    candidate_plan: CandidatePlan,
    selected_node: Option<(usize, usize, u16)>,
}

enum SpecialPattern {
    /// `<$T>` over a JSX-capable grammar: opening and self-closing elements.
    JsxTagName { capture: String },
    KeyValuePair {
        key_capture: String,
        value_capture: String,
    },
}

impl CompiledPattern {
    fn root(&self) -> Option<Node<'_>> {
        let tree = self.tree.as_ref()?;
        let Some((start, end, kind)) = self.selected_node else {
            return Some(effective_pattern_root(tree.root_node(), &self.source));
        };
        let mut node = tree
            .root_node()
            .named_descendant_for_byte_range(start, end)?;
        while node.kind_id() != kind || node.start_byte() != start || node.end_byte() != end {
            node = node.parent()?;
        }
        Some(node)
    }

    pub(super) fn language(&self) -> &Language {
        &self.language
    }

    /// The declaration a pattern spells: its root's kind when the grammar
    /// gives that node a `name` field, with the name node's kind and text
    /// unless the name is a metavariable. `None` for any other pattern.
    pub(super) fn declaration(&self) -> Option<PatternDeclaration> {
        let root = self.root()?;
        let name = root.child_by_field_name("name")?;
        let named = meta_from_node(name, &self.source, self.expando)
            .is_none()
            .then(|| {
                (
                    name.kind().to_owned(),
                    node_text(name, &self.source).to_owned(),
                )
            });
        Some(PatternDeclaration {
            kind: root.kind().to_owned(),
            name: named,
        })
    }

    pub(super) fn is_special(&self) -> bool {
        self.special.is_some()
    }

    pub(super) fn candidate_plan(&self) -> &CandidatePlan {
        &self.candidate_plan
    }

    pub(super) fn matches_candidate(&self, candidate: Node<'_>) -> bool {
        self.candidate_plan.matches(candidate)
    }

    pub(super) fn find_special_matches(
        &self,
        content: &str,
        deadline: Instant,
        recovery: &Recovery,
        found: &mut Found<'_>,
    ) -> Result<(), ExecutionError> {
        let Some(special) = &self.special else {
            return Ok(());
        };
        let tree = parse_tree_with_deadline(&self.language, content, deadline)?;
        if recovery.hides_match(&tree, content) {
            found.mark_recovered();
        }

        // One match per node span.
        let mut seen = HashSet::new();
        let line_index = LineIndex::tree_sitter(content);
        visit_named(tree.root_node(), deadline, &mut |candidate| {
            if !self.matches_candidate(candidate) {
                return Ok(());
            }
            // Direct patterns and pattern rules share capture equality semantics.
            let mut captures = CaptureEnv::default();
            if self.matches_special(special, candidate, content, &mut captures)
                && seen.insert((candidate.start_byte(), candidate.end_byte()))
            {
                found.push(candidate, || {
                    let (metavars, metavar_ranges_raw) = captures.into_maps();
                    to_structural_match_with_index(
                        candidate,
                        content,
                        &line_index,
                        metavars,
                        metavar_ranges_raw,
                    )
                });
            }
            Ok(())
        })
    }

    pub(super) fn matches(
        &self,
        candidate: Node<'_>,
        content: &str,
        captures: &mut CaptureEnv,
        deadline: Instant,
    ) -> Result<bool, ExecutionError> {
        if let Some(special) = &self.special {
            return Ok(self.matches_special(special, candidate, content, captures));
        }

        let Some(root) = self.root() else {
            return Ok(false);
        };
        let mut budget = MatchBudget {
            attempts: MAX_MULTI_CAPTURE_ATTEMPTS,
            deadline,
        };
        self.match_node(
            root,
            &self.source,
            candidate,
            content,
            captures,
            0,
            &mut budget,
        )
    }

    fn matches_special(
        &self,
        special: &SpecialPattern,
        candidate: Node<'_>,
        content: &str,
        captures: &mut CaptureEnv,
    ) -> bool {
        match special {
            SpecialPattern::JsxTagName { capture } => {
                let Some(tag_name) = jsx_tag_name_node(candidate) else {
                    return false;
                };
                captures.capture_one(capture, node_text(tag_name, content), raw_range(tag_name))
            }
            SpecialPattern::KeyValuePair {
                key_capture,
                value_capture,
            } => {
                let Some((key, value)) = key_value_nodes(candidate) else {
                    return false;
                };
                let checkpoint = captures.checkpoint();
                let matched =
                    captures.capture_one(key_capture, node_text(key, content), raw_range(key))
                        && captures.capture_one(
                            value_capture,
                            node_text(value, content),
                            raw_range(value),
                        );
                if !matched {
                    captures.rollback(checkpoint);
                }
                matched
            }
        }
    }
}

/// Opening/self-closing JSX elements: the nodes `<$T>` matches.
const JSX_TAG_KINDS: [&str; 2] = ["jsx_opening_element", "jsx_self_closing_element"];

fn jsx_tag_name_node(candidate: Node<'_>) -> Option<Node<'_>> {
    if !JSX_TAG_KINDS.contains(&candidate.kind()) {
        return None;
    }
    candidate.child_by_field_name("name")
}

fn key_value_nodes(candidate: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if candidate.kind() != "pair" {
        return None;
    }
    let named = named_children(candidate);
    match named.as_slice() {
        [key, value, ..] => Some((*key, *value)),
        _ => None,
    }
}
