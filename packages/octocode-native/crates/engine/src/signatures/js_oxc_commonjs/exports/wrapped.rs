//! The whole export of a UMD-wrapped module (B122): a module whose body is
//! one function called in place (`;(function () { … }.call(this))`) that
//! writes `module.exports` once, at the wrapper's statement level or in its
//! `if` branches, directly or through an alias of `module` (`var m = … &&
//! module`). Every other use of `module` and of the alias must leave
//! `module.exports` alone: a `typeof`, a member read, a test. A write in a
//! branch holds only when that branch runs.

use oxc_ast::{AstKind, ast::*};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use super::super::{inner, reference_symbol};

/// The value a wrapper writes to `module.exports`, and whether the write sits
/// in a branch.
pub(in crate::signatures) struct WrappedWhole<'a> {
    pub(in crate::signatures) value: &'a IdentifierReference<'a>,
    pub(in crate::signatures) conditional: bool,
}

pub(in crate::signatures) fn wrapped_whole<'a>(
    semantic: &Semantic<'a>,
    program: &'a Program<'a>,
) -> Option<WrappedWhole<'a>> {
    let body = wrapper_body(program)?;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let alias = |identifier: &IdentifierReference| {
        reference_symbol(scoping, identifier).filter(|symbol| module_alias(semantic, *symbol))
    };
    let is_module = |identifier: &IdentifierReference| {
        identifier.name == "module" && reference_symbol(scoping, identifier).is_none()
    };
    let mut writes = Vec::<(&'a AssignmentExpression<'a>, &'a Expression<'a>)>::new();
    for node in nodes.iter() {
        let AstKind::IdentifierReference(identifier) = node.kind() else {
            continue;
        };
        let holder = if is_module(identifier) {
            None
        } else if let Some(symbol) = alias(identifier) {
            Some(symbol)
        } else {
            continue;
        };
        let span = identifier.span;
        match nodes.parent_kind(node.id()) {
            AstKind::UnaryExpression(_)
            | AstKind::IfStatement(_)
            | AstKind::ConditionalExpression(_) => {}
            // The last operand of an alias declarator, or a test.
            AstKind::LogicalExpression(_) => {}
            AstKind::StaticMemberExpression(member) if member.object.span() == span => {
                let parent = nodes.parent_id(nodes.parent_id(node.id()));
                let written = match nodes.kind(parent) {
                    AstKind::AssignmentExpression(assignment)
                        if assignment.left.span() == member.span =>
                    {
                        Some(assignment)
                    }
                    AstKind::UpdateExpression(_) => return None,
                    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
                        return None;
                    }
                    _ => None,
                };
                if member.property.name == "exports" {
                    match written {
                        Some(assignment) => writes.push((assignment, &assignment.right)),
                        // `m.exports.k = …` or `m.exports` passed on.
                        None if exports_escape(semantic, nodes.parent_id(node.id())) => {
                            return None;
                        }
                        None => {}
                    }
                } else if written.is_some() {
                    return None;
                }
            }
            // `var m = module` declares the alias.
            AstKind::VariableDeclarator(_) if holder.is_none() => {}
            _ => return None,
        }
    }
    let [(write, value)] = writes.as_slice() else {
        return None;
    };
    if write.operator != AssignmentOperator::Assign {
        return None;
    }
    let Expression::Identifier(value) = inner(value) else {
        return None;
    };
    let conditional = branch_of(&body.statements, write.span, false)?;
    Some(WrappedWhole { value, conditional })
}

/// The body of the one function the program calls in place.
fn wrapper_body<'a>(program: &'a Program<'a>) -> Option<&'a FunctionBody<'a>> {
    let mut statements = program
        .body
        .iter()
        .filter(|statement| !matches!(statement, Statement::EmptyStatement(_)));
    let (Some(Statement::ExpressionStatement(statement)), None) =
        (statements.next(), statements.next())
    else {
        return None;
    };
    let mut expression = inner(&statement.expression);
    if let Expression::UnaryExpression(unary) = expression {
        expression = inner(&unary.argument);
    }
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let callee = match inner(&call.callee) {
        Expression::StaticMemberExpression(member)
            if matches!(member.property.name.as_str(), "call" | "apply") =>
        {
            inner(&member.object)
        }
        callee => callee,
    };
    match callee {
        Expression::FunctionExpression(function) => function.body.as_deref(),
        _ => None,
    }
}

/// `var m = <tests> && module`: a never-written variable whose initializer is
/// `module` or a `&&` chain that ends with it.
fn module_alias(semantic: &Semantic<'_>, symbol: SymbolId) -> bool {
    let scoping = semantic.scoping();
    if scoping.symbol_is_mutated(symbol) {
        return false;
    }
    let AstKind::VariableDeclarator(declarator) =
        semantic.nodes().kind(scoping.symbol_declaration(symbol))
    else {
        return false;
    };
    let mut value = declarator.init.as_ref().map(inner);
    while let Some(Expression::LogicalExpression(logical)) = value {
        if logical.operator != LogicalOperator::And {
            return false;
        }
        value = Some(inner(&logical.right));
    }
    matches!(value, Some(Expression::Identifier(identifier))
        if identifier.name == "module" && reference_symbol(scoping, identifier).is_none())
}

/// Whether the `m.exports` member at `member` is written through or passed
/// on: only a comparison or a test reads it harmlessly.
fn exports_escape(semantic: &Semantic<'_>, member: oxc_semantic::NodeId) -> bool {
    !matches!(
        semantic.nodes().parent_kind(member),
        AstKind::BinaryExpression(_)
            | AstKind::LogicalExpression(_)
            | AstKind::IfStatement(_)
            | AstKind::UnaryExpression(_)
    )
}

/// Whether the statement holding `span` sits in an `if` branch (`Some(true)`)
/// or directly in `statements` (`Some(false)`); `None` anywhere else.
fn branch_of(statements: &[Statement], span: Span, in_branch: bool) -> Option<bool> {
    let statement = statements
        .iter()
        .find(|statement| statement.span().contains_inclusive(span))?;
    match statement {
        Statement::ExpressionStatement(_) => Some(in_branch),
        Statement::BlockStatement(block) => branch_of(&block.body, span, in_branch),
        Statement::IfStatement(statement) => {
            let branches = std::iter::once(&statement.consequent).chain(&statement.alternate);
            let branches = branches.collect::<Vec<_>>();
            let branch = branches
                .into_iter()
                .find(|branch| branch.span().contains_inclusive(span))?;
            branch_of(std::slice::from_ref(branch), span, true)
        }
        _ => None,
    }
}
