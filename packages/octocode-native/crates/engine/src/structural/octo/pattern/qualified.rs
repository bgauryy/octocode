//! The longer spellings of a pattern: a pattern matches a path as written,
//! so `Mutex::new($X)` does not match `std::sync::Mutex::new(x)`, a call on
//! a bare name does not match a call that qualifies it (`redact_lines($$$A)`
//! against `crate::security::scan::redact_lines(x)`), and a name inside a
//! larger pattern does not match its qualified spelling (`impl ToolOutput for
//! $T {}` against `impl crate::x::ToolOutput for B {}`). The same pattern
//! with a capture and a separator before some of its names matches exactly
//! the sites that qualify those names.

use std::collections::HashMap;

use tree_sitter::Node;

use super::CompiledPattern;
use crate::structural::language::AgLanguage;
use crate::structural::octo::matching::node_text;
use crate::structural::octo::metavars::meta_from_node;

/// How a grammar qualifies a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Separator {
    /// A namespace path (`a::name`): a longer path before a literal name is
    /// a qualification, never a receiver.
    Path,
    /// A member access (`a.name`): the qualifier can be a namespace, a module
    /// or a receiver, so only a called bare name and a type name (no value,
    /// so never a receiver's member: `ns.Bar` in an annotation) have a longer
    /// spelling.
    Member,
}

impl Separator {
    fn text(self) -> &'static str {
        match self {
            Self::Path => "::",
            Self::Member => ".",
        }
    }
}

/// See [`CompiledPattern::qualifier`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedPattern {
    /// The pattern as the caller wrote it.
    pattern: String,
    separator: Separator,
    /// Each name a longer path can be spelled before, in pattern order: its
    /// text and where it starts in `pattern`.
    names: Vec<(String, usize)>,
    /// `pattern` with a fresh capture in place of each name.
    skeleton: String,
    /// The capture standing for each name in `skeleton`.
    captures: Vec<String>,
}

impl QualifiedPattern {
    /// The names a longer path can be spelled before, in pattern order.
    pub fn names(&self) -> Vec<&str> {
        self.names.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// The pattern with a capture in place of each of its
    /// [`names`](Self::names): it matches every spelling of the pattern, bare
    /// or qualified, and [`Self::qualified_names`] tells them apart.
    pub fn skeleton(&self) -> &str {
        &self.skeleton
    }

    /// Which names (indexes into [`names`](Self::names)) a match of the
    /// [`skeleton`](Self::skeleton) spells with a path or receiver before
    /// them, read from its captures. `None` when a capture holds neither the
    /// name nor a longer spelling of it (another name), or when every name is
    /// spelled bare (a match of the pattern as written).
    pub fn qualified_names(&self, metavars: &HashMap<String, Vec<String>>) -> Option<Vec<usize>> {
        let separator = self.separator.text();
        let mut qualified = Vec::new();
        for (index, ((name, _), capture)) in self.names.iter().zip(&self.captures).enumerate() {
            let [text] = metavars.get(capture)?.as_slice() else {
                return None;
            };
            let spelled: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
            if spelled == *name {
                continue;
            }
            let prefix = spelled.strip_suffix(name.as_str())?;
            if prefix.len() <= separator.len() || !prefix.ends_with(separator) {
                return None;
            }
            qualified.push(index);
        }
        (!qualified.is_empty()).then_some(qualified)
    }

    /// The pattern with a fresh capture and the separator before each name
    /// `qualified` lists (indexes into [`names`](Self::names)): it matches
    /// the sites that qualify exactly those names. The captures are free in
    /// `beside` too, the rest of a rule the spelling goes in: a rule binds
    /// one capture name to one text wherever it is spelled.
    pub fn spelling(&self, qualified: &[usize], beside: &str) -> String {
        let taken = format!("{}\n{beside}", self.pattern);
        let captures = free_captures(&taken, QUALIFIER, qualified.len());
        let mut at: Vec<(usize, &str)> = qualified
            .iter()
            .zip(&captures)
            .filter_map(|(index, capture)| {
                self.names
                    .get(*index)
                    .map(|(_, offset)| (*offset, capture.as_str()))
            })
            .collect();
        at.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
        let mut spelled = self.pattern.clone();
        for (offset, capture) in at {
            spelled.insert_str(offset, &format!("${capture}{}", self.separator.text()));
        }
        spelled
    }

    /// Whether `source` spells any name with the separator before it (space
    /// allowed between): a file that does not holds no match
    /// [`Self::qualified_names`] counts as a longer spelling.
    pub fn may_qualify(&self, source: &str) -> bool {
        let separator = self.separator.text();
        self.names.iter().any(|(name, _)| {
            source.match_indices(name.as_str()).any(|(at, _)| {
                let after = source
                    .get(at + name.len()..)
                    .and_then(|rest| rest.chars().next());
                !after.is_some_and(is_name_char)
                    && source
                        .get(..at)
                        .is_some_and(|before| before.trim_end().ends_with(separator))
            })
        })
    }
}

pub(super) fn is_name_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// A `::` path node and the field that holds its qualifier.
fn qualifier_field(kind: &str) -> Option<&'static str> {
    match kind {
        "scoped_identifier" | "scoped_type_identifier" => Some("path"),
        "qualified_identifier" => Some("scope"),
        _ => None,
    }
}

