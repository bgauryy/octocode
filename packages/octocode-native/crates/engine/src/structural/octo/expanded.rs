//! The tree structural matching reads: the file's own tree, with each macro
//! body ([`MacroBodies`]) re-parsed in place of its flat tokens. The candidate
//! walk and every relation (`has`, `inside`, `precedes`, `follows`,
//! `nthChild`) read a node's children and ancestors here, so they agree on
//! what the tree contains.
//!
//! In that tree, an expanded body's owner (a Rust token tree, a C `#define`
//! value) has the body's nodes as its children. What the re-parse adds and
//! no source spells is not in it ([`MacroBodies::is_member`]: the body
//! tree's root, the delimiter wrapper, text past the body, text inside a
//! string literal the re-parse misread); such a node's children take its
//! place. Which nodes are matches is narrower ([`Seen`]): a node the re-parse
//! could not read is in the tree, as an ancestor or sibling, but never a
//! match. Each body is parsed once, before the walk, and lives as long as the
//! file's match, so positions stay in file coordinates and node identities
//! stay unique.
//!
//! A Rust body whose re-parse recovered syntax (a `json!` object is no Rust)
//! also has each part of its flat tokens that the re-parse could not read
//! re-parsed on its own ([`Cover`], [`MacroBodies::parts`]). Those part
//! bodies follow the body's root among the owner's children; the body's own
//! nodes that lie within a part are shadowed: in the tree, so `has` and
//! chains still see them, but never matches, as the part's own re-parse
//! holds them. Text no re-parse read names its body when a match of the
//! query could start in it ([`ExpandedTree::name_hiding_bodies`]).

use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroU16;

use tree_sitter::{Language, Node, Tree, TreeCursor};

use super::macro_bodies::{
    Inner, MAX_MACRO_DEPTH, MacroBodies, PART_TAIL, misread_strings, recovered_ranges,
    string_literals,
};
use super::matching::{Deadline, ExecutionError, ancestors_from_root};
use super::source_context::{SourceContext, tree_id};
use crate::structural::types::RecoveredBody;

type Span = std::ops::Range<usize>;

/// What a walk does after visiting a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Visit {
    /// The walk's question is answered: end it.
    Found,
    /// Visit this node's children next.
    Descend,
    /// Leave this node's subtree unvisited.
    Skip,
}

/// How a walk sees a node of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Seen {
    /// It may be a match.
    Candidate,
    /// The re-parse of its body could not read it (an ERROR node, a MISSING
    /// token, recovered syntax at its own level, an extent across a bracket
    /// group): no match, but a rule matching it names its body.
    Unreadable,
    /// A body node within a part that has its own re-parse: no match, and
    /// no disclosure (the part's re-parse answers for it); also a node the
    /// re-parse could not read that overlaps such a part. A node read
    /// cleanly that starts at the part and goes on is a candidate.
    Shadowed,
}

/// A re-parsed macro body.
struct Body {
    tree: Tree,
    /// The id of the body tree's root.
    root: usize,
    /// The body's text, in file coordinates: the owner's range, or a part's
    /// ([`Cover`]).
    range: tree_sitter::Range,
    /// What the parser read after the text.
    tail: &'static str,
    /// The node spanning exactly the text is a synthetic wrapper (a token
    /// tree's delimiters), not a match.
    delimited: bool,
    /// The owner node, its range, and the tree that holds it.
    owner: usize,
    owner_range: tree_sitter::Range,
    owner_tree: usize,
    /// The string and char literals of the text as first parsed that the
    /// re-parse did not read as one ([`misread_strings`]).
    strings: Vec<Span>,
    /// When the re-parse recovered syntax: the parts of the flat tokens
    /// with a re-parse of their own (shadowed), the flat bracket groups,
    /// sorted by start, the same by end, and each ERROR or MISSING range
    /// with the innermost group whose delimiters hold it.
    parts: Vec<Span>,
    groups: Vec<Span>,
    group_ends: Vec<(usize, usize)>,
    errors: Vec<(Span, Option<Span>)>,
    /// The flat bracket groups, for the test-only extent audit.
    #[cfg(test)]
    audit_groups: Vec<Span>,
    /// What a disclosure names: the macro and the text's first line.
    disclosed: RecoveredBody,
    /// Text of the body no re-parse read holds an anchor: a match could be
    /// there ([`Cover::unread_text`]).
    hides: bool,
    /// Source units for a match in this body: the file's, each enclosing
    /// body's and this one's.
    source: SourceContext,
}

