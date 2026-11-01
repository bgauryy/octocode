//! Whether a file's recovered parse could hide a match.
//!
//! Tree-sitter recovers from a syntax error by wrapping text it cannot place
//! in an ERROR node or reshaping its neighbours, and a pattern or kind that
//! needed that text as a clean node misses it silently (a macro attribute
//! such as `JSON_HEDLEY_NON_NULL(2)` before a method garbles the method and
//! the calls inside it). The search reports such a file so its answer states
//! the gap; it must not report the many files that merely hold an error
//! somewhere (92% of nlohmann's headers, every NASM file), or the notice
//! would carry no information.
//!
//! A file counts when its parse has errors and the match could sit in them:
//!
//! - A query with literal anchors (`StructuralQuery::recovery_anchors`) can
//!   only match at an anchor. An occurrence in code counts when the smallest
//!   node holding it, or that node's parent when it is a leaf token, is an
//!   ERROR or contains one: the construct around the anchor was recovered, so
//!   a clean node the query needs may not exist. An occurrence in a comment
//!   or a string is no code, and an occurrence under clean nodes parses as it
//!   would in a clean file, even if an error sits elsewhere in the file.
//! - A query with no anchor (a kind, a bare call shape) cannot say where a
//!   match would be, so any file with recovered syntax counts.
//!
//! Only the file's own parse counts: a macro body that is not code re-parses
//! with errors by design ([`super::MacroBodies`]).

use tree_sitter::{Node, Tree};

use crate::signatures::node_kinds::{is_comment_kind, is_string_kind};

/// The literal anchors of a query (`None`: anchorless).
pub(super) struct Recovery(Option<Vec<String>>);

impl Recovery {
    pub(super) fn new(anchors: Option<Vec<String>>) -> Self {
        Self(anchors.filter(|anchors| !anchors.is_empty()))
    }

    /// Whether the recovered syntax of `tree` (parsed from `content`) could
    /// hide a match of the query.
    pub(super) fn hides_match(&self, tree: &Tree, content: &str) -> bool {
        let root = tree.root_node();
        if !root.has_error() {
            return false;
        }
        let Some(anchors) = &self.0 else {
            return true;
        };
        anchors.iter().any(|anchor| {
            content
                .match_indices(anchor.as_str())
                .any(|(start, _)| recovered_at(root, start, start + anchor.len()))
        })
    }
}

/// Whether the code at `start..end` sits in recovered syntax.
fn recovered_at(root: Node<'_>, start: usize, end: usize) -> bool {
    let Some(mut node) = root.descendant_for_byte_range(start, end) else {
        return false;
    };
    if is_text(node) {
        return false;
    }
    if node.child_count() == 0
        && let Some(parent) = node.parent()
    {
        // A leaf token is clean on its own; its construct is what recovered.
        if is_text(parent) {
            return false;
        }
        node = parent;
    }
    node.is_error() || node.has_error()
}

/// A comment or a string: no code to match.
fn is_text(node: Node<'_>) -> bool {
    is_comment_kind(node.kind()) || is_string_kind(node.kind())
}
