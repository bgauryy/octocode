//! Matching a compiled pattern node against a candidate node.

use std::num::NonZeroU16;

use tree_sitter::Node;

use crate::structural::octo::matching::{
    CaptureEnv, ExecutionError, children, node_text, raw_range,
};
use crate::structural::octo::metavars::{MetaVar, meta_from_node};

use super::unsaid::{Declaration, ParameterSlot, Unsaid, is_longer_form, wrapped_capture};
use super::{CompiledPattern, MatchBudget, node_key};

mod multi;

// Native recursion guard for pattern matching, from the large stack the
// matcher runs on; AST traversal is iterative.
use crate::structural::octo::stack::MAX_STRUCTURAL_DEPTH;

/// The source text of each side of a match.
#[derive(Clone, Copy)]
pub(super) struct Texts<'s> {
    pub(super) pattern: &'s str,
    pub(super) candidate: &'s str,
}

impl CompiledPattern {
    // `depth` guards native-stack growth against pathologically nested patterns
    // (see `MAX_STRUCTURAL_DEPTH`); `attempts` is the shared `$$$` split budget
    // (see `MAX_MULTI_CAPTURE_ATTEMPTS`). The budget also carries the run deadline.
    pub(super) fn match_node(
        &self,
        pattern: Node<'_>,
        candidate: Node<'_>,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        ExecutionError::check(budget.deadline)?;
        if depth >= MAX_STRUCTURAL_DEPTH {
            return Err(ExecutionError::limit(
                "structural.match.depthLimit",
                "match",
                "Structural pattern matching exceeded its recursion limit",
            ));
        }
        if !self.ends_as_written(pattern, candidate, texts.pattern) {
            return Ok(false);
        }
        if let Some(meta) = meta_from_node(pattern, texts.pattern, self.expando) {
            // A MISSING node is tree-sitter's zero-width error-recovery
            // placeholder for a token the grammar expected but the source
            // never had. A bare metavar (`$X`/`$_`) would otherwise bind to
            // it unconditionally, reporting a phantom match with empty
            // captured text on a syntactically broken file. This does not
            // exclude `is_error()` subtrees generally — those wrap real
            // (if malformed) source text and a legitimate match can still
            // occur inside them.
            return Ok(bind_meta(meta, pattern, candidate, texts, captures));
        }

        let candidate = match self.across_kinds(pattern, candidate, texts, budget) {
            Across::Decided(decided) => return Ok(decided),
            Across::Parts(candidate) => candidate,
            Across::Failed(error) => return Err(*error),
        };
        let (pattern_kin, candidate_kin) = self.child_lists(pattern, candidate);
        // A pattern leaf is a token the caller spelled: its text decides,
        // whatever parts the candidate holds. A grammar may read a keyword
        // as a name by wrapping its token (Scala's soft `extension` is an
        // `identifier` over the keyword), and an empty pattern list would
        // accept that token as an unsaid trailing part of any name.
        if pattern.child_count() == 0
            || (pattern_kin.nodes.is_empty() && candidate_kin.nodes.is_empty())
        {
            return Ok(node_text(pattern, texts.pattern) == node_text(candidate, texts.candidate));
        }
        let lists = Lists {
            unsaid: Unsaid::of(
                pattern,
                self.pattern_declares(pattern),
                candidate,
                texts.pattern,
                self.expando,
            ),
            by_fields: spells_fields_only(pattern),
            pattern: pattern_kin,
            candidate: candidate_kin,
        };
        self.match_child_list(
            &lists.pattern.nodes,
            &lists.candidate.nodes,
            &lists,
            texts,
            captures,
            depth,
            budget,
        )
    }