impl Body {
    fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Whether `node`, of this body's tree, is in the tree structural
    /// matching reads: not the tree's root, not the synthetic wrapper
    /// spanning a delimited body, nothing reaching past the text onto its
    /// tail (a `#define` body's whole `do { … } while (0)` statement
    /// included: its `;` is not source), and not text inside a string
    /// literal the re-parse misread (a DSL body such as
    /// `json!({"k": "fn a() {}"})` can re-parse a string's text as code). A
    /// literal itself, an expression starting with one, and the children of
    /// a literal read as one are in it.
    fn is_member(&self, node: Node<'_>) -> bool {
        node.id() != self.root
            && node.end_byte() <= self.range.end_byte
            && !(self.delimited
                && node.byte_range() == (self.range.start_byte..self.range.end_byte))
            && !self
                .strings
                .iter()
                .any(|string| string.start < node.start_byte() && node.start_byte() < string.end)
    }

    /// How a walk sees `node`, a member ([`Self::is_member`]).
    fn seen(&self, node: Node<'_>) -> Seen {
        let span = node.byte_range();
        if self
            .parts
            .iter()
            .any(|part| part.start <= span.start && span.end <= part.end)
        {
            return Seen::Shadowed;
        }
        if node.is_error()
            || node.is_missing()
            || self.crosses_a_group(node.byte_range())
            || (node.has_error() && !self.recovers_inside_its_groups(node.byte_range()))
        {
            // Recovery read a part's text with text around it (a `json!`
            // key and its value as one call): the part's own re-parse
            // answers for the part, the body's own nodes or its unread text
            // ([`Cover::unread_text`]) for the rest. Only a match crossing
            // a part's edge, which no Rust spells at a `,` or `;`, is not
            // named.
            if self
                .parts
                .iter()
                .any(|part| part.start < span.end && span.start < part.end)
            {
                return Seen::Shadowed;
            }
            return Seen::Unreadable;
        }
        Seen::Candidate
    }

    /// Whether `node`'s extent disagrees with the flat bracket groups: a
    /// group starting inside it (or at its start) ends past it, or a group
    /// ending inside it (or at its end) starts before it. A node the parser
    /// read cleanly spans each group it touches or lies strictly inside it.
    fn crosses_a_group(&self, node: Span) -> bool {
        let first = self
            .groups
            .partition_point(|group| group.start < node.start);
        let starts_wrong = self.groups[first..]
            .iter()
            .take_while(|group| group.start < node.end)
            .any(|group| group.end > node.end);
        let first_end = self
            .group_ends
            .partition_point(|&(end, _)| end <= node.start);
        let ends_wrong = self.group_ends[first_end..]
            .iter()
            .take_while(|&&(end, _)| end <= node.end)
            .any(|&(_, start)| start < node.start);
        starts_wrong || ends_wrong
    }

    /// Whether every ERROR or MISSING range under `node` lies inside a
    /// bracket group strictly within `node`: the tokens at `node`'s own
    /// level parsed cleanly, so its kind and extent hold (`fn p(x in 0..3)
    /// {…}` in `proptest!`), as for a node with recovered syntax below it in
    /// the file's own tree. A `json!` key read into a call (`"id": v.f()`)
    /// recovers at the node's own level.
    fn recovers_inside_its_groups(&self, node: Span) -> bool {
        let first = self
            .errors
            .partition_point(|(error, _)| error.start < node.start);
        self.errors[first..]
            .iter()
            .take_while(|(error, _)| error.start <= node.end && error.end <= node.end)
            .all(|(_, group)| {
                group.as_ref().is_some_and(|group| {
                    node.start <= group.start && group.end <= node.end && *group != node
                })
            })
    }
}

