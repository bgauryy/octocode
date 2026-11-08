//! The members of an exported value (B122): `module.exports = v` exports
//! what `v` holds, followed through identifier chains only. A variable
//! initialized by another name, or by a call of the file's own function
//! whose every `return` gives one local, holds what that name holds; the
//! chain ends at a function or class. Its members are the `value.k = f`
//! statements written directly in the scope that declares the value (before
//! the function returns it), `f` a function declared in that scope.

use std::collections::{HashMap, HashSet};

use oxc_ast::{AstKind, ast::*};
use oxc_ast_visit::Visit;
use oxc_semantic::ScopeFlags;
use oxc_semantic::{NodeId, ScopeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use super::super::{inner, reference_symbol};

/// What a value export holds: the declaration the chain ends at, its members
/// by public name (each the binding span of the function it holds), and
/// whether the value escapes to code that may change its members.
pub(in crate::signatures) struct ValueMembers {
    pub(in crate::signatures) value: Span,
    pub(in crate::signatures) members: Vec<(Span, String)>,
    pub(in crate::signatures) escapes: bool,
}

/// The members of the value `identifier` names, when its chain ends at a
/// function or class declaration of this file.
pub(in crate::signatures) fn value_members(
    semantic: &Semantic<'_>,
    identifier: &IdentifierReference,
) -> Option<ValueMembers> {
    let scoping = semantic.scoping();
    let mut symbol = reference_symbol(scoping, identifier)?;
    let mut seen = HashSet::new();
    // The `return` statements of each function the chain passed through.
    let mut returns = Vec::<(SymbolId, Vec<Span>)>::new();
    let mut allowed = HashSet::<NodeId>::new();
    loop {
        if !seen.insert(symbol) {
            return None;
        }
        let declaration = scoping.symbol_declaration(symbol);
        match semantic.nodes().kind(declaration) {
            AstKind::Function(_) | AstKind::Class(_) => break,
            AstKind::VariableDeclarator(declarator) => {
                if scoping.symbol_is_mutated(symbol) {
                    return None;
                }
                match declarator.init.as_ref().map(inner)? {
                    Expression::FunctionExpression(_) | Expression::ClassExpression(_) => break,
                    Expression::Identifier(next) => {
                        allowed.insert(reference_node(semantic, next)?);
                        symbol = reference_symbol(scoping, next)?;
                    }
                    Expression::CallExpression(call) => {
                        let Expression::Identifier(callee) = inner(&call.callee) else {
                            return None;
                        };
                        let function = reference_symbol(scoping, callee)?;
                        let (returned, spans) = returned_local(semantic, function)?;
                        for reference in scoping.get_resolved_references(returned) {
                            if spans.iter().any(|span| {
                                span.contains_inclusive(semantic.nodes().kind(reference.node_id()).span())
                            }) {
                                allowed.insert(reference.node_id());
                            }
                        }
                        returns.push((returned, spans));
                        symbol = returned;
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    let scope = scoping.symbol_scope_id(symbol);
    let returned_at = returns
        .iter()
        .find(|(returned, _)| *returned == symbol)
        .and_then(|(_, spans)| spans.iter().map(|span| span.start).min());
    let mut keys = HashMap::<String, Option<Span>>::new();
    let mut escapes = false;
    for reference in scoping.get_resolved_references(symbol) {
        let node = reference.node_id();
        if allowed.contains(&node) {
            continue;
        }
        match member_write(semantic, node, scope, returned_at) {
            Some(Some((key, function))) => {
                keys.entry(key)
                    .and_modify(|held| *held = None)
                    .or_insert(Some(function));
            }
            Some(None) => {}
            None => escapes = true,
        }
    }
    let mut members = keys
        .into_iter()
        .filter_map(|(key, function)| Some((function?, key)))
        .collect::<Vec<_>>();
    members.sort_by_key(|(span, key)| (span.start, key.clone()));
    Some(ValueMembers {
        value: scoping.symbol_span(symbol),
        members,
        escapes,
    })
}

/// The node of an identifier reference.
fn reference_node(semantic: &Semantic<'_>, identifier: &IdentifierReference) -> Option<NodeId> {
    let id = identifier.reference_id.get()?;
    Some(semantic.scoping().get_reference(id).node_id())
}

/// What one reference to the value does: `Some(Some((k, f)))` for a
/// `value.k = f` statement of `scope` before `returned_at`, `f` a function
/// that scope declares; `Some(None)` for a member read or a write of another
/// shape (`value.k = 1`, `value.prototype.x = …`, one in a nested scope),
/// which changes no member a call can name; `None` when the value is passed
/// on, aliased or written through a computed key: anything may change.
fn member_write(
    semantic: &Semantic<'_>,
    reference: NodeId,
    scope: ScopeId,
    returned_at: Option<u32>,
) -> Option<Option<(String, Span)>> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let parent = nodes.parent_node(reference);
    let member = match parent.kind() {
        AstKind::StaticMemberExpression(member) => member,
        AstKind::ComputedMemberExpression(member) => {
            let written = matches!(nodes.parent_kind(parent.id()),
                AstKind::AssignmentExpression(assignment) if assignment.left.span() == member.span);
            return (!written).then_some(None);
        }
        _ => return None,
    };
    let AstKind::AssignmentExpression(assignment) = nodes.parent_kind(parent.id()) else {
        return Some(None);
    };
    if assignment.left.span() != member.span {
        return Some(None);
    }
    let statement = nodes.parent_node(nodes.parent_id(parent.id()));
    let in_scope = matches!(statement.kind(), AstKind::ExpressionStatement(_))
        && statement.scope_id() == scope
        && returned_at.is_none_or(|end| assignment.span.end <= end);
    let Expression::Identifier(value) = inner(&assignment.right) else {
        return Some(None);
    };
    let function = reference_symbol(scoping, value).filter(|function| {
        scoping.symbol_scope_id(*function) == scope
            && matches!(
                nodes.kind(scoping.symbol_declaration(*function)),
                AstKind::Function(_)
            )
    });
    Some(
        function
            .filter(|_| in_scope && assignment.operator == AssignmentOperator::Assign)
            .map(|function| {
                (
                    member.property.name.as_str().to_owned(),
                    scoping.symbol_span(function),
                )
            }),
    )
}

/// The one local every top-level `return` of `function`'s body gives, and
/// those statements' spans; a nested function's returns are its own.
fn returned_local(semantic: &Semantic<'_>, function: SymbolId) -> Option<(SymbolId, Vec<Span>)> {
    let scoping = semantic.scoping();
    let body = match semantic.nodes().kind(scoping.symbol_declaration(function)) {
        AstKind::Function(function) => function.body.as_ref()?,
        AstKind::VariableDeclarator(declarator) if !scoping.symbol_is_mutated(function) => {
            match declarator.init.as_ref().map(inner)? {
                Expression::FunctionExpression(function) => function.body.as_ref()?,
                _ => return None,
            }
        }
        _ => return None,
    };
    let mut returns = Returns {
        scoping,
        found: Vec::new(),
    };
    for statement in &body.statements {
        returns.visit_statement(statement);
    }
    let mut returned = None;
    let mut spans = Vec::new();
    for (span, symbol) in returns.found {
        let symbol = symbol?;
        if returned.is_some_and(|known| known != symbol) {
            return None;
        }
        returned = Some(symbol);
        spans.push(span);
    }
    Some((returned?, spans))
}

/// The `return` statements of one function body, each with the local it
/// returns (`None`: any other value); a nested function's are its own.
struct Returns<'s> {
    scoping: &'s oxc_semantic::Scoping,
    found: Vec<(Span, Option<SymbolId>)>,
}

impl<'a> Visit<'a> for Returns<'_> {
    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        let symbol = match it.argument.as_ref().map(inner) {
            Some(Expression::Identifier(value)) => reference_symbol(self.scoping, value),
            _ => None,
        };
        self.found.push((it.span, symbol));
    }
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _it: &Class<'a>) {}
}
