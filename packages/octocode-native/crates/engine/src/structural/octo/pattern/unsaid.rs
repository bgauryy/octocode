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

use std::cell::RefCell;
use std::num::NonZeroU16;

use tree_sitter::{Language, Node};

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

/// A language's field ids by field name, `None` where the grammar lacks it.
type FieldIds = Vec<(Language, Vec<(&'static str, Option<u16>)>)>;

thread_local! {
    /// Field ids by language and field name: `child_by_field_name` scans the
    /// language's whole field table on every call, and a match asks the same
    /// few names at every node pair.
    static FIELD_IDS: RefCell<FieldIds> =
        const { RefCell::new(Vec::new()) };
}

/// `node`'s child in the field `name`, as [`Node::child_by_field_name`] reads
/// it, through the cached field id.
fn child_in_field<'tree>(node: Node<'tree>, name: &'static str) -> Option<Node<'tree>> {
    let id = FIELD_IDS.with_borrow_mut(|languages| {
        let language = node.language();
        let at = match languages.iter().position(|(known, _)| *known == *language) {
            Some(at) => at,
            None => {
                languages.push((language.clone(), Vec::new()));
                languages.len() - 1
            }
        };
        let names = &mut languages[at].1;
        if let Some((_, id)) = names.iter().find(|(known, _)| *known == name) {
            return *id;
        }
        let id = language.field_id_for_name(name).map(NonZeroU16::get);
        names.push((name, id));
        id
    })?;
    node.child_by_field_id(id)
}

fn has_field(node: Node<'_>, fields: &[&'static str]) -> bool {
    fields
        .iter()
        .any(|field| child_in_field(node, field).is_some())
}

/// Whether the grammar spells `node` as a declaration, by the fields this
/// spelling of it fills: parameters, or a binder and what it binds. A node
/// with arguments is a call, whatever else it fills.
pub(super) fn declares(node: Node<'_>) -> bool {
    if has_field(node, &["arguments"]) {
        return false;
    }
    has_field(node, &[PARAMETERS, PARAMETER])
        || types_declarators(node)
        || holds_typed_declarators(node)
        || (has_field(node, &["name"]) && has_field(node, &["body", "value", "type"]))
        || (has_field(node, &["pattern"]) && has_field(node, &["value"]))
        || (has_field(node, &["type"]) && has_field(node, &["body", DECLARATOR]))
        || (has_field(node, &[DECLARATOR]) && has_field(node, &["body"]))
}

/// Whether `node` binds a `type` to the declarators after it, each naming
/// what it types in no field of `node` (C# `variable_declaration`: `int a =
/// 1, b`). A declarator in a `declarator` field (a Java local) makes its
/// node a declaration by itself, and a statement list ending in one (a Java
/// block) holds it rather than declares through it.
fn types_declarators(node: Node<'_>) -> bool {
    let fielded = fields_of(node);
    has_field(node, &["type"])
        && named_children(node)
            .iter()
            .any(|part| has_field(*part, &["name"]) && field_of(&fielded, *part).is_none())
}

/// Whether `node` declares through the typed declarators it ends with,
/// naming no part of its own: the statement or member around them, whose
/// other parts (modifiers) a pattern may leave unsaid (C#
/// `local_declaration_statement` and `field_declaration` around a
/// `variable_declaration`).
fn holds_typed_declarators(node: Node<'_>) -> bool {
    fields_of(node).is_empty()
        && named_children(node)
            .last()
            .is_some_and(|last| types_declarators(*last))
}

/// Whether `candidate_kind` is `pattern`'s kind with one more leading word, the
/// grammar's own longer form of that declaration
/// (`generator_function_declaration`, `abstract_class_declaration`).
pub(super) fn is_longer_form(pattern: Node<'_>, candidate_kind: &str) -> bool {
    adds_a_leading_word(pattern.kind(), candidate_kind) && declares(pattern)
}

/// Whether `longer` is `kind` with one more leading word.
pub(super) fn adds_a_leading_word(kind: &str, longer: &str) -> bool {
    kind.contains('_')
        && longer
            .strip_suffix(kind)
            .and_then(|word| word.strip_suffix('_'))
            .is_some_and(|word| !word.is_empty() && !word.contains('_'))
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
    /// The candidate is of another kind than the pattern, one the match
    /// accepts as it (a longer form or an alternate): its own tokens spell
    /// what its kind adds (`function*`).
    other_kind: bool,
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

/// Whether `node` takes parameters, itself or through the declarators it
/// nests its name in (a C function's `function_declarator`): its body is
/// then statements, never a type's members.
fn takes_parameters(node: Node<'_>) -> bool {
    std::iter::successors(Some(node), |node| child_in_field(*node, DECLARATOR))
        .any(|node| has_field(node, &[PARAMETERS, PARAMETER]))
}

/// Whether `list` is the member list of a declaration that takes no
/// parameters, held through wrappers that hold only it: Go's
/// `field_declaration_list` in the `struct_type` a `type_spec` types. A
/// list beside other parts (an `if`'s block) or in a function is not.
fn is_held_member_list(list: Node<'_>) -> bool {
    let mut node = list;
    while let Some(parent) = node.parent() {
        if declares(parent) {
            return field_of(&fields_of(parent), node).is_some() && !takes_parameters(parent);
        }
        if named_children(parent).len() != 1 {
            return false;
        }
        node = parent;
    }
    false
}

/// `body`, when its members are all declarations or named parts (an enum's
/// variants) and at least one is spelled.
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
        if !declares(member) && !has_field(member, &["name"]) {
            return false;
        }
        spelled = true;
    }
    spelled
}

