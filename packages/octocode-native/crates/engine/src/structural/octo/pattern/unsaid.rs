//! What a declaration pattern leaves unsaid.
//!
//! A pattern spells the parts it cares about, so `fn $N($$$) { $$$ }` means
//! every function whether or not it adds a visibility, type parameters, a
//! return type or a `where` clause. The grammar decides what a declaration is:
//! a node with parameters, or one that binds a name, pattern or type to a
//! body, value or declarator ([`declares`]). The pattern itself proves a part
//! optional, as it parsed as that node without it.
//!
//! What stays exact: a part the pattern spells must be there and agree; the
//! contents of its parameters and of a function's body; literal text; and every
//! other kind of node, so calls and expressions keep all their parts.

use std::num::NonZeroU16;

use tree_sitter::Node;

use crate::structural::language::Expando;
use crate::structural::octo::matching::named_children;
use crate::structural::octo::metavars::{MetaVar, meta_from_node};

/// The field that holds a function's parameter list.
const PARAMETERS: &str = "parameters";
/// The field a grammar holds a lone parameter in, beside [`PARAMETERS`]
/// (`x => x`).
const PARAMETER: &str = "parameter";

/// The field C-family grammars nest a declared name in, through the pointers
/// and references around it.
const DECLARATOR: &str = "declarator";

fn has_field(node: Node<'_>, fields: &[&str]) -> bool {
    fields
        .iter()
        .any(|field| node.child_by_field_name(field).is_some())
}

/// Whether the grammar spells `node` as a declaration, by the fields this
/// spelling of it fills: parameters, or a binder and what it binds. A node
/// with arguments is a call, whatever else it fills.
pub(super) fn declares(node: Node<'_>) -> bool {
    if has_field(node, &["arguments"]) {
        return false;
    }
    has_field(node, &[PARAMETERS, PARAMETER])
        || (has_field(node, &["name"]) && has_field(node, &["body", "value", "type"]))
        || (has_field(node, &["pattern"]) && has_field(node, &["value"]))
        || (has_field(node, &["type"]) && has_field(node, &["body", DECLARATOR]))
        || (has_field(node, &[DECLARATOR]) && has_field(node, &["body"]))
}

/// Whether `candidate_kind` is `pattern`'s kind with one more leading word, the
/// grammar's own longer form of that declaration
/// (`generator_function_declaration`, `abstract_class_declaration`).
pub(super) fn is_longer_form(pattern: Node<'_>, candidate_kind: &str) -> bool {
    let kind = pattern.kind();
    kind.contains('_')
        && candidate_kind
            .strip_suffix(kind)
            .and_then(|word| word.strip_suffix('_'))
            .is_some_and(|word| !word.is_empty() && !word.contains('_'))
        && declares(pattern)
}

/// The capture `part` wraps, and the node `candidate` wraps in its place, when
/// both open with one token around one node: `: $R` reads the type predicate of
/// `: x is T` and the assertion of `: asserts x`, the other forms a return slot
/// takes. A body stays itself: `{ $X }` opens with another token than `(y)`.
pub(super) fn wrapped_capture<'p, 'c>(
    part: Node<'p>,
    candidate: Node<'c>,
    texts: (&str, &str),
    expando: Expando,
) -> Option<(Node<'p>, Node<'c>)> {
    if part.kind() == candidate.kind() {
        return None;
    }
    let (spelled, source) = texts;
    let [capture] = *named_children(part).as_slice() else {
        return None;
    };
    if !matches!(
        meta_from_node(capture, spelled, expando),
        Some(MetaVar::Single(_) | MetaVar::IgnoredSingle)
    ) {
        return None;
    }
    let [wrapped] = *named_children(candidate).as_slice() else {
        return None;
    };
    let opening = |node: Node<'_>, text: &str| {
        node.child(0)
            .filter(|first| !first.is_named())
            .map(|first| text[first.byte_range()].to_owned())
    };
    (opening(part, spelled).is_some() && opening(part, spelled) == opening(candidate, source))
        .then_some((capture, wrapped))
}

/// How a pattern node's children read against a candidate's.
pub(super) enum Unsaid {
    /// All of them are spelled.
    Exact,
    Declaration(Declaration),
    /// A type's members: the pattern's are held in order, among others.
    Members,
}

/// A declaration's children, by the field each fills in its own node.
pub(super) struct Declaration {
    pattern_fields: Vec<(usize, NonZeroU16)>,
    candidate_fields: Vec<(usize, NonZeroU16)>,
    /// This grammar's parameter-slot fields, as [`PARAMETERS`] then
    /// [`PARAMETER`].
    parameter_fields: [Option<NonZeroU16>; 2],
    /// The candidate spells a parameter slot.
    candidate_has_parameters: bool,
    /// The body whose members the pattern lists, on a declaration that takes
    /// no parameters: a function's body holds statements, in full.
    member_body: Option<usize>,
    /// The pattern's declarator, when the pattern spells no literal type: a
    /// type it spells stays exact, so `void $F()` is not `void *f()`.
    declarator: Option<usize>,
}