/// The innermost of `groups` (sorted by start, properly nested) whose
/// delimiters hold `error`.
fn innermost_group(groups: &[Span], error: &Span) -> Option<Span> {
    let before = groups.partition_point(|group| group.start < error.start);
    groups[..before]
        .iter()
        .rev()
        .find(|group| error.end < group.end)
        .cloned()
}

/// Test-only extent audit: a match from a body lies inside the body's text
/// and agrees with every flat bracket group of its owner (it spans a group,
/// or lies inside it without touching its delimiters, or is apart from it).
/// A violation aborts the test binary (a panic could be caught as a failed
/// search), naming the node.
#[cfg(test)]
fn audit_extent(body: &Body, node: Node<'_>) {
    let span = node.byte_range();
    let inside_text = body.range.start_byte <= span.start && span.end <= body.range.end_byte;
    let agrees = |group: &Span| {
        let apart = span.end <= group.start || group.end <= span.start;
        let spans = span.start <= group.start && group.end <= span.end;
        let within = group.start < span.start && span.end < group.end;
        apart || spans || within
    };
    if !(inside_text && body.audit_groups.iter().all(agrees)) {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "extent audit: a {} match at {span:?} disagrees with its body {:?} ({} line {}) or a bracket group of it",
            node.kind(),
            body.range.start_byte..body.range.end_byte,
            body.disclosed.name,
            body.disclosed.line,
        );
        std::process::abort();
    }
}

/// The body whose tree holds a node; `None` is the file's own tree.
type Frame<'t> = Option<&'t Body>;

/// A node waiting in a walk, with its depth and the tree it sits in.
type Pending<'t> = (Node<'t>, usize, Frame<'t>);

/// One file's tree with its macro bodies expanded.
pub(super) struct ExpandedTree<'f> {
    root: Node<'f>,
    macros: Option<&'f MacroBodies>,
    source: SourceContext,
    bodies: Vec<Body>,
    /// Body indexes by their owner node's id: the owner's body, then its
    /// parts' bodies in source order.
    by_owner: HashMap<usize, Vec<usize>>,
    /// Body index by its tree.
    by_tree: HashMap<usize, usize>,
}

/// A visitor of one node, its ancestor chain (the root first) and whether
/// it may be a match ([`Seen::Candidate`]) or only names its body
/// ([`Seen::Unreadable`]).
pub(super) type VisitNode<'v, 't> =
    dyn FnMut(Node<'t>, &[Node<'t>], Seen) -> Result<(), ExecutionError> + 'v;

