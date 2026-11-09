use std::collections::HashSet;

use tree_sitter::{Language, Node, Tree};

use super::line_index_support::to_structural_match_with_context;
use super::matching::{
    CandidatePlan, CaptureEnv, Deadline, ExecutionError, Found, named_children, node_text,
    parse_tree_with_deadline, raw_range, visit_named,
};
use super::metavars::meta_from_node;
use super::recovery::Recovery;
use super::source_context::SourceContext;
use crate::structural::language::Expando;
use crate::text::LineIndex;

mod compile;
mod node_match;
mod qualified;
mod shape;
mod sigils;
mod unsaid;

use compile::effective_pattern_root;
pub use qualified::{QualifiedPattern, Separator};
pub use sigils::{LiteralSigil, SigilForm, SigilRepair};
use unsaid::declares;

/// Budget on `$$$` multi-metavar split attempts within a single top-level
/// child-list match. Each `$$$` tries every split point (`for take in
/// 0..=max_take`) and several `$$$` against a wide node are combinatorial; this
/// caps the total work so a crafted pattern + wide input can't stall for
/// seconds. Exhaustion is an explicit incomplete execution error.
const MAX_MULTI_CAPTURE_ATTEMPTS: usize = 10_000;

pub(super) struct MatchBudget {
    attempts: usize,
    deadline: Deadline,
}

/// See [`CompiledPattern::declaration`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternDeclaration {
    /// The declaration's node kind (`function_item`).
    pub kind: String,
    /// The name node's kind and text, when the pattern spells a name.
    pub name: Option<(String, String)>,
    /// The kind of the node that wraps the declaration, when the pattern spells
    /// one around it (`export_statement` for `export function $N() {}`).
    pub inside: Option<String>,
    /// The grammar's fuller forms of this declaration kind: kinds whose
    /// name words are a strict subsequence of this kind's (`function_item`
    /// for `function_signature_item`). A pattern of a smaller form matches
    /// none of them.
    pub fuller_kinds: Vec<String>,
    /// The other kinds the pattern matches as this declaration: the bodied
    /// form of a bodiless one, the member form of a local one.
    pub alternate_kinds: Vec<String>,
}

/// Nodes a grammar names by a `name` field that refer to a declaration
/// rather than declare one: a Java method call, a `::` path. The rest of
/// the name-only nodes (`struct $S;`) count as declarations for a near miss.
const REFERENCE_KINDS: [&str; 4] = [
    "method_invocation",
    "scoped_identifier",
    "scoped_type_identifier",
    "qualified_identifier",
];

pub(super) struct CompiledPattern {
    language: Language,
    expando: Expando,
    source: String,
    tree: Option<Tree>,
    special: Option<SpecialPattern>,
    candidate_plan: CandidatePlan,
    selected_node: Option<(usize, usize, u16)>,
    /// Where in `source` the terminator compilation appended to give the
    /// pattern its context starts (`impl X for $T` parses only as
    /// `impl X for $T;`): the caller did not write it, so no candidate
    /// needs it.
    unwritten_terminator: Option<usize>,
    /// The pattern's nodes that also match other kinds ([`Alternate`]).
    alternates: Vec<Alternate>,
    /// The `;` the caller wrote after the root, outside it, when there is one.
    written_terminator: Option<WrittenTerminator>,
    /// Whether the grammar reads a token as the infix separator of a kind
    /// in a pattern node's place ([`node_match`]'s leading separators), by
    /// pattern node, kind and token: each answer costs a parse.
    separators: std::sync::Mutex<std::collections::HashMap<(NodeKey, u16, u16), bool>>,
}

/// A pattern node by where it is in the compiled source and its kind.
type NodeKey = (usize, usize, u16);

fn node_key(node: Node<'_>) -> NodeKey {
    (node.start_byte(), node.end_byte(), node.kind_id())
}