/// The kind of a bare callee: a plain name, not a capture.
const BARE_CALLEE_KIND: &str = "identifier";

/// The kind of a type name a type position refers to (an annotation's,
/// a parameter's, a return type's); one a declaration names is no reference,
/// and the grammar reads no `$Q.Name` there.
const TYPE_NAME_KIND: &str = "type_identifier";

/// The field that holds what a call calls.
const CALLEE: &str = "function";

/// The names a longer path can be spelled before: a name, not a capture or
/// a path keyword (`crate`, `self`, `super`).
const HEAD_KINDS: [&str; 3] = ["identifier", "type_identifier", "namespace_identifier"];

/// The capture name the longer spellings bind.
const QUALIFIER: &str = "QUALIFIER";

impl CompiledPattern {
    /// The longer spellings of `pattern` (the text this pattern compiled
    /// from) in `lang`, which qualifies names with `separator`. Its names are
    /// the literal names, other than the root of a pattern that is one bare
    /// name (it already matches a path's last segment), that:
    ///
    /// - in a grammar that qualifies with [`Separator::Path`], start a path
    ///   (the leftmost segment of `a::b`, or a bare name), where the grammar
    ///   reads the name with a capture and `::` before it as a path in the
    ///   same place of the same pattern;
    /// - in any grammar, are called bare, where the grammar reads the name
    ///   with a capture and its separator before it as the same call on a
    ///   qualified callee;
    /// - in a grammar that qualifies with [`Separator::Member`], name a type,
    ///   where the grammar reads the name with a capture and `.` before it as
    ///   a qualified type in the same place (`ns.Bar` in an annotation).
    ///
    /// `None` when the pattern has no such name, or its names cannot all be
    /// captures in place.
    pub(in crate::structural::octo) fn qualifier(
        &self,
        lang: &AgLanguage,
        pattern: &str,
        separator: Separator,
    ) -> Option<QualifiedPattern> {
        if self.is_special() {
            return None;
        }
        let root = self.root()?;
        let probe = free_captures(pattern, QUALIFIER, 1).concat();
        let mut names = Vec::new();
        let mut nodes = Vec::new();
        for node in descendants(root) {
            if node == root || !self.qualifiable(node, separator) {
                continue;
            }
            let Some(offset) = self.pattern_offset(pattern, root, node) else {
                continue;
            };
            let name = node_text(node, &self.source).to_owned();
            let mut spelled = pattern.to_owned();
            spelled.insert_str(offset, &format!("${probe}{}", separator.text()));
            // The name, or the path it starts, now reads as a longer path
            // (or member) ending in the same text.
            let qualified = |longer: &Self, written: Node<'_>, replaced: Node<'_>, _: usize| {
                replaced.kind() != node.kind()
                    && !replaced.has_error()
                    && (separator == Separator::Member
                        || qualifier_field(replaced.kind()).is_some())
                    && longer
                        .expando
                        .restore(node_text(replaced, &longer.source))
                        .ends_with(&*self.expando.restore(node_text(written, &self.source)))
            };
            let at_path = separator == Separator::Path;
            if self.respelled(lang, &spelled, &[node], at_path, qualified) {
                names.push((name, offset));
                nodes.push(node);
            }
        }
        if names.is_empty() {
            return None;
        }
        let captures = free_captures(pattern, QUALIFIER, names.len());
        let mut skeleton = pattern.to_owned();
        for ((name, offset), capture) in names.iter().zip(&captures).rev() {
            skeleton.replace_range(*offset..offset + name.len(), &format!("${capture}"));
        }
        let captured = |longer: &Self, _: Node<'_>, replaced: Node<'_>, index: usize| {
            meta_from_node(replaced, &longer.source, longer.expando).is_some()
                && longer.expando.restore(node_text(replaced, &longer.source))
                    == format!("${}", captures[index])
        };
        self.respelled(lang, &skeleton, &nodes, false, captured)
            .then(|| QualifiedPattern {
                pattern: pattern.to_owned(),
                separator,
                names,
                skeleton,
                captures,
            })
    }