impl<'f> ExpandedTree<'f> {
    /// The tree of `root` (a whole file), every body `macros` expands
    /// parsed; `None` keeps every macro's flat tokens. Only the parts of the
    /// file that hold one of the macro anchors are searched for bodies.
    pub(super) fn new(
        root: Node<'f>,
        content: &str,
        language: &Language,
        macros: Option<&'f MacroBodies>,
        deadline: Deadline,
    ) -> Result<Self, ExecutionError> {
        let source = SourceContext::root(root, content);
        let mut bodies = Vec::new();
        if let Some(macros) = macros {
            let anchors = macros.anchor_starts(content);
            if anchors.as_ref().is_none_or(|starts| !starts.is_empty()) {
                let expansion = Expansion {
                    content,
                    language,
                    macros,
                    anchors: anchors.as_deref(),
                    deadline,
                };
                expansion.expand(root, &source, 0, &[], &mut bodies)?;
            }
        }
        let mut by_owner: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, body) in bodies.iter().enumerate() {
            by_owner.entry(body.owner).or_default().push(index);
        }
        let by_tree = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| (tree_id(body.root()), index))
            .collect();
        Ok(Self {
            root,
            macros,
            source,
            bodies,
            by_owner,
            by_tree,
        })
    }

    /// The bodies `node` owns, when it was expanded: its own, then its
    /// parts'.
    fn bodies_of(&self, node: Node<'_>) -> Option<&[usize]> {
        if self.bodies.is_empty() || self.macros.is_none_or(|macros| !macros.is_body(node)) {
            return None;
        }
        self.by_owner
            .get(&node.id())
            .filter(|indexes| {
                indexes
                    .first()
                    .is_some_and(|&index| self.bodies[index].owner_tree == tree_id(node))
            })
            .map(Vec::as_slice)
    }

    /// The body whose tree holds `node`; `None` for the file's own tree.
    fn frame_of(&self, node: Node<'_>) -> Frame<'_> {
        if self.bodies.is_empty() {
            return None;
        }
        self.by_tree
            .get(&tree_id(node))
            .map(|&index| &self.bodies[index])
    }

    /// Whether `node`, of `frame`'s tree, is in the tree.
    fn is_member(node: Node<'_>, frame: Frame<'_>) -> bool {
        frame.is_none_or(|body| body.is_member(node))
    }

    /// How a walk sees `node`, a member of `frame`'s tree.
    fn seen(node: Node<'_>, frame: Frame<'_>) -> Seen {
        frame.map_or(Seen::Candidate, |body| body.seen(node))
    }

    /// The macro body a disclosure names for `node`: the body whose tree
    /// holds it; `None` for the file's own tree.
    pub(super) fn disclosed_body(&self, node: Node<'_>) -> Option<&RecoveredBody> {
        self.frame_of(node).map(|body| &body.disclosed)
    }

    /// Name each macro body with text no re-parse read where a match of the
    /// query could start: a match inside it may be missing, though none of
    /// its nodes matched.
    pub(super) fn name_hiding_bodies(&self, found: &mut super::matching::Found<'_>) {
        for body in self.bodies.iter().filter(|body| body.hides) {
            found.mark_recovered_body(&body.disclosed);
        }
    }

    /// Whether `node` is in the tree.
    fn contains(&self, node: Node<'_>) -> bool {
        Self::is_member(node, self.frame_of(node))
    }

    /// Push `node`'s parsed children for a walk, last first: its bodies'
    /// roots when it was expanded, else its own children (only those in
    /// `field` when one is set). A pushed node not in the tree is passed
    /// through.
    fn push_parsed_children<'t>(
        &'t self,
        (node, depth, frame): Pending<'t>,
        field: Option<NonZeroU16>,
        cursor: &mut TreeCursor<'t>,
        stack: &mut Vec<Pending<'t>>,
    ) {
        if let Some(indexes) = self.bodies_of(node) {
            if field.is_none() {
                for &index in indexes.iter().rev() {
                    let body = &self.bodies[index];
                    stack.push((body.root(), depth, Some(body)));
                }
            }
            return;
        }
        cursor.reset(node);
        if !cursor.goto_first_child() {
            return;
        }
        let start = stack.len();
        loop {
            if field.is_none_or(|field| cursor.field_id() == Some(field)) {
                stack.push((cursor.node(), depth, frame));
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        stack[start..].reverse();
    }

    /// `node`'s children in source order, named or not.
    pub(super) fn children<'t>(&'t self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut children = Vec::new();
        let mut stack = Vec::new();
        let mut cursor = node.walk();
        self.push_parsed_children(
            (node, 0, self.frame_of(node)),
            None,
            &mut cursor,
            &mut stack,
        );
        while let Some(pending) = stack.pop() {
            let (child, _, frame) = pending;
            if Self::is_member(child, frame) {
                children.push(child);
            } else {
                self.push_parsed_children(pending, None, &mut cursor, &mut stack);
            }
        }
        children
    }

    /// Visit the named nodes below `start` in pre-order (source order), the
    /// first level limited to children in `field`. `ancestors` is `start`'s
    /// ancestor chain followed by `start`; `visit` gets each node, its own
    /// chain (root first, parent last) and how it is [`Seen`].
    /// Returns whether a visit reported [`Visit::Found`].
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "`ancestors` is the walk's own depth stack: truncating it to a popped node's depth drops only the chain of a subtree already visited, never a row or line"
        )
    )]
    fn walk<'t>(
        &'t self,
        start: Node<'t>,
        mut ancestors: Vec<Node<'t>>,
        field: Option<NonZeroU16>,
        deadline: Deadline,
        mut visit: impl FnMut(Node<'t>, &[Node<'t>], Seen) -> Result<Visit, ExecutionError>,
    ) -> Result<bool, ExecutionError> {
        let mut stack = Vec::new();
        let mut cursor = start.walk();
        self.push_parsed_children(
            (start, ancestors.len(), self.frame_of(start)),
            field,
            &mut cursor,
            &mut stack,
        );
        while let Some(pending) = stack.pop() {
            ExecutionError::check(deadline)?;
            let (node, depth, frame) = pending;
            if !(node.is_named() && Self::is_member(node, frame)) {
                self.push_parsed_children(pending, None, &mut cursor, &mut stack);
                continue;
            }
            ancestors.truncate(depth);
            let seen = Self::seen(node, frame);
            #[cfg(test)]
            if let (Seen::Candidate, Some(body)) = (seen, frame) {
                audit_extent(body, node);
            }
            match visit(node, &ancestors, seen)? {
                Visit::Found => return Ok(true),
                Visit::Skip => {}
                Visit::Descend => {
                    ancestors.push(node);
                    self.push_parsed_children(
                        (node, depth + 1, frame),
                        None,
                        &mut cursor,
                        &mut stack,
                    );
                }
            }
        }
        Ok(false)
    }

    /// Visit the named nodes below `start` in pre-order (source order), the
    /// first level limited to children in `field`. `ancestors` is `start`'s
    /// ancestor chain followed by `start`; `visit` gets each node and its own
    /// chain (root first, parent last). Returns whether a visit reported
    /// [`Visit::Found`].
    pub(super) fn walk_below<'t>(
        &'t self,
        start: Node<'t>,
        ancestors: Vec<Node<'t>>,
        field: Option<NonZeroU16>,
        deadline: Deadline,
        mut visit: impl FnMut(Node<'t>, &[Node<'t>]) -> Result<Visit, ExecutionError>,
    ) -> Result<bool, ExecutionError> {
        self.walk(start, ancestors, field, deadline, |node, chain, _| {
            visit(node, chain)
        })
    }

    /// Visit every named node of the tree that may be a match or names its
    /// body ([`Seen`]), with its ancestor chain, the root first.
    pub(super) fn visit_nodes<'t>(
        &'t self,
        deadline: Deadline,
        visit: &mut VisitNode<'_, 't>,
    ) -> Result<(), ExecutionError> {
        let root: Node<'t> = self.root;
        if root.is_named() {
            visit(root, &[], Seen::Candidate)?;
        }
        self.walk(root, vec![root], None, deadline, |node, ancestors, seen| {
            if seen != Seen::Shadowed {
                visit(node, ancestors, seen)?;
            }
            Ok(Visit::Descend)
        })?;
        Ok(())
    }

    /// `node`'s ancestor chain (root first, parent last), for a caller that
    /// has no walk's chain for it: the chain the walk hands it.
    pub(super) fn ancestors<'t>(
        &'t self,
        node: Node<'t>,
        deadline: Deadline,
    ) -> Result<Vec<Node<'t>>, ExecutionError> {
        let Some(body) = self.frame_of(node) else {
            let mut chain = ancestors_from_root(self.root, node, deadline)?;
            chain.retain(Node::is_named);
            return Ok(chain);
        };
        let Some(owner) = self.owner(body) else {
            return Ok(Vec::new());
        };
        let mut chain = self.ancestors(owner, deadline)?;
        if self.contains(owner) && owner.is_named() {
            chain.push(owner);
        }
        chain.extend(
            ancestors_from_root(body.root(), node, deadline)?
                .into_iter()
                .filter(|ancestor| body.is_member(*ancestor) && ancestor.is_named()),
        );
        Ok(chain)
    }

    /// The node `body` was parsed from.
    fn owner<'t>(&'t self, body: &'t Body) -> Option<Node<'t>> {
        let tree: Node<'t> = match self.by_tree.get(&body.owner_tree) {
            Some(&index) => self.bodies[index].root(),
            None => self.root,
        };
        let mut node =
            tree.descendant_for_byte_range(body.owner_range.start_byte, body.owner_range.end_byte)?;
        while node.id() != body.owner {
            node = node.parent()?;
        }
        Some(node)
    }

    /// The source units a match of `node` resolves its own span and every
    /// capture's (`trees`, the trees the captures sit in) through: the units
    /// of `node`'s tree, plus each other body a capture sits in (a `has`
    /// descendant inside a body below `node`).
    pub(super) fn source_for(
        &self,
        node: Node<'_>,
        trees: impl IntoIterator<Item = usize>,
    ) -> Result<Cow<'_, SourceContext>, ExecutionError> {
        let source = self
            .frame_of(node)
            .map_or(&self.source, |body| &body.source);
        let mut missing: Vec<usize> = trees
            .into_iter()
            .filter(|tree| !source.holds(*tree))
            .filter_map(|tree| self.by_tree.get(&tree).copied())
            .collect();
        if missing.is_empty() {
            return Ok(Cow::Borrowed(source));
        }
        missing.sort_unstable();
        missing.dedup();
        let mut extended = None;
        for index in missing {
            let body = &self.bodies[index];
            let from: &SourceContext = extended.as_ref().unwrap_or(source);
            extended = Some(from.region(body.root(), body.range, body.tail)?);
        }
        Ok(extended.map_or(Cow::Borrowed(source), Cow::Owned))
    }
}