    /// The fields of the pattern's `declared` node in which `candidate`
    /// differs ([`CompiledPattern::differing_fields`]): each spelled part,
    /// read as a match reads it, against the candidate's part in its field.
    /// Each part binds in a fresh environment, so one part's captures do not
    /// decide another's.
    pub(super) fn fields_that_differ(
        &self,
        declared: Node<'_>,
        candidate: Node<'_>,
        content: &str,
    ) -> Vec<String> {
        let Unsaid::Declaration(declaration) = Unsaid::of(
            declared,
            self.pattern_declares(declared),
            candidate,
            &self.source,
            self.expando,
        ) else {
            return Vec::new();
        };
        let texts = Texts {
            pattern: &self.source,
            candidate: content,
        };
        let mut differ: Vec<String> = Vec::new();
        let mut cursor = declared.walk();
        let spelled: Vec<_> = declared
            .children(&mut cursor)
            .enumerate()
            .filter(|(_, part)| part.is_named() && !part.is_extra())
            .filter_map(|(index, part)| {
                let field = declared.field_name_for_child(u32::try_from(index).ok()?)?;
                Some((field, part))
            })
            .collect();
        for (field, part) in spelled {
            if differ.iter().any(|known| known == field) {
                continue;
            }
            let found = candidate.child_by_field_name(field);
            // `($$$)` takes any parameters, or none where the grammar omits
            // the list: no spelling of the slot differs.
            if !matches!(
                declaration.parameter_slot(part, found, &self.source, self.expando),
                ParameterSlot::Usual
            ) {
                continue;
            }
            let mut budget = MatchBudget {
                attempts: super::MAX_MULTI_CAPTURE_ATTEMPTS,
                deadline: crate::structural::octo::matching::Deadline::UNBOUNDED,
            };
            let matched = found.is_some_and(|found| {
                self.match_declaration_part(
                    &declaration,
                    part,
                    found,
                    texts,
                    &mut CaptureEnv::default(),
                    1,
                    &mut budget,
                )
                .unwrap_or(true)
            });
            if !matched {
                differ.push(field.to_owned());
            }
        }
        differ
    }