/// A pattern node the grammar reads as another kind for the same spelled
/// parts, so it matches that kind too (and the grammar's longer forms of it,
/// [`unsaid::adds_a_leading_word`]):
///
/// - the kind it reads as with a body in place of the terminator the caller
///   did not write (`function_item` for `fn $F()`, whose node is a
///   `function_signature_item`): the pattern spells neither;
/// - the kind its text reads as in a class body (a Java or C# field for
///   `int $A = $B`, a local variable): a declaration reads the same wherever
///   the grammar declares it.
struct Alternate {
    node: NodeKey,
    kinds: Vec<u16>,
}

/// A `;` the caller wrote after the pattern's root, outside it (in the
/// statement or declaration the grammar wraps it in: `foo($X);`, C
/// `struct $S;`). A pattern requires what the caller wrote, so it matches
/// only a node that ends there too: one followed by a `;` in a wrapper of
/// the same kind (any, when the pattern's wrapper is the file), whose last
/// part is in the place the pattern's is (not a body after `struct S`). A
/// statement the grammar ends without one (TypeScript's automatic `;`, a
/// Rust block's tail expression) is not it; a pattern that writes no `;`
/// matches both. A `;` inside the root (a Rust unit struct, a Java local)
/// is one of its parts, required like any other. Where the grammar makes
/// the `;` implicit (Go, Python), the tree holds none to require, so the
/// written one asks for nothing more.
struct WrittenTerminator {
    root: NodeKey,
    /// The wrapper's kind; `None` when it is the file.
    parent: Option<u16>,
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

    /// The kind of node the pattern parsed as; `None` for a special pattern.
    pub(super) fn root_kind(&self) -> Option<String> {
        self.root().map(|root| root.kind().to_owned())
    }

    pub(super) fn language(&self) -> &Language {
        &self.language
    }