/// What expanding a file's bodies reads.
struct Expansion<'a> {
    content: &'a str,
    language: &'a Language,
    macros: &'a MacroBodies,
    /// Where an anchor starts in `content`, sorted; `None` when every body
    /// is expanded.
    anchors: Option<&'a [usize]>,
    deadline: Deadline,
}

impl Expansion<'_> {
    /// Whether `node`'s text may hold an anchor, so a body under it may be
    /// expandable.
    fn may_hold_anchor(&self, node: Node<'_>) -> bool {
        self.anchors
            .is_none_or(|_| self.holds_anchor(node.byte_range()))
    }

    /// Whether an anchor starts in `span`; `false` for a query without
    /// anchors, which cannot say where a match would be.
    fn holds_anchor(&self, span: Span) -> bool {
        self.anchors.is_some_and(|starts| {
            let first = starts.partition_point(|&start| start < span.start);
            starts.get(first).is_some_and(|&start| start < span.end)
        })
    }

    /// Parse every expandable body under `root` (bodies inside bodies too,
    /// up to [`MAX_MACRO_DEPTH`]) into `bodies`. A subtree holding no anchor
    /// is not entered, and an expanded body's flat tokens are not searched:
    /// its re-parse holds them. Nothing starting in `shadowed` (macro calls
    /// re-parsed on their own) is expanded or searched: the call's re-parse
    /// holds what is inside the call, and a token tree starting there and
    /// running past it is a misread.
    fn expand(
        &self,
        root: Node<'_>,
        source: &SourceContext,
        depth: usize,
        shadowed: &[Span],
        bodies: &mut Vec<Body>,
    ) -> Result<(), ExecutionError> {
        let mut cursor = root.walk();
        let mut parents: Vec<Node<'_>> = Vec::new();
        loop {
            ExecutionError::check(self.deadline)?;
            let current = cursor.node();
            let start = current.start_byte();
            let mut descend =
                self.may_hold_anchor(current) && !shadowed.iter().any(|call| call.contains(&start));
            if descend
                && depth < MAX_MACRO_DEPTH
                && self
                    .macros
                    .is_expandable(current, parents.last().copied(), self.content)
                && self.expand_owner(current, source, depth, bodies)?
            {
                descend = false;
            }
            if descend && cursor.goto_first_child() {
                parents.push(current);
                continue;
            }
            loop {
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    return Ok(());
                }
                parents.pop();
            }
        }
    }

    /// Parse the body `owner` holds and, when its re-parse recovered syntax
    /// in a delimited body, each part of its flat tokens the re-parse could
    /// not read ([`Cover`]), into `bodies` (the owner's body first among its
    /// own). `false` when the parser declines the body: its flat tokens stay.
    fn expand_owner(
        &self,
        owner: Node<'_>,
        source: &SourceContext,
        depth: usize,
        bodies: &mut Vec<Body>,
    ) -> Result<bool, ExecutionError> {
        let range = owner.range();
        let tail = self.macros.terminator();
        let Some(tree) =
            MacroBodies::parse(self.content, self.language, range, tail, self.deadline)?
        else {
            return Ok(false);
        };
        let recovered = self.macros.is_delimited() && tree.root_node().has_error();
        let strings = misread_strings(string_literals(owner), tree.root_node());
        let mut cover = Cover {
            expansion: self,
            unread: Vec::new(),
            parts: Vec::new(),
            hides: false,
        };
        if recovered {
            cover.unread = recovered_ranges(tree.root_node());
            cover.unread.extend(strings.iter().cloned());
            cover.group(owner)?;
        }
        let Cover { parts, hides, .. } = cover;
        let shadowed: Vec<Span> = parts
            .iter()
            .map(|(part, _)| part.start_byte..part.end_byte)
            .collect();
        let groups = if recovered || cfg!(test) {
            self.macros.groups_below(owner)
        } else {
            Vec::new()
        };
        let body_source = source.region(tree.root_node(), range, tail)?;
        self.expand(tree.root_node(), &body_source, depth + 1, &shadowed, bodies)?;
        let disclosed = RecoveredBody {
            name: MacroBodies::body_name(owner, self.content),
            line: line_of(range.start_point),
        };
        let body = Body {
            root: tree.root_node().id(),
            range,
            tail,
            delimited: self.macros.is_delimited(),
            owner: owner.id(),
            owner_range: range,
            owner_tree: tree_id(owner),
            strings,
            parts: shadowed,
            groups: Vec::new(),
            group_ends: Vec::new(),
            errors: Vec::new(),
            #[cfg(test)]
            audit_groups: Vec::new(),
            disclosed: disclosed.clone(),
            hides,
            source: body_source,
            tree,
        };
        bodies.push(with_groups(body, &groups, recovered));
        for (part, tree) in parts {
            let body_source = source.region(tree.root_node(), part, PART_TAIL)?;
            self.expand(tree.root_node(), &body_source, depth + 1, &[], bodies)?;
            let span = part.start_byte..part.end_byte;
            let within = |inner: &Span| span.start <= inner.start && inner.end <= span.end;
            let part_groups: Vec<Span> = groups.iter().filter(|g| within(g)).cloned().collect();
            let strings = string_literals(owner)
                .into_iter()
                .filter(|string| within(string))
                .collect();
            let part_recovered = tree.root_node().has_error();
            let body = Body {
                root: tree.root_node().id(),
                range: part,
                tail: PART_TAIL,
                delimited: false,
                owner: owner.id(),
                owner_range: range,
                owner_tree: tree_id(owner),
                strings: misread_strings(strings, tree.root_node()),
                parts: Vec::new(),
                groups: Vec::new(),
                group_ends: Vec::new(),
                errors: Vec::new(),
                #[cfg(test)]
                audit_groups: Vec::new(),
                disclosed: disclosed.clone(),
                hides: false,
                source: body_source,
                tree,
            };
            bodies.push(with_groups(body, &part_groups, part_recovered));
        }
        Ok(true)
    }
}

