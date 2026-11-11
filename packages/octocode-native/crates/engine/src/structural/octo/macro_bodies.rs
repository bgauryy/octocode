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
//!   that is no code (a lone brace, a type, a token paste) yields no node
//!   the expanded walk matches ([`super::expanded::ExpandedTree`]).
//!
//! - A Rust body that is no Rust (a `json!` object) re-parses with recovered
//!   syntax, and recovery can pull neighbouring text into a node or read a
//!   key's quotes as a string around code. Each part of its flat tokens the
//!   re-parse could not read ([`MacroBodies::parts`]: split at `,` `;` `:`
//!   `=>`) is then re-parsed on its own, so the code in it has nodes with
//!   their own extents; a part that still recovers is split further, by its
//!   macro calls (`path ! group`) and bracket groups ([`MacroBodies::inner`]).
//!
//! [`super::expanded::ExpandedTree`] parses every body of a file once and
//! puts it in place of its flat tokens for the walk and every relation.

use tree_sitter::{Language, Node, Tree};

use super::ExecutionError;
use crate::structural::kinds::named_kind_id;
use crate::structural::language::AgLanguage;

/// Nesting bound for macro bodies inside macro bodies.
pub(super) const MAX_MACRO_DEPTH: usize = 16;

/// The source the parser reads right after a part re-parsed on its own
/// ([`MacroBodies::parts`]): an expression or statement ending the source
/// needs its `;` (`y.len()` alone ends in a MISSING one), as a `#define`
/// body does; a lone `name!(…)` reads as one `macro_invocation` either way
/// (tests: `a_lone_macro_call_reparses_as_one_invocation`). The `;` is past
/// the part's text, so nothing reaching it is a match.
pub(super) const PART_TAIL: &str = ";";

/// The flat tokens that end one part of a group and start the next: list
/// and statement separators, and the joints of the key-value forms macros
/// take (`"k": v` in `json!`, `k => v`). `::` is a token of its own.
const SEPARATORS: [&str; 4] = [",", ";", ":", "=>"];