impl Unsaid {
    /// Out of line: [`super::CompiledPattern::match_node`] recurses, and its
    /// frame must stay small on deep patterns.
    #[inline(never)]
    pub(super) fn of(
        pattern: Node<'_>,
        candidate: Node<'_>,
        source: &str,
        expando: Expando,
    ) -> Self {
        // A pattern that spells no part that makes it a declaration (`impl X
        // for $T`, no body) leaves the rest of a declaration unsaid too.
        if !declares(pattern) && !declares(candidate) {
            if lists_members(pattern, source, expando) && is_held_member_list(pattern) {
                return Self::Members;
            }
            return Self::Exact;
        }
        let member_body = child_in_field(pattern, "body")
            .filter(|body| !takes_parameters(pattern) && lists_members(*body, source, expando))
            .map(|body| body.id());
        Self::Declaration(Declaration {
            parameter_fields: [PARAMETERS, PARAMETER]
                .map(|field| pattern.language().field_id_for_name(field)),
            pattern_fields: fields_of(pattern),
            candidate_fields: fields_of(candidate),
            candidate_has_parameters: has_field(candidate, &[PARAMETERS, PARAMETER]),
            member_body,
            declarator: child_in_field(pattern, "type")
                .is_none_or(|spelled| meta_from_node(spelled, source, expando).is_some())
                .then(|| child_in_field(pattern, DECLARATOR))
                .flatten()
                .map(|node| node.id()),
            other_kind: pattern.kind_id() != candidate.kind_id(),
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

    /// Whether the candidate is of another kind the match accepts as the
    /// pattern's (a longer form or an alternate).
    pub(super) fn is_other_kind(&self) -> bool {
        self.other_kind
    }

    /// Whether the candidate child `node` is in a parameter field: its lists
    /// (Scala's `[T]` and `(x: T)`, curried lists) answer to the pattern's
    /// parameter slot ([`ParameterSlot`]), not to the one-item rule.
    pub(super) fn holds_parameters(&self, node: Node<'_>) -> bool {
        field_of(&self.candidate_fields, node)
            .is_some_and(|field| self.parameter_fields.contains(&Some(field)))
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
            node = child_in_field(node, DECLARATOR).or_else(|| {
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