    /// Whether `node` is a name a longer path can be spelled before.
    fn qualifiable(&self, node: Node<'_>, separator: Separator) -> bool {
        if !HEAD_KINDS.contains(&node.kind())
            || meta_from_node(node, &self.source, self.expando).is_some()
        {
            return false;
        }
        let parent = node.parent();
        match separator {
            Separator::Member => {
                node.kind() == TYPE_NAME_KIND
                    || (node.kind() == BARE_CALLEE_KIND
                        && parent.and_then(|parent| parent.child_by_field_name(CALLEE))
                            == Some(node))
            }
            // The segments after a path's first are qualified already.
            Separator::Path => parent.is_none_or(|parent| {
                qualifier_field(parent.kind())
                    .is_none_or(|field| parent.child_by_field_name(field) == Some(node))
            }),
        }
    }

    /// Whether `spelled` compiles in `lang` to this pattern's tree with each
    /// of `nodes` replaced by a node `replaced` accepts (given the compiled
    /// spelling, the node as written, its replacement and its index in
    /// `nodes`), and every other node of the same kind and shape. With
    /// `at_path`, a path whose qualifier is one of `nodes` is what is
    /// replaced: a grammar may nest a longer path either way (Rust
    /// `(a::b)::c`, C++ `a::(b::c)`).
    fn respelled(
        &self,
        lang: &AgLanguage,
        spelled: &str,
        nodes: &[Node<'_>],
        at_path: bool,
        replaced: impl Fn(&Self, Node<'_>, Node<'_>, usize) -> bool,
    ) -> bool {
        let Ok(longer) = Self::new(lang, spelled) else {
            return false;
        };
        if longer.is_special() {
            return false;
        }
        let (Some(root), Some(longer_root)) = (self.root(), longer.root()) else {
            return false;
        };
        let slots = Slots { nodes, at_path };
        same_shape(root, longer_root, &slots, &|written, node, index| {
            replaced(&longer, written, node, index)
        })
    }

    /// Where `node` (inside `root`) starts in the caller's `pattern`: the
    /// compiled source may wrap the pattern, respell its captures and end it
    /// with a terminator the caller did not write, so the root is found by its
    /// own text, without that terminator, spelled back as the caller wrote it.
    fn pattern_offset(&self, pattern: &str, root: Node<'_>, node: Node<'_>) -> Option<usize> {
        let before = self.source.get(root.start_byte()..node.start_byte())?;
        let root_end = self.unwritten_terminator.unwrap_or(root.end_byte());
        let written = self.source.get(root.start_byte()..root_end)?.trim_end();
        let root_at = pattern.find(&*self.expando.restore(written))?;
        let at = root_at + self.expando.restore(before).len();
        let name = node_text(node, &self.source);
        (pattern.get(at..at + name.len()) == Some(name)).then_some(at)
    }
}

/// The nodes a respelling replaces ([`CompiledPattern::respelled`]).
struct Slots<'a, 'tree> {
    nodes: &'a [Node<'tree>],
    at_path: bool,
}

impl Slots<'_, '_> {
    /// The index of the slot `node` is replaced as: the node itself, or with
    /// `at_path` the path it qualifies.
    fn of(&self, node: Node<'_>) -> Option<usize> {
        let qualifier = self
            .at_path
            .then(|| qualifier_field(node.kind()))
            .flatten()
            .and_then(|field| node.child_by_field_name(field));
        self.nodes
            .iter()
            .position(|slot| *slot == node || Some(*slot) == qualifier)
    }
}

/// Whether `longer` has `node`'s shape, with each of the `slots` (nodes of
/// `node`'s tree) standing for a node `accept` takes, given the node as
/// written and the slot's index.
fn same_shape(
    node: Node<'_>,
    longer: Node<'_>,
    slots: &Slots<'_, '_>,
    accept: &dyn Fn(Node<'_>, Node<'_>, usize) -> bool,
) -> bool {
    if let Some(index) = slots.of(node) {
        return accept(node, longer, index);
    }
    if node.kind() != longer.kind() || node.named_child_count() != longer.named_child_count() {
        return false;
    }
    let mut cursor = node.walk();
    let mut longer_cursor = longer.walk();
    node.named_children(&mut cursor)
        .zip(longer.named_children(&mut longer_cursor))
        .all(|(child, longer_child)| same_shape(child, longer_child, slots, accept))
}

/// `node` and its named descendants, in source order.
fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut found = vec![node];
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        found.extend(descendants(child));
    }
    found
}

/// `count` capture names `pattern` does not spell: `name`, then `name2`,
/// `name3`, and so on.
pub(super) fn free_captures(pattern: &str, name: &str, count: usize) -> Vec<String> {
    std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name}{n}")))
        .filter(|candidate| !spells_capture(pattern, candidate))
        .take(count)
        .collect()
}

/// Whether `pattern` spells the capture `$name` (or `$$$name`), not a longer
/// capture name that starts with it.
fn spells_capture(pattern: &str, name: &str) -> bool {
    let spelled = format!("${name}");
    pattern.match_indices(&spelled).any(|(at, _)| {
        !pattern
            .get(at + spelled.len()..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(is_name_char)
    })
}