fn fields_of(node: Node<'_>) -> Vec<(usize, NonZeroU16)> {
    let mut cursor = node.walk();
    let mut fields = Vec::new();
    if cursor.goto_first_child() {
        loop {
            if let Some(field) = cursor.field_id() {
                fields.push((cursor.node().id(), field));
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    fields
}

fn field_of(fields: &[(usize, NonZeroU16)], node: Node<'_>) -> Option<NonZeroU16> {
    fields
        .iter()
        .find(|(id, _)| *id == node.id())
        .map(|(_, field)| *field)
}

/// A list that holds a `$$$` and nothing else, and the name it captures under.
struct SoleMulti<'s>(Option<&'s str>);

fn sole_multi<'s>(list: Node<'_>, source: &'s str, expando: Expando) -> Option<SoleMulti<'s>> {
    let named = named_children(list);
    let [only] = named.as_slice() else {
        return None;
    };
    match meta_from_node(*only, source, expando) {
        Some(MetaVar::Multi(name)) => Some(SoleMulti(name)),
        Some(MetaVar::IgnoredMulti) => Some(SoleMulti(None)),
        _ => None,
    }
}

/// `body`, when its members are all declarations and at least one is spelled.
fn lists_members(body: Node<'_>, source: &str, expando: Expando) -> bool {
    let mut spelled = false;
    for member in named_children(body) {
        if member.is_extra() && !member.is_error() {
            continue;
        }
        if matches!(
            meta_from_node(member, source, expando),
            Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
        ) {
            continue;
        }
        if !declares(member) {
            return false;
        }
        spelled = true;
    }
    spelled
}

impl Unsaid {
    pub(super) fn of(
        pattern: Node<'_>,
        candidate: Node<'_>,
        source: &str,
        expando: Expando,
    ) -> Self {
        if !declares(pattern) {
            return Self::Exact;
        }
        let member_body = pattern
            .child_by_field_name("body")
            .filter(|body| {
                !has_field(pattern, &[PARAMETERS, PARAMETER])
                    && lists_members(*body, source, expando)
            })
            .map(|body| body.id());
        Self::Declaration(Declaration {
            parameter_fields: [PARAMETERS, PARAMETER]
                .map(|field| pattern.language().field_id_for_name(field)),
            pattern_fields: fields_of(pattern),
            candidate_fields: fields_of(candidate),
            candidate_has_parameters: has_field(candidate, &[PARAMETERS, PARAMETER]),
            member_body,
            declarator: pattern
                .child_by_field_name("type")
                .is_none_or(|spelled| meta_from_node(spelled, source, expando).is_some())
                .then(|| pattern.child_by_field_name(DECLARATOR))
                .flatten()
                .map(|node| node.id()),
        })
    }
}

/// What to do with a pattern's `($$$)` at a candidate child.
pub(super) enum ParameterSlot<'s> {
    /// The pattern's child is not an any-parameters list, or this child reads
    /// as usual.
    Usual,
    /// The candidate has no parameter slot: the list holds nothing.
    Absent(Option<&'s str>),
    /// The candidate spells the slot another way: the list holds that node.
    Spelled(Option<&'s str>),
}

impl Declaration {
    pub(super) fn field_of_pattern(&self, node: Node<'_>) -> Option<NonZeroU16> {
        field_of(&self.pattern_fields, node)
    }

    pub(super) fn field_of_candidate(&self, node: Node<'_>) -> Option<NonZeroU16> {
        field_of(&self.candidate_fields, node)
    }

    pub(super) fn is_member_body(&self, node: Node<'_>) -> bool {
        self.member_body == Some(node.id())
    }

    /// The node `candidate` holds where `part`, this declaration's declarator,
    /// expects its kind: `$T $F() {}` reads the function under `int *f() {}`, as
    /// the pointer belongs to the declarator and the pattern does not spell it.
    pub(super) fn under_declarator<'tree>(
        &self,
        part: Node<'_>,
        candidate: Node<'tree>,
    ) -> Option<Node<'tree>> {
        if self.declarator != Some(part.id()) {
            return None;
        }
        let mut node = candidate;
        while node.kind() != part.kind() {
            node = node.child_by_field_name(DECLARATOR).or_else(|| {
                let named = named_children(node);
                match named.as_slice() {
                    [only] => Some(*only),
                    _ => None,
                }
            })?;
        }
        Some(node)
    }

    /// Whether a candidate's named child is left unsaid by the pattern children
    /// still to match: neither its field nor, without one, its kind is spelled
    /// there.
    pub(super) fn leaves_unsaid(&self, candidate: Node<'_>, pattern_rest: &[Node<'_>]) -> bool {
        if !candidate.is_named() {
            return false;
        }
        match self.field_of_candidate(candidate) {
            Some(field) => !pattern_rest
                .iter()
                .any(|part| self.field_of_pattern(*part) == Some(field)),
            None => !pattern_rest
                .iter()
                .any(|part| part.kind() == candidate.kind()),
        }
    }

    /// How a pattern child that may be `($$$)` reads against the candidate's
    /// next child: `($$$)` says any parameters, so however the grammar spells
    /// the slot satisfies it, and so does a grammar's leaving it out. A body
    /// or any other list keeps its own node: `{ $$$ }` is a block.
    pub(super) fn parameter_slot<'s>(
        &self,
        part: Node<'_>,
        candidate: Option<Node<'_>>,
        source: &'s str,
        expando: Expando,
    ) -> ParameterSlot<'s> {
        let [parameters, parameter] = self.parameter_fields;
        if parameters.is_none() || self.field_of_pattern(part) != parameters {
            return ParameterSlot::Usual;
        }
        let Some(SoleMulti(name)) = sole_multi(part, source, expando) else {
            return ParameterSlot::Usual;
        };
        if !self.candidate_has_parameters {
            return ParameterSlot::Absent(name);
        }
        match candidate {
            Some(candidate) if candidate.kind() != part.kind() => {
                let field = self.field_of_candidate(candidate);
                if field.is_some() && (field == parameters || field == parameter) {
                    ParameterSlot::Spelled(name)
                } else {
                    ParameterSlot::Usual
                }
            }
            _ => ParameterSlot::Usual,
        }
    }
}
