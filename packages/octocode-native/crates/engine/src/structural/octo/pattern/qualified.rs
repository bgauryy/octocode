//! The longer spelling of a `::`-qualified pattern: a pattern matches a path
//! as written, so `Mutex::new($X)` does not match `std::sync::Mutex::new(x)`.
//! The same pattern with one capture before its leftmost segment matches
//! exactly those longer spellings.

use tree_sitter::Node;

use super::CompiledPattern;
use crate::structural::octo::matching::node_text;
use crate::structural::octo::metavars::meta_from_node;

/// See [`CompiledPattern::qualifier`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedPattern {
    /// The leftmost segment the pattern spells (`Mutex`).
    pub head: String,
    /// The pattern with a capture before that segment
    /// (`$QUALIFIER::Mutex::new($X)`).
    pub broader: String,
}

/// A `::` path node and the field that holds its qualifier.
fn qualifier_field(kind: &str) -> Option<&'static str> {
    match kind {
        "scoped_identifier" | "scoped_type_identifier" => Some("path"),
        "qualified_identifier" => Some("scope"),
        _ => None,
    }
}

/// The leftmost segments a longer path can be spelled before: a name, not a
/// capture or a path keyword (`crate`, `self`, `super`).
const HEAD_KINDS: [&str; 3] = ["identifier", "type_identifier", "namespace_identifier"];

/// The capture name the longer spelling binds.
const QUALIFIER: &str = "QUALIFIER";

impl CompiledPattern {
    /// The longer spelling of `pattern` (the text this pattern compiled
    /// from): when its root is a `::` path or a call on one and the path's
    /// leftmost segment is a literal name, the pattern with a fresh capture
    /// and `::` before that segment. `None` for any other pattern.
    pub(in crate::structural::octo) fn qualifier(&self, pattern: &str) -> Option<QualifiedPattern> {
        if self.is_special() {
            return None;
        }
        let root = self.root()?;
        let path = root.child_by_field_name("function").unwrap_or(root);
        let mut head = path;
        while let Some(field) = qualifier_field(head.kind()) {
            head = head.child_by_field_name(field)?;
        }
        if head == path
            || !HEAD_KINDS.contains(&head.kind())
            || meta_from_node(head, &self.source, self.expando).is_some()
        {
            return None;
        }
        let at = self.pattern_offset(pattern, root, head)?;
        let capture = free_capture(pattern);
        Some(QualifiedPattern {
            head: node_text(head, &self.source).to_owned(),
            broader: format!("{}${capture}::{}", &pattern[..at], &pattern[at..]),
        })
    }

    /// Where `node` (inside `root`) starts in the caller's `pattern`: the
    /// compiled source may wrap the pattern and respell its captures, so the
    /// root is found by its own text spelled back as the caller wrote it.
    fn pattern_offset(&self, pattern: &str, root: Node<'_>, node: Node<'_>) -> Option<usize> {
        let before = self.source.get(root.start_byte()..node.start_byte())?;
        let root_at = pattern.find(&self.expando.restore(node_text(root, &self.source)))?;
        Some(root_at + self.expando.restore(before).len())
    }
}

/// `QUALIFIER`, else the first `QUALIFIER<n>` the pattern does not spell.
fn free_capture(pattern: &str) -> String {
    std::iter::once(QUALIFIER.to_owned())
        .chain((2..).map(|n| format!("{QUALIFIER}{n}")))
        .find(|name| !pattern.contains(&format!("${name}")))
        .unwrap_or_default()
}