/// The parts of a recovered body its re-parse could not read, each
/// re-parsed on its own: a part of a group's flat tokens
/// ([`MacroBodies::parts`]) that holds unread text is read alone when that
/// reads cleanly; else its macro calls and bracket groups are read the same
/// way ([`MacroBodies::inner`]), and its own-level text stays unread. A part
/// the body's re-parse read keeps that reading (and its ancestors).
struct Cover<'c, 'a> {
    expansion: &'c Expansion<'a>,
    /// What the body's re-parse could not read: its ERROR and MISSING
    /// ranges and the literals it misread.
    unread: Vec<Span>,
    /// Each part read on its own, with its tree, in source order.
    parts: Vec<(tree_sitter::Range, Tree)>,
    /// Own-level text no re-parse read holds an anchor.
    hides: bool,
}

impl Cover<'_, '_> {
    fn group(&mut self, group: Node<'_>) -> Result<(), ExecutionError> {
        for part in MacroBodies::parts(group) {
            self.part(&part)?;
        }
        Ok(())
    }

    /// Read `tokens`: a part, or a macro call in one.
    fn part(&mut self, tokens: &[Node<'_>]) -> Result<(), ExecutionError> {
        ExecutionError::check(self.expansion.deadline)?;
        let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
            return Ok(());
        };
        let range = tree_sitter::Range {
            start_byte: first.start_byte(),
            start_point: first.start_position(),
            end_byte: last.end_byte(),
            end_point: last.end_position(),
        };
        let span = range.start_byte..range.end_byte;
        if !self.unread.iter().any(|unread| touches(unread, &span)) {
            return Ok(());
        }
        let expansion = self.expansion;
        if let Some(tree) = MacroBodies::parse(
            expansion.content,
            expansion.language,
            range,
            PART_TAIL,
            expansion.deadline,
        )? && !tree.root_node().has_error()
        {
            self.parts.push((range, tree));
            return Ok(());
        }
        let mut own = 0;
        for inner in expansion.macros.inner(tokens) {
            let (from, to) = match &inner {
                Inner::Call(call) => (call.start, call.end),
                Inner::Group(at) => (*at, at + 1),
            };
            self.unread_text(&tokens[own..from]);
            match inner {
                // The whole part is the call: read its group's parts.
                Inner::Call(call) if call.len() == tokens.len() => {
                    self.group(tokens[call.end - 1])?;
                }
                Inner::Call(call) => self.part(&tokens[call])?,
                Inner::Group(at) => self.group(tokens[at])?,
            }
            own = to;
        }
        self.unread_text(&tokens[own..]);
        Ok(())
    }