    /// One pattern child against one candidate child of a declaration: a type's
    /// body reads its members as held, a declarator the function under it, a
    /// capture's wrapper the other form of its slot, anything else as itself.
    fn match_declaration_part(
        &self,
        declaration: &Declaration,
        part: Node<'_>,
        candidate: Node<'_>,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        if declaration.is_member_body(part) {
            if part.kind() != candidate.kind() {
                return Ok(false);
            }
            let (pattern, candidate) = self.child_lists(part, candidate);
            let lists = Lists {
                unsaid: Unsaid::Members,
                by_fields: false,
                pattern,
                candidate,
            };
            return self.match_child_list(
                &lists.pattern.nodes,
                &lists.candidate.nodes,
                &lists,
                texts,
                captures,
                depth + 1,
                budget,
            );
        }
        let candidate = declaration
            .under_declarator(part, candidate)
            .unwrap_or(candidate);
        if let Some((capture, wrapped)) = wrapped_capture(
            part,
            candidate,
            (texts.pattern, texts.candidate),
            self.expando,
        ) {
            return self.match_node(capture, wrapped, texts, captures, depth + 1, budget);
        }
        self.match_node(part, candidate, texts, captures, depth + 1, budget)
    }

    /// One step of a declaration's child list: whether its first pattern part
    /// matched the first candidate child, and whether that child may be
    /// skipped when not. A part in a field reads only the candidate's child
    /// in that field: a capture does not bind the modifier or token before
    /// it. Out of [`Self::match_child_list`] to keep its frame small.
    #[inline(never)]
    fn declaration_step(
        &self,
        declaration: &Declaration,
        lists: &Lists<'_, '_>,
        pattern_children: &[Node<'_>],
        candidate_children: &[Node<'_>],
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<(bool, bool), ExecutionError> {
        let (first, candidate_first) = (pattern_children[0], candidate_children[0]);
        let skips = declaration_skips(declaration, lists, pattern_children, candidate_children);
        let other_slot = declaration
            .field_of_pattern(first)
            .is_some_and(|field| declaration.field_of_candidate(candidate_first) != Some(field));
        let matched = !other_slot
            && self.match_declaration_part(
                declaration,
                first,
                candidate_first,
                texts,
                captures,
                depth,
                budget,
            )?;
        Ok((matched, skips))
    }

    fn match_child_list(
        &self,
        mut pattern_children: &[Node<'_>],
        mut candidate_children: &[Node<'_>],
        lists: &Lists<'_, '_>,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        // Ordinary siblings consume no native stack; only nested patterns and
        // multi-capture branches recurse, and both spend the depth guard.
        ExecutionError::check(budget.deadline)?;
        if depth >= MAX_STRUCTURAL_DEPTH {
            return Err(ExecutionError::limit(
                "structural.match.depthLimit",
                "match",
                "Structural pattern matching exceeded its recursion limit",
            ));
        }
        // Bind straight into `captures`; any failure rolls back to here so a
        // `false` result leaves the caller's environment unchanged.
        let checkpoint = captures.checkpoint();
        let matched = loop {
            // ast-grep "smart" strictness, which rewrite uses: once every
            // pattern node matched, trailing candidate nodes (a trailing
            // argument comma, a `;` the pattern omitted) do not fail it;
            // another item of the list the last part matched in does.
            let Some(first) = pattern_children.first().copied() else {
                break matches!(lists.unsaid, Unsaid::Members)
                    || !holds_another_item(&lists.candidate, candidate_children);
            };
            let multi = match meta_from_node(first, texts.pattern, self.expando) {
                Some(MetaVar::Multi(name)) => Some(name),
                Some(MetaVar::IgnoredMulti) => Some(None),
                _ => None,
            };
            if let Some(name) = multi {
                break self.match_multi_capture(
                    name,
                    &pattern_children[1..],
                    candidate_children,
                    lists,
                    texts,
                    captures,
                    depth + 1,
                    budget,
                )?;
            }
            let candidate_first = candidate_children.first().copied();
            if let Unsaid::Declaration(declaration) = &lists.unsaid {
                match declaration.parameter_slot(
                    first,
                    candidate_first,
                    texts.pattern,
                    self.expando,
                ) {
                    ParameterSlot::Usual => {}
                    ParameterSlot::Absent(name) => {
                        if !bind_parameters(name, &[], texts, captures) {
                            break false;
                        }
                        pattern_children = &pattern_children[1..];
                        continue;
                    }
                    ParameterSlot::Spelled(name) => {
                        if !bind_parameters(name, &candidate_children[..1], texts, captures) {
                            break false;
                        }
                        pattern_children = &pattern_children[1..];
                        candidate_children = &candidate_children[1..];
                        continue;
                    }
                }
            }
            let Some(candidate_first) = candidate_first else {
                break false;
            };
            let (part_matched, skips) = match &lists.unsaid {
                Unsaid::Exact => (
                    self.fills_its_slot(
                        first,
                        lists.pattern.field_at(pattern_children),
                        lists.candidate.field_at(candidate_children),
                        texts.pattern,
                    ) && self.match_node(
                        first,
                        candidate_first,
                        texts,
                        captures,
                        depth + 1,
                        budget,
                    )?,
                    exact_skips(lists, pattern_children, candidate_children),
                ),
                Unsaid::Members => (
                    self.match_node(first, candidate_first, texts, captures, depth + 1, budget)?,
                    true,
                ),
                Unsaid::Declaration(declaration) => self.declaration_step(
                    declaration,
                    lists,
                    pattern_children,
                    candidate_children,
                    texts,
                    captures,
                    depth,
                    budget,
                )?,
            };
            if !part_matched {
                // Smart strictness again: every pattern node must match, but
                // what the pattern did not spell (punctuation, comments, a
                // declaration's unsaid parts) is skipped rather than failing
                // the match.
                if skips {
                    candidate_children = &candidate_children[1..];
                    continue;
                }
                break false;
            }
            pattern_children = &pattern_children[1..];
            candidate_children = &candidate_children[1..];
        };
        if !matched {
            captures.rollback(checkpoint);
        }
        Ok(matched)
    }
}

impl CompiledPattern {
    /// Whether `candidate` ends where `pattern`, the root, does when the
    /// caller wrote a `;` after it ([`super::WrittenTerminator`]).
    fn ends_as_written(&self, pattern: Node<'_>, candidate: Node<'_>, source: &str) -> bool {
        let Some(written) = &self.written_terminator else {
            return true;
        };
        if written.root != node_key(pattern) {
            return true;
        }
        let terminated = std::iter::successors(candidate.next_sibling(), Node::next_sibling)
            .find(|node| !node.is_extra())
            .is_some_and(|node| !node.is_named() && !node.is_missing() && node.kind() == ";");
        let wrapped = written.parent.is_none_or(|kind| {
            candidate
                .parent()
                .is_some_and(|parent| parent.kind_id() == kind)
        });
        let (Some((last, field)), Some((candidate_last, candidate_field))) =
            (last_part(pattern), last_part(candidate))
        else {
            return terminated && wrapped;
        };
        let same_end = match meta_from_node(last, source, self.expando) {
            Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti) => true,
            meta => {
                field == candidate_field
                    && (field.is_some()
                        || meta.is_some()
                        || last.kind_id() == candidate_last.kind_id())
            }
        };
        terminated && wrapped && same_end
    }

    /// How `pattern` reads a `candidate` of another kind: the node behind a
    /// leading separator, or a longer form or alternate compared by parts
    /// ([`Across::Parts`]), else whether it is the same name. Out of
    /// [`Self::match_node`] to keep its frame small on deep patterns.
    #[inline(never)]
    fn across_kinds<'c>(
        &self,
        pattern: Node<'_>,
        candidate: Node<'c>,
        texts: Texts<'_>,
        budget: &MatchBudget,
    ) -> Across<'c> {
        if pattern.kind_id() == candidate.kind_id() {
            return Across::Parts(candidate);
        }
        match self.after_leading_separator(pattern, candidate, budget) {
            Ok(Some(separated)) => return self.across_kinds(pattern, separated, texts, budget),
            Ok(None) => {}
            Err(error) => return Across::Failed(Box::new(error)),
        }
        match self.other_kind(pattern, candidate, texts) {
            Some(decided) => Across::Decided(decided),
            None => Across::Parts(candidate),
        }
    }

    /// The node `candidate` wraps behind a leading separator, when that is all
    /// it holds: a token before its one named part that the grammar reads, in
    /// `pattern`'s place, as the infix separator of `candidate`'s kind (the
    /// leading `|` of a Rust arm `| None`). The grammar decides it: the
    /// pattern with `P tok P` in place of `P` must parse as a node of that
    /// kind, which `-x` (`x - x` is a subtraction) does not. Out of
    /// [`Self::match_node`] to keep its frame small.
    #[inline(never)]
    fn after_leading_separator<'c>(
        &self,
        pattern: Node<'_>,
        candidate: Node<'c>,
        budget: &MatchBudget,
    ) -> Result<Option<Node<'c>>, ExecutionError> {
        let parts = children(candidate)
            .into_iter()
            .filter(|part| !part.is_extra())
            .collect::<Vec<_>>();
        let [token, separated] = parts.as_slice() else {
            return Ok(None);
        };
        if token.is_named() || !separated.is_named() || is_word(token.kind()) {
            return Ok(None);
        }
        let key = (node_key(pattern), candidate.kind_id(), token.kind_id());
        let known = self
            .separators
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .copied();
        let separates = match known {
            Some(separates) => separates,
            None => {
                let separates =
                    self.separates(pattern, candidate.kind_id(), token.kind(), budget)?;
                self.separators
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(key, separates);
                separates
            }
        };
        Ok(separates.then_some(*separated))
    }

    /// Whether the pattern, with `pattern tok pattern` in `pattern`'s place,
    /// parses there as one error-free node of kind `kind`.
    fn separates(
        &self,
        pattern: Node<'_>,
        kind: u16,
        token: &str,
        budget: &MatchBudget,
    ) -> Result<bool, ExecutionError> {
        let (Some(before), Some(spelled), Some(after)) = (
            self.source.get(..pattern.start_byte()),
            self.source.get(pattern.byte_range()),
            self.source.get(pattern.end_byte()..),
        ) else {
            return Ok(false);
        };
        let doubled = format!("{spelled} {token} {spelled}");
        let source = format!("{before}{doubled}{after}");
        let tree = crate::structural::octo::matching::parse_tree_with_deadline(
            &self.language,
            &source,
            budget.deadline.parse_by(),
        )?;
        let (start, end) = (pattern.start_byte(), pattern.start_byte() + doubled.len());
        let mut node = tree.root_node().descendant_for_byte_range(start, end);
        while let Some(found) = node {
            if found.start_byte() != start || found.end_byte() != end {
                break;
            }
            if found.kind_id() == kind {
                return Ok(!found.has_error());
            }
            node = found.parent();
        }
        Ok(false)
    }

    /// `pattern` against a `candidate` of another kind: `None` when the
    /// candidate is a longer form of the declaration or a kind the grammar
    /// reads the pattern as elsewhere (their parts decide), else whether it
    /// is the same name spelled with another kind. Out of
    /// [`Self::match_node`] to keep its frame small on deep patterns.
    #[inline(never)]
    fn other_kind(&self, pattern: Node<'_>, candidate: Node<'_>, texts: Texts<'_>) -> Option<bool> {
        let compared_by_parts =
            is_longer_form(pattern, candidate.kind()) || self.is_alternate(pattern, candidate);
        (!compared_by_parts).then(|| same_name(pattern, candidate, texts))
    }

    /// Whether a candidate in `candidate_field` can stand for `part`, in
    /// `part_field`: a single capture in a field (a declaration's name) reads
    /// only a candidate child in that field, so `struct $S` needs a name and
    /// an anonymous struct has none.
    fn fills_its_slot(
        &self,
        part: Node<'_>,
        part_field: Option<NonZeroU16>,
        candidate_field: Option<NonZeroU16>,
        source: &str,
    ) -> bool {
        let single = matches!(
            meta_from_node(part, source, self.expando),
            Some(MetaVar::Single(_) | MetaVar::IgnoredSingle)
        );
        !single || part_field.is_none_or(|field| candidate_field == Some(field))
    }

    /// Each node's children to compare.
    ///
    /// MISSING nodes drop from the PATTERN's own children. A MISSING node is
    /// tree-sitter's zero-width error-recovery stand-in for a token the
    /// grammar expected but never got, which never represents user intent
    /// (nobody types a "missing token" into a pattern string). It shows up
    /// because some grammars parse a bare `$$$NAME`/`$X` expando identifier at
    /// statement position ambiguously (C/C++/C# treat an unrecognized
    /// identifier as the start of a declaration and then expect a trailing
    /// `;`): the compiled pattern's root is not `is_error()`, but the MISSING
    /// sibling it left behind could never match a real candidate child. The
    /// candidate side is left as-is: a MISSING token in the searched document
    /// is genuine evidence of broken source and must still fail to match.
    ///
    /// So does a `;` the caller did not write: a lone one right after a
    /// `$$$`, which a grammar needed to parse the capture, and the one
    /// compilation appended to give the whole pattern its context
    /// (`CompiledPattern::new`; `impl X for $T` parses only as
    /// `impl X for $T;`, and a candidate with a body in its place matches,
    /// since the pattern leaves the body unsaid).
    fn child_lists<'p, 'c>(&self, pattern: Node<'p>, candidate: Node<'c>) -> (Kin<'p>, Kin<'c>) {
        let (source, expando) = (self.source.as_str(), self.expando);
        let mut after_multi = false;
        let pattern_kin = Kin::read(pattern, |node| {
            if node.is_missing() {
                return false;
            }
            let appended = !node.is_named() && Some(node.start_byte()) == self.unwritten_terminator;
            let terminator = appended || (after_multi && node_text(node, source) == ";");
            after_multi = matches!(
                meta_from_node(node, source, expando),
                Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
            );
            !terminator
        });
        (pattern_kin, Kin::read(candidate, |_| true))
    }
}

