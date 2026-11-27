//! The census of a module's `module` and `exports` occurrences: which writes
//! the export shapes may count, and which uses leave its exports unprovable.

use std::collections::HashMap;

use oxc_ast::{AstKind, ast::*};
use oxc_semantic::{Scoping, Semantic};
use oxc_span::{GetSpan, Span};

use super::super::{inner, reference_symbol};

/// What a member access reaches.
#[derive(Clone, Copy)]
pub(super) enum Target<'a> {
    /// `module.exports` itself.
    Whole,
    /// A key of `exports` (`true`) or of `module.exports` (`false`); `None`
    /// when the key is computed.
    Key(Option<&'a str>, bool),
}

/// What every `module`/`exports` occurrence in the file allows.
#[derive(Default)]
pub(super) struct Census<'a> {
    pub(super) whole_writes: usize,
    pub(super) key_writes: HashMap<&'a str, usize>,
    pub(super) exports_rebinds: usize,
    pub(super) first_write: Option<Span>,
    /// `module` or `module.exports` may be replaced: nothing is provable.
    pub(super) unprovable: bool,
    /// Some key may change outside a counted write: no key is provable.
    pub(super) keys_unknown: bool,
}

/// One walk over every node: counted writes, and every use of `module`,
/// `exports` or `module.exports` that could change them uncounted.
pub(super) fn census<'a>(semantic: &Semantic<'a>) -> Census<'a> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut census = Census::default();
    for node in nodes.iter() {
        let parent = nodes.parent_kind(node.id());
        if let AstKind::IdentifierReference(identifier) = node.kind() {
            let Some(module) = global(scoping, identifier) else {
                continue;
            };
            let span = identifier.span;
            match parent {
                // A member access is classified as a target below.
                AstKind::StaticMemberExpression(member) if member.object.span() == span => {}
                AstKind::ComputedMemberExpression(member) if member.object.span() == span => {
                    // `module[name]` may name `exports`.
                    census.unprovable |= module && member.static_property_name().is_none();
                }
                AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {}
                AstKind::AssignmentExpression(assignment)
                    if !module && assignment.left.span() == span =>
                {
                    census.exports_rebinds += 1;
                }
                AstKind::CallExpression(call) if !module && marks_es_module(call, span) => {}
                // Rebound, aliased or passed on: anything may change.
                _ if module => census.unprovable = true,
                _ => census.keys_unknown = true,
            }
            continue;
        }
        let (object, property, span) = match node.kind() {
            AstKind::StaticMemberExpression(member) => (
                &member.object,
                Some(member.property.name.as_str()),
                member.span,
            ),
            AstKind::ComputedMemberExpression(member) => (
                &member.object,
                member.static_property_name().map(|name| name.as_str()),
                member.span,
            ),
            _ => continue,
        };
        let Some(target) = target(scoping, object, property) else {
            continue;
        };
        let whole = matches!(target, Target::Whole);
        match parent {
            AstKind::AssignmentExpression(assignment) if assignment.left.span() == span => {
                census.first_write.get_or_insert(span);
                match target {
                    Target::Whole => census.whole_writes += 1,
                    Target::Key(Some(key), _) => *census.key_writes.entry(key).or_default() += 1,
                    Target::Key(None, _) => census.keys_unknown = true,
                }
            }
            AstKind::UpdateExpression(_)
            | AstKind::ArrayAssignmentTarget(_)
            | AstKind::ObjectAssignmentTarget(_)
            | AstKind::AssignmentTargetRest(_)
            | AstKind::AssignmentTargetWithDefault(_)
            | AstKind::AssignmentTargetPropertyProperty(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_) => {
                if whole {
                    census.unprovable = true;
                } else {
                    census.keys_unknown = true;
                }
            }
            AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
                census.keys_unknown = true;
            }
            // A key read, or `module.exports` reached through a member.
            _ if !whole => {}
            AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::UnaryExpression(_) => {}
            AstKind::CallExpression(call) if marks_es_module(call, span) => {}
            // `module.exports` read as a value may be mutated elsewhere.
            _ => census.keys_unknown = true,
        }
    }
    census
}

/// `Some(true)` for an unshadowed `module`, `Some(false)` for `exports`.
pub(super) fn global(scoping: &Scoping, identifier: &IdentifierReference) -> Option<bool> {
    let module = match identifier.name.as_str() {
        "module" => true,
        "exports" => false,
        _ => return None,
    };
    reference_symbol(scoping, identifier)
        .is_none()
        .then_some(module)
}

/// What `object.property` reaches, when `object` is `module`, `exports` or
/// `module.exports`.
pub(super) fn target<'a>(
    scoping: &Scoping,
    object: &'a Expression<'a>,
    property: Option<&'a str>,
) -> Option<Target<'a>> {
    match inner(object) {
        Expression::Identifier(identifier) => match global(scoping, identifier)? {
            true => (property == Some("exports")).then_some(Target::Whole),
            false => Some(Target::Key(property, true)),
        },
        object => {
            let member = object.as_member_expression()?;
            let Expression::Identifier(identifier) = inner(member.object()) else {
                return None;
            };
            (global(scoping, identifier) == Some(true)
                && member.static_property_name() == Some("exports"))
            .then_some(Target::Key(property, false))
        }
    }
}

/// `Object.defineProperty(<span>, "__esModule", …)`: the interop marker
/// compilers emit, which defines no export name.
fn marks_es_module(call: &CallExpression, span: Span) -> bool {
    matches!(inner(&call.callee), Expression::StaticMemberExpression(callee)
        if callee.property.name == "defineProperty"
            && matches!(inner(&callee.object), Expression::Identifier(object) if object.name == "Object"))
        && call.arguments.len() >= 2
        && call.arguments[0].span() == span
        && matches!(call.arguments[1].as_expression().map(inner),
            Some(Expression::StringLiteral(literal)) if literal.value == "__esModule")
}