/// What a part of a group's flat tokens holds below its own level, by the
/// indexes of its tokens in the part.
pub(super) enum Inner {
    /// A macro call (`path ! group`): its tokens.
    Call(std::ops::Range<usize>),
    /// Another bracket group (a body node).
    Group(usize),
}

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

    /// Whether a body's text includes its delimiters (Rust token trees).
    pub(super) fn is_delimited(&self) -> bool {
        self.delimited
    }

    /// Parse the text at `range` (`content[range]`, and `content` holds
    /// nothing it needs after it), followed by `tail`, as source; `None`
    /// when the parser declines it. The result's positions stay in `content`
    /// coordinates; only a node reaching past the range, onto `tail`, is
    /// outside it.
    pub(super) fn parse(
        content: &str,
        language: &Language,
        range: tree_sitter::Range,
        tail: &str,
        deadline: super::Deadline,
    ) -> Result<Option<Tree>, ExecutionError> {
        let mut included = range;
        included.end_byte += tail.len();
        included.end_point.column += tail.len();
        let tree = crate::signatures::extractor::parse_ranges_ending(
            &content[..range.end_byte],
            tail,
            language,
            &[included],
            deadline.parse_by(),
        );
        if tree.is_none() {
            ExecutionError::check(deadline)?;
        }
        Ok(tree)
    }

    /// Whether `node` holds a body's text ([`Self::is_expandable`] checks
    /// its parent and text).
    pub(super) fn is_body(&self, node: Node<'_>) -> bool {
        node.kind_id() == self.body
    }

    /// How a disclosure names the body `owner` holds: the macro and `!`
    /// (`json!`), `macro_rules!`, or `#define NAME`.
    pub(super) fn body_name(owner: Node<'_>, content: &str) -> String {
        let Some(parent) = owner.parent() else {
            return String::new();
        };
        let text = |node: Node<'_>| content.get(node.byte_range()).unwrap_or_default();
        match parent.kind() {
            "macro_invocation" => parent
                .child_by_field_name("macro")
                .map(|name| format!("{}!", text(name)))
                .unwrap_or_default(),
            "macro_rule" => "macro_rules!".to_owned(),
            _ => parent
                .child_by_field_name("name")
                .map(|name| format!("#define {}", text(name)))
                .unwrap_or_default(),
        }
    }

    /// The flat tokens of `group` (a body node) between its delimiters,
    /// split at its own-level [`SEPARATORS`]: each part non-empty, in source
    /// order. A nested group is one token of a part.
    pub(super) fn parts(group: Node<'_>) -> Vec<Vec<Node<'_>>> {
        let mut cursor = group.walk();
        let tokens: Vec<Node<'_>> = group.children(&mut cursor).collect();
        let inside = match tokens.as_slice() {
            [open, inside @ .., close]
                if matches!(open.kind(), "(" | "[" | "{")
                    && matches!(close.kind(), ")" | "]" | "}") =>
            {
                inside
            }
            all => all,
        };
        inside
            .split(|token| SEPARATORS.contains(&token.kind()))
            .filter(|part| !part.is_empty())
            .map(<[Node<'_>]>::to_vec)
            .collect()
    }

    /// What `part` (of [`Self::parts`]) holds below its own level, in source
    /// order: each macro call (`path ! group`, the path an identifier with
    /// optional `::` parts) and each other bracket group.
    pub(super) fn inner(&self, part: &[Node<'_>]) -> Vec<Inner> {
        let mut found = Vec::new();
        let mut at = 0;
        while at < part.len() {
            let token = part[at];
            if token.kind() == "!"
                && let Some(&body) = part.get(at + 1)
                && self.is_body(body)
                && let Some(start) = path_start(&part[..at])
            {
                found.push(Inner::Call(start..at + 2));
                at += 2;
                continue;
            }
            if self.is_body(token) {
                found.push(Inner::Group(at));
            }
            at += 1;
        }
        found
    }

    /// The ranges of the body nodes strictly below `owner_body` (a token
    /// tree's nested token trees: its delimited groups), sorted by start.
    pub(super) fn groups_below(&self, owner_body: Node<'_>) -> Vec<std::ops::Range<usize>> {
        let mut groups = Vec::new();
        let mut cursor = owner_body.walk();
        // The cursor's own depth below `owner_body` (`TreeCursor::depth`
        // recounts its whole path).
        let mut depth = 0usize;
        if !cursor.goto_first_child() {
            return groups;
        }
        depth += 1;
        loop {
            let node = cursor.node();
            if self.is_body(node) {
                groups.push(node.byte_range());
            }
            if cursor.goto_first_child() {
                depth += 1;
                continue;
            }
            while !cursor.goto_next_sibling() {
                depth -= 1;
                if depth == 0 || !cursor.goto_parent() {
                    return groups;
                }
            }
        }
    }

    /// Where each anchor starts in `content`, sorted: only a subtree holding
    /// one can hold an expandable body. `None` when every body is expanded.
    pub(super) fn anchor_starts(&self, content: &str) -> Option<Vec<usize>> {
        let anchors = self.anchors.as_ref()?;
        let mut starts: Vec<usize> = anchors
            .iter()
            .flat_map(|anchor| content.match_indices(anchor.as_str()).map(|(at, _)| at))
            .collect();
        starts.sort_unstable();
        starts.dedup();
        Some(starts)
    }

}

/// Where the macro path ending `tokens` starts: an identifier, preceded by
/// any `identifier ::` parts and an optional leading `::`.
fn path_start(tokens: &[Node<'_>]) -> Option<usize> {
    let mut start = tokens.len().checked_sub(1)?;
    if tokens[start].kind() != "identifier" {
        return None;
    }
    while start >= 2
        && tokens[start - 1].kind() == "::"
        && tokens[start - 2].kind() == "identifier"
    {
        start -= 2;
    }
    if start >= 1 && tokens[start - 1].kind() == "::" {
        start -= 1;
    }
    Some(start)
}

/// The literals of `strings` (from [`string_literals`]) that the body's
/// re-parse under `root` did not read as one string or char literal over
/// the same text: such text may have re-parsed as code. A literal read as
/// one keeps its own children (escape sequences, content).
pub(super) fn misread_strings(
    strings: Vec<std::ops::Range<usize>>,
    root: Node<'_>,
) -> Vec<std::ops::Range<usize>> {
    if strings.is_empty() {
        return strings;
    }
    let read = string_literals(root);
    strings
        .into_iter()
        .filter(|string| {
            !read
                .binary_search_by_key(&string.start, |read| read.start)
                .is_ok_and(|at| read.get(at) == Some(string))
        })
        .collect()
}

/// Byte ranges of the string and char literals under `root` (a token tree
/// as parsed before its re-parse, or a re-parsed body), in source order.
pub(crate) fn string_literals(root: Node<'_>) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let string = crate::signatures::node_kinds::is_string_kind(node.kind());
        if string {
            found.push(node.byte_range());
        }
        if !string && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

/// Byte ranges of the ERROR nodes and MISSING tokens under `root`, in
/// source order (an ERROR node's own subtree is not searched).
pub(super) fn recovered_ranges(root: Node<'_>) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let recovered = node.is_error() || node.is_missing();
        if recovered {
            found.push(node.byte_range());
        }
        if !recovered && node.has_error() && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}
