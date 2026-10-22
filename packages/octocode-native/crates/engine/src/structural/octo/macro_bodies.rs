//! Code inside a macro is opaque to tree-sitter, so structural search and
//! rewrite re-parse each macro body with tree-sitter included ranges: node
//! positions stay in file coordinates, and the body is read in the file's own
//! grammar.
//!
//! - Rust: `assert!(x.unwrap())` is `macro_invocation > token_tree` of flat
//!   tokens, so no expression pattern can match inside it. Each invocation's
//!   token tree is re-parsed with its delimiters (so `(a, b)` reads as a tuple
//!   and `{ … }` as a block), and nested invocations are expanded the same
//!   way. A `macro_rules!` arm's template body is re-parsed too; its matcher
//!   half (`($e:expr)`) is no source and keeps its flat tokens.
//! - C, C++ and CUDA: a `#define` body is one opaque `preproc_arg` (a
//!   `preproc_function_def` or `preproc_def` value). It is re-parsed as
//!   top-level source: an expression or statement reads as code, and a body
//!   that is no code (a lone brace, a type, a token paste) yields nothing
//!   that [`MacroBodies::is_candidate`] accepts.

use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use super::ExecutionError;
use crate::structural::kinds::named_kind_id;
use crate::structural::language::AgLanguage;

/// Nesting bound for macro bodies inside macro bodies.
pub(super) const MAX_MACRO_DEPTH: usize = 16;

/// Grammar ids for macro expansion; absent for grammars without macro bodies.
#[derive(Clone)]
pub(super) struct MacroBodies {
    /// The node that holds a body's text.
    body: u16,
    /// The parents that make `body` source: a Rust invocation or `macro_rules!`
    /// arm (its right-hand token tree is the template body), a C-family
    /// `#define`.
    owners: Vec<u16>,
    /// The body's text includes its delimiters (Rust token trees), so the
    /// node spanning the whole body is the wrapper, not a match.
    delimited: bool,
    /// Literals one of which every match contains; a body without any of them
    /// cannot match and keeps its flat text.
    anchors: Option<Vec<String>>,
}

impl MacroBodies {
    pub(super) fn for_language(
        language: &AgLanguage,
        anchors: Option<Vec<String>>,
    ) -> Option<Self> {
        let ts = language.tree_sitter_language();
        let anchors = anchors.filter(|anchors| !anchors.is_empty());
        let kinds = |names: &[&str]| -> Vec<u16> {
            names
                .iter()
                .filter_map(|name| named_kind_id(&ts, name))
                .collect()
        };
        if language.is_c_family() {
            return Some(Self {
                body: named_kind_id(&ts, "preproc_arg")?,
                owners: kinds(&["preproc_function_def", "preproc_def"]),
                delimited: false,
                anchors,
            });
        }
        named_kind_id(&ts, "macro_invocation")?;
        Some(Self {
            body: named_kind_id(&ts, "token_tree")?,
            owners: kinds(&["macro_invocation", "macro_rule"]),
            delimited: true,
            anchors,
        })
    }

    /// Whether `node` is a macro body worth re-parsing: an owner's body that
    /// is not empty and holds one of the pattern's anchors.
    pub(super) fn is_expandable(
        &self,
        node: Node<'_>,
        parent: Option<Node<'_>>,
        content: &str,
    ) -> bool {
        // A delimited body is empty when only its two delimiters remain.
        let empty = if self.delimited { 2 } else { 0 };
        node.kind_id() == self.body
            && parent.is_some_and(|parent| self.owners.contains(&parent.kind_id()))
            && node.end_byte() > node.start_byte() + empty
            && self.anchors.as_ref().is_none_or(|anchors| {
                let text = content.get(node.byte_range()).unwrap_or_default();
                anchors.iter().any(|anchor| text.contains(anchor.as_str()))
            })
    }

    /// The source the parser reads right after a body. A `#define` body is
    /// an expression or statement without its terminator: read bare,
    /// `zfree(p)` is a macro-typed declaration, so the parser reads a `;`
    /// after it. Rust token trees carry their own delimiters.
    pub(super) fn terminator(&self) -> &'static str {
        if self.delimited { "" } else { ";" }
    }

    /// Parse the body at `range` (its text is `content[range]`, and `content`
    /// holds nothing the body needs after it) as source; `None` when the
    /// parser declines it. The result's positions stay in `content`
    /// coordinates; only a node reaching past the body, onto the
    /// [`Self::terminator`], is outside it ([`Self::is_candidate`]).
    pub(super) fn parse(
        &self,
        content: &str,
        language: &Language,
        range: tree_sitter::Range,
        deadline: Instant,
    ) -> Result<Option<Tree>, ExecutionError> {
        let tail = self.terminator();
        let mut included = range;
        included.end_byte += tail.len();
        included.end_point.column += tail.len();
        let tree = crate::signatures::extractor::parse_ranges_ending(
            &content[..range.end_byte],
            tail,
            language,
            &[included],
            deadline,
        );
        if tree.is_none() {
            ExecutionError::check(deadline)?;
        }
        Ok(tree)
    }

    /// Whether a node of a re-parsed body is a real match candidate: not the
    /// body tree's own root, not the synthetic wrapper spanning a delimited
    /// body, nothing reaching past the body onto its terminator (a `#define`
    /// body's whole `do { … } while (0)` statement included: its `;` is not
    /// source), not recovered syntax, and not text inside one of the body's
    /// string literals (`strings`, from [`string_literals`]: a DSL body such
    /// as `json!({"k": "fn a() {}"})` can re-parse a string's text as code).
    pub(crate) fn is_candidate(
        &self,
        node: Node<'_>,
        body: &tree_sitter::Range,
        strings: &[std::ops::Range<usize>],
    ) -> bool {
        node.parent().is_some()
            && node.end_byte() <= body.end_byte
            && !(self.delimited && node.byte_range() == (body.start_byte..body.end_byte))
            && !node.is_error()
            && !node.has_error()
            && !strings
                .iter()
                .any(|string| string.contains(&node.start_byte()))
    }
}

/// Byte ranges of the string and char literals under `root` (a token tree
/// as parsed before its re-parse).
pub(crate) fn string_literals(root: Node<'_>) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if crate::structural::kinds::is_string_kind(node.kind()) {
            found.push(node.byte_range());
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found
}