/// A node's children, as the matcher compares them, with the field each
/// fills. Read once from the node's cursor: the matcher asks a child's field
/// and neighbours here, by position, never through `Node::parent` or a
/// sibling step, which tree-sitter answers by walking down from the root, so
/// a deeply nested match would pay its depth at every level. Every child
/// slice the matcher holds is a suffix of `nodes` ([`Self::at`]).
struct Kin<'t> {
    nodes: Vec<Node<'t>>,
    fields: Vec<Option<NonZeroU16>>,
}

impl<'t> Kin<'t> {
    fn read(node: Node<'t>, mut keep: impl FnMut(Node<'t>) -> bool) -> Self {
        let (mut nodes, mut fields) = (Vec::new(), Vec::new());
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if keep(child) {
                    nodes.push(child);
                    fields.push(cursor.field_id());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        Self { nodes, fields }
    }

    /// The position `rest`, a suffix of these children, starts at.
    fn at(&self, rest: &[Node<'_>]) -> usize {
        self.nodes.len().saturating_sub(rest.len())
    }

    fn field(&self, at: usize) -> Option<NonZeroU16> {
        self.fields.get(at).copied().flatten()
    }

    /// The field of the first child of `rest`.
    fn field_at(&self, rest: &[Node<'_>]) -> Option<NonZeroU16> {
        self.field(self.at(rest))
    }

    /// The position of the last named part before `at`, other than an extra.
    fn named_before(&self, at: usize) -> Option<usize> {
        let before = self.nodes.get(..at)?;
        before.iter().rposition(|node| is_part(*node))
    }
}

/// The two child lists a list step compares, and how it reads them.
struct Lists<'p, 'c> {
    unsaid: Unsaid,
    pattern: Kin<'p>,
    candidate: Kin<'c>,
    /// The pattern node spells its parts by their fields alone
    /// ([`spells_fields_only`]).
    by_fields: bool,
}

/// A named part, other than an extra.
fn is_part(node: Node<'_>) -> bool {
    node.is_named() && !node.is_extra()
}

/// Whether `pattern` and `candidate` are the same name the grammar spells
/// with two kinds in one slot: named leaves in the same field with the same
/// text (a C++ member's `field_identifier` for an `identifier`).
fn same_name(pattern: Node<'_>, candidate: Node<'_>, texts: Texts<'_>) -> bool {
    let leaf = |node: Node<'_>| node.is_named() && node.child_count() == 0;
    leaf(pattern)
        && leaf(candidate)
        && node_text(pattern, texts.pattern) == node_text(candidate, texts.candidate)
        && field_in_parent(pattern).is_some_and(|field| field_in_parent(candidate) == Some(field))
}

/// The field `node` fills in its parent.
fn field_in_parent(node: Node<'_>) -> Option<std::num::NonZeroU16> {
    let parent = node.parent()?;
    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        if cursor.node() == node {
            return cursor.field_id();
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

/// `node`'s last named part, other than an extra, and the field it is in.
fn last_part(node: Node<'_>) -> Option<(Node<'_>, Option<std::num::NonZeroU16>)> {
    let mut cursor = node.walk();
    let mut last = None;
    if cursor.goto_first_child() {
        loop {
            let part = cursor.node();
            if part.is_named() && !part.is_extra() {
                last = Some((part, cursor.field_id()));
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    last
}

/// Fill an any-parameters list with the nodes the candidate holds there.
fn bind_parameters(
    name: Option<&str>,
    nodes: &[Node<'_>],
    texts: Texts<'_>,
    captures: &mut CaptureEnv,
) -> bool {
    name.is_none_or(|name| {
        captures.capture_many(
            name,
            nodes.iter().map(|node| node_text(*node, texts.candidate)),
            || nodes.iter().map(|node| raw_range(*node)).collect(),
        )
    })
}

/// Whether the first of `candidates` is one the pattern need not spell
/// (ast-grep's smart strictness): an extra such as a comment, a named part
/// the pattern leaves `unsaid`, or a token that belongs to what it leaves
/// out. A token is one of those when it is a keyword (a modifier: `unsafe`,
/// `async`), when `kind_tokens` (the candidate is of another kind the match
/// accepts, whose own tokens spell what it adds: `function*`), when no named
/// part follows it (a trailing `,`), or when the named part it introduces is
/// unsaid too (`->` before a return type). Any other token stands before a
/// part the pattern spells and changes what that part means (`impl !Tr`), so
/// it is required.
///
/// The grammar names an anonymous token by its literal text: a keyword's is
/// a word, punctuation's is not ([`is_word`]).
///
/// Two kinds of token sit before the node's first named part and say
/// nothing of the parts after it: a kind token there belongs to the kind's
/// head (`function*`), while one between its parts (Go's `...` before a
/// variadic type) changes the part after it; and a leading separator, the
/// token the node also puts between its named parts (`| A | B`).
///
/// `candidates` holds the candidate's children; the one to skip is at `at`,
/// and `unsaid` takes a position in it.
#[inline(never)]
fn skips_unspelled(
    candidates: &Kin<'_>,
    at: usize,
    kind_tokens: bool,
    unsaid: impl Fn(usize) -> bool,
) -> bool {
    let Some(first) = candidates.nodes.get(at) else {
        return false;
    };
    if first.is_extra() {
        return true;
    }
    if first.is_named() {
        return unsaid(at);
    }
    let before = candidates.nodes.get(..at).unwrap_or_default();
    let rest = candidates.nodes.get(at + 1..).unwrap_or_default();
    let leads = before.iter().all(|node| !is_part(*node));
    (kind_tokens && leads)
        || is_word(first.kind())
        || (leads && separates_its_parts(*first, rest))
        || rest
            .iter()
            .position(|part| is_part(*part))
            .is_none_or(|next| unsaid(at + 1 + next))
}

/// Whether `rest`, the candidate children left when the pattern ended, holds
/// another item of the list the child before them (the one the pattern's
/// last part matched) is one of: a named sibling in the same field, or in
/// no field and of the same kind. A single capture binds one item, so `from $M
/// import $X` ends short of `from m import a, b`, as a closing token ends a
/// call's `f($A)` short of `f(a, b)`. A token, or a part in another field
/// (an `else`, a body the pattern left unsaid), is no further item.
#[inline(never)]
fn holds_another_item(candidates: &Kin<'_>, rest: &[Node<'_>]) -> bool {
    let at = candidates.at(rest);
    let Some((last, before)) = at
        .checked_sub(1)
        .and_then(|before| Some((*candidates.nodes.get(before)?, before)))
        .filter(|(last, _)| is_part(*last))
    else {
        return false;
    };
    let field = candidates.field(before);
    rest.iter().enumerate().any(|(offset, node)| {
        is_part(*node)
            && candidates.field(at + offset) == field
            && (field.is_some() || node.kind_id() == last.kind_id())
    })
}

/// Bind a single capture to `candidate`, or refuse it. A MISSING node is
/// tree-sitter's zero-width error-recovery placeholder for a token the
/// grammar expected but the source never had: a bare metavar (`$X`/`$_`)
/// would otherwise bind to it, reporting a phantom match with empty text on
/// a broken file (`is_error()` subtrees still match: they wrap real text). A
/// bare marker ([`is_marker`]) is no part a capture names. Out of
/// [`CompiledPattern::match_node`] to keep its frame small.
#[inline(never)]
fn bind_meta(
    meta: MetaVar<'_>,
    pattern: Node<'_>,
    candidate: Node<'_>,
    texts: Texts<'_>,
    captures: &mut CaptureEnv,
) -> bool {
    if candidate.is_missing() {
        return false;
    }
    if !meta_is_multi(&meta) && (is_marker(candidate) || holds_more_items(pattern, candidate)) {
        return false;
    }
    match meta {
        MetaVar::Single(name) => captures.capture_one(
            name,
            node_text(candidate, texts.candidate),
            raw_range(candidate),
        ),
        MetaVar::IgnoredSingle => true,
        MetaVar::Multi(_) | MetaVar::IgnoredMulti => false,
    }
}

/// Whether a single capture's `pattern` node is a one-item list that only
/// wraps it (Go's `expression_list` in `$A := $C`) while `candidate`, a list
/// of the same kind, holds several items: the capture binds one item, not
/// `a, b`. An item is a named child in the capture's field; in no field, one
/// of the first item's kind or after a token that separates it (`s.x, b`). A
/// part in another field, or an unseparated one of another kind, is no item:
/// `<$T>` binds the whole parameter `T extends B` (its `constraint`, Java's
/// `type_bound`), as ast-grep does.
fn holds_more_items(pattern: Node<'_>, candidate: Node<'_>) -> bool {
    if pattern.kind_id() != candidate.kind_id() {
        return false;
    }
    let parts = |node: Node<'_>| {
        let mut cursor = node.walk();
        let mut parts = Vec::new();
        let mut separated = false;
        if cursor.goto_first_child() {
            loop {
                let part = cursor.node();
                if part.is_named() && !part.is_extra() {
                    parts.push((cursor.field_id(), part.kind_id(), separated));
                    separated = false;
                } else if !part.is_extra() && !parts.is_empty() {
                    separated = true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        parts
    };
    let [(field, _, _)] = *parts(pattern).as_slice() else {
        return false;
    };
    let mut items = parts(candidate)
        .into_iter()
        .filter(|(part_field, _, _)| *part_field == field);
    let Some((_, first_kind, _)) = items.next() else {
        return false;
    };
    items.any(|(_, kind, separated)| field.is_some() || kind == first_kind || separated)
}

/// Whether `candidate` is another item of the list the part before `part`
/// (the pattern's last spelled one) is an item of: a named child in that
/// part's field, or in no field and of its kind. The pattern spelled one
/// item there, so the further one is never skipped as unsaid (C `int a =
/// 1, b = 2;`, Go `var p, q int`).
fn repeats_spelled_item(lists: &Lists<'_, '_>, part: usize, candidate: usize) -> bool {
    let Some(node) = lists
        .candidate
        .nodes
        .get(candidate)
        .filter(|node| is_part(**node))
    else {
        return false;
    };
    let Some(spelled) = lists.pattern.named_before(part) else {
        return false;
    };
    let field = lists.pattern.field(spelled);
    lists.candidate.field(candidate) == field
        && (field.is_some()
            || lists
                .pattern
                .nodes
                .get(spelled)
                .is_some_and(|spelled| node.kind_id() == spelled.kind_id()))
}

/// Whether an exactly read list skips its next candidate child: only an
/// unspelled token or extra, or an annotation the pattern node, spelled by
/// its fields alone, leaves out ([`is_annotation`]); never a further item of
/// a list the pattern spelled one of ([`repeats_spelled_item`]).
#[inline(never)]
fn exact_skips(lists: &Lists<'_, '_>, pattern: &[Node<'_>], candidates: &[Node<'_>]) -> bool {
    let (part, at) = (lists.pattern.at(pattern), lists.candidate.at(candidates));
    !repeats_spelled_item(lists, part, at)
        && skips_unspelled(&lists.candidate, at, false, |next| {
            lists.by_fields && is_annotation(&lists.candidate, next)
        })
}

/// Whether a declaration skips its next candidate child: one it leaves
/// unsaid, or a token of that.
#[inline(never)]
fn declaration_skips(
    declaration: &Declaration,
    lists: &Lists<'_, '_>,
    pattern_rest: &[Node<'_>],
    candidates: &[Node<'_>],
) -> bool {
    let (part, at) = (
        lists.pattern.at(pattern_rest),
        lists.candidate.at(candidates),
    );
    let repeats = !pattern_rest.is_empty()
        && repeats_spelled_item(lists, part, at)
        && !candidates
            .first()
            .is_some_and(|node| declaration.holds_parameters(*node));
    !repeats
        && skips_unspelled(&lists.candidate, at, declaration.is_other_kind(), |next| {
            lists
                .candidate
                .nodes
                .get(next)
                .is_some_and(|node| declaration.leaves_unsaid(*node, pattern_rest))
        })
}

/// How a pattern node reads a candidate ([`CompiledPattern::across_kinds`]).
enum Across<'c> {
    /// Decided without reading parts.
    Decided(bool),
    /// Compare the pattern's parts with this node's.
    Parts(Node<'c>),
    /// The grammar could not be asked (a parse past the deadline); boxed to
    /// keep the recursive frame small.
    Failed(Box<ExecutionError>),
}

/// Whether `token`'s kind also stands in `rest` between two named parts.
fn separates_its_parts(token: Node<'_>, rest: &[Node<'_>]) -> bool {
    let named = |part: &Node<'_>| part.is_named() && !part.is_extra();
    rest.iter().enumerate().any(|(at, part)| {
        !part.is_named()
            && part.kind_id() == token.kind_id()
            && rest[..at].iter().any(named)
            && rest[at + 1..].iter().any(named)
    })
}

/// Whether every named part of `node` (a pattern node) is in a field: it
/// spells the node by its fields alone.
fn spells_fields_only(node: Node<'_>) -> bool {
    let kin = Kin::read(node, |_| true);
    let mut named =
        (0..kin.nodes.len()).filter(|&at| kin.nodes.get(at).is_some_and(|part| is_part(*part)));
    let first = named.next();
    first.is_some()
        && first
            .into_iter()
            .chain(named)
            .all(|at| kin.field(at).is_some())
}

/// Whether `part` annotates the type its node holds: a named part in no
/// field, ahead of the node's `type` field, that names something (a Rust
/// lifetime before a reference's type). One made of tokens alone (`mut`)
/// is a specifier that changes the type, and a part before another field
/// (Go's `for` clause before the body) is the node's own.
fn is_annotation(candidates: &Kin<'_>, at: usize) -> bool {
    let Some(part) = candidates.nodes.get(at) else {
        return false;
    };
    let type_field = part.language().field_id_for_name("type");
    part.named_child_count() > 0
        && candidates.field(at).is_none()
        && type_field.is_some()
        && (at + 1..candidates.nodes.len()).any(|after| candidates.field(after) == type_field)
}

/// Whether `node` is a bare marker: a named node of one punctuation token,
/// standing in a list in no field (Python's keyword-only `*`), which a
/// single capture never binds: it holds no name, value or keyword. A unit
/// `()` (two tokens) is a value, and Rust's `!` in a return type a type.
#[inline(never)]
fn is_marker(node: Node<'_>) -> bool {
    node.is_named()
        && node.named_child_count() == 0
        && node.child_count() == 1
        && field_in_parent(node).is_none()
        && children(node)
            .iter()
            .all(|token| !token.is_named() && !is_word(token.kind()))
}

fn meta_is_multi(meta: &MetaVar<'_>) -> bool {
    matches!(meta, MetaVar::Multi(_) | MetaVar::IgnoredMulti)
}

/// Whether a token's literal text is a word: a keyword, not punctuation.
fn is_word(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && token.chars().all(|c| c.is_alphanumeric() || c == '_')
}