    /// The declaration a pattern spells: its root when [`declares`] says it is
    /// one or the grammar names it by a `name` field, with the name node's kind
    /// and text unless the name is a metavariable. A root that holds a
    /// `declaration` and is none itself wraps it, and the wrapped node is the
    /// declaration. `None` for any other pattern, including the calls and paths
    /// a grammar also names by `name`.
    pub(super) fn declaration(&self) -> Option<PatternDeclaration> {
        let root = self.root()?;
        let is_declaration = |node: Node<'_>| {
            declares(node)
                || (node.child_by_field_name("name").is_some()
                    && !REFERENCE_KINDS.contains(&node.kind()))
        };
        let wrapped = root
            .child_by_field_name("declaration")
            .filter(|_| !is_declaration(root));
        let declared = wrapped.unwrap_or(root);
        if !is_declaration(declared) {
            return None;
        }
        let named = declared
            .child_by_field_name("name")
            .filter(|name| meta_from_node(*name, &self.source, self.expando).is_none())
            .map(|name| {
                (
                    name.kind().to_owned(),
                    // As the caller wrote it: no grammar placeholder.
                    self.expando.restore(node_text(name, &self.source)),
                )
            });
        Some(PatternDeclaration {
            kind: declared.kind().to_owned(),
            name: named,
            inside: wrapped.map(|_| root.kind().to_owned()),
            fuller_kinds: if declared.child_by_field_name("body").is_some() {
                Vec::new()
            } else {
                let mut fuller = self.fuller_kinds(declared.kind());
                let matched = self.alternate_kinds(declared);
                fuller.retain(|kind| !matched.contains(kind));
                fuller
            },
            alternate_kinds: self.alternate_kinds(declared),
        })
    }

    /// The named, concrete kinds of this grammar that spell `kind` with
    /// words left out of its name: the kinds `kind` is a smaller form of.
    fn fuller_kinds(&self, kind: &str) -> Vec<String> {
        let words = |kind: &str| kind.split('_').map(str::to_owned).collect::<Vec<_>>();
        let smaller = words(kind);
        let is_subsequence = |fuller: &[String]| {
            fuller.len() < smaller.len() && {
                let mut rest = smaller.iter();
                fuller.iter().all(|word| rest.any(|other| other == word))
            }
        };
        let mut kinds = Vec::new();
        for id in 0..self.language.node_kind_count() {
            let Ok(id) = u16::try_from(id) else { break };
            let Some(name) = self.language.node_kind_for_id(id) else {
                continue;
            };
            if self.language.node_kind_is_named(id)
                && self.language.node_kind_is_visible(id)
                && !self.language.node_kind_is_supertype(id)
                && is_subsequence(&words(name))
                && !kinds.iter().any(|known| known == name)
            {
                kinds.push(name.to_owned());
            }
        }
        kinds
    }

    /// Whether `candidate` is of a kind the grammar reads `pattern` as
    /// elsewhere ([`Alternate`]).
    fn is_alternate(&self, pattern: Node<'_>, candidate: Node<'_>) -> bool {
        let key = node_key(pattern);
        self.alternates.iter().any(|alternate| {
            alternate.node == key && alternate.kinds.contains(&candidate.kind_id())
        })
    }

    /// The kinds the grammar reads `node` (of this pattern) as elsewhere.
    fn alternate_kinds(&self, node: Node<'_>) -> Vec<String> {
        let key = node_key(node);
        self.alternates
            .iter()
            .filter(|alternate| alternate.node == key)
            .flat_map(|alternate| &alternate.kinds)
            .filter_map(|id| self.language.node_kind_for_id(*id))
            .map(str::to_owned)
            .collect()
    }

    /// Whether matching this pattern binds and compares no capture: it has
    /// no named metavariable (`$_`, an unnamed `$$$` and plain code bind
    /// nothing), and is not a special pattern, which captures by its shape.
    /// Its answer for a node then does not depend on the environment.
    pub(super) fn binds_nothing(&self) -> bool {
        if self.special.is_some() {
            return false;
        }
        let Some(root) = self.tree.as_ref().map(Tree::root_node) else {
            return true;
        };
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if matches!(
                meta_from_node(node, &self.source, self.expando),
                Some(
                    super::metavars::MetaVar::Single(_) | super::metavars::MetaVar::Multi(Some(_))
                )
            ) {
                return false;
            }
            pending.extend(node.children(&mut node.walk()));
        }
        true
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
        deadline: Deadline,
        recovery: &Recovery,
        found: &mut Found<'_>,
    ) -> Result<(), ExecutionError> {
        let Some(special) = &self.special else {
            return Ok(());
        };
        let tree = parse_tree_with_deadline(&self.language, content, deadline.parse_by())?;
        if recovery.hides_match(&tree, content) {
            found.mark_recovered();
        }

        // One match per node span.
        let mut seen = HashSet::new();
        let line_index = LineIndex::tree_sitter(content);
        let source = SourceContext::root(tree.root_node(), content);
        visit_named(tree.root_node(), deadline, &mut |candidate| {
            if !self.matches_candidate(candidate) {
                return Ok(());
            }
            // Direct patterns and pattern rules share capture equality semantics.
            let mut captures = CaptureEnv::default();
            if special.matches(candidate, content, &mut captures)
                && seen.insert((candidate.start_byte(), candidate.end_byte()))
            {
                found.push(candidate, || {
                    let (metavars, metavar_ranges_raw) = captures.into_maps();
                    to_structural_match_with_context(
                        &source,
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
        deadline: Deadline,
    ) -> Result<bool, ExecutionError> {
        if let Some(special) = &self.special {
            return Ok(special.matches(candidate, content, captures));
        }

        let Some(root) = self.root() else {
            return Ok(false);
        };
        let mut budget = MatchBudget {
            attempts: MAX_MULTI_CAPTURE_ATTEMPTS,
            deadline,
        };
        let texts = node_match::Texts {
            pattern: &self.source,
            candidate: content,
        };
        self.match_node(root, candidate, texts, captures, 0, &mut budget)
    }
}

impl SpecialPattern {
    /// Whether `candidate` matches this pattern, capturing its parts.
    fn matches(&self, candidate: Node<'_>, content: &str, captures: &mut CaptureEnv) -> bool {
        match self {
            Self::JsxTagName { capture } => {
                let Some(tag_name) = jsx_tag_name_node(candidate) else {
                    return false;
                };
                captures.capture_one(capture, node_text(tag_name, content), raw_range(tag_name))
            }
            Self::KeyValuePair {
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