    /// A part's own-level `tokens` no re-parse read: a match could start in
    /// them when an anchor does.
    fn unread_text(&mut self, tokens: &[Node<'_>]) {
        if let (Some(first), Some(last)) = (tokens.first(), tokens.last()) {
            self.hides |= self
                .expansion
                .holds_anchor(first.start_byte()..last.end_byte());
        }
    }
}

/// Whether `unread` (an ERROR or MISSING range, or a misread literal) lies
/// on `span`'s text; a zero-width MISSING token where `span` holds it.
fn touches(unread: &Span, span: &Span) -> bool {
    if unread.is_empty() {
        span.start <= unread.start && unread.start < span.end
    } else {
        unread.start < span.end && span.start < unread.end
    }
}

/// `body` with its owner's flat bracket `groups` (sorted by start): kept for
/// the candidacy checks when its re-parse `recovered` syntax, and for the
/// test-only extent audit always.
fn with_groups(mut body: Body, groups: &[Span], recovered: bool) -> Body {
    #[cfg(test)]
    {
        body.audit_groups = groups.to_vec();
    }
    if recovered {
        body.errors = recovered_ranges(body.root())
            .into_iter()
            .map(|error| {
                let group = innermost_group(groups, &error);
                (error, group)
            })
            .collect();
        body.groups = groups.to_vec();
        body.group_ends = groups
            .iter()
            .map(|group| (group.end, group.start))
            .collect();
        body.group_ends.sort_unstable();
    }
    body
}

/// The 1-based line of `point`.
fn line_of(point: tree_sitter::Point) -> u32 {
    crate::num::saturating_u32(point.row).saturating_add(1)
}
