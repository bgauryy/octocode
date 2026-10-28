//! Callee, JSX component and base-type names as written.

use crate::graph::GraphFactCall;
use crate::signatures::js_oxc_shared::SpanPositions;
use oxc_ast::ast::*;
use oxc_span::Span;

pub(super) fn push_call(
    owner: &str,
    callee: String,
    span: Span,
    li: &SpanPositions,
    kind: &'static str,
    receiver_type: Option<String>,
    calls: &mut Vec<GraphFactCall>,
) {
    let range = li.range(span);
    let line = range.start.line + 1;
    let id_prefix = if kind == "constructs" {
        "construct"
    } else {
        "call"
    };
    calls.push(GraphFactCall {
        binding: crate::graph::GraphCallBinding::SyntaxLookup,
        id: format!("{id_prefix}:{owner}:{callee}:{line}"),
        caller: owner.to_string(),
        caller_id: None,
        callee,
        line,
        source_range: Some(crate::text::analysis::ObservedSpan::root(
            span.start, span.end,
        )),
        parse_assessment: crate::graph::GraphParseAssessment::parsed(range.clone()),
        range,
        kind: kind.to_owned(),
        receiver_type,
    });
}

pub(in crate::signatures) fn unwrap_expr<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(paren) => unwrap_expr(&paren.expression),
        Expression::TSAsExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSTypeAssertion(ts) => unwrap_expr(&ts.expression),
        Expression::TSNonNullExpression(ts) => unwrap_expr(&ts.expression),
        Expression::TSInstantiationExpression(ts) => unwrap_expr(&ts.expression),
        other => other,
    }
}

pub(super) fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::StaticMemberExpression(member) => {
            let property = member.property.name.as_str();
            Some(
                expression_name(&member.object)
                    .map(|object| format!("{object}.{property}"))
                    .unwrap_or_else(|| property.to_string()),
            )
        }
        Expression::ComputedMemberExpression(member) => {
            let property = match &member.expression {
                Expression::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
                Expression::TemplateLiteral(lit) if lit.expressions.is_empty() => lit
                    .quasis
                    .first()
                    .and_then(|q| q.value.cooked.as_ref())
                    .map(|cooked| cooked.as_str().to_string()),
                _ => None,
            }?;
            Some(
                expression_name(&member.object)
                    .map(|object| format!("{object}.{property}"))
                    .unwrap_or(property),
            )
        }
        Expression::ParenthesizedExpression(paren) => callee_name(&paren.expression),
        Expression::TSAsExpression(ts) => callee_name(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => callee_name(&ts.expression),
        Expression::TSTypeAssertion(ts) => callee_name(&ts.expression),
        Expression::TSNonNullExpression(ts) => callee_name(&ts.expression),
        Expression::TSInstantiationExpression(ts) => callee_name(&ts.expression),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(_) => None,
            ChainElement::TSNonNullExpression(non_null) => callee_name(&non_null.expression),
            other => other
                .as_member_expression()
                .and_then(|member| match member {
                    oxc_ast::ast::MemberExpression::StaticMemberExpression(static_member) => {
                        let property = static_member.property.name.as_str();
                        Some(
                            expression_name(&static_member.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or_else(|| property.to_string()),
                        )
                    }
                    oxc_ast::ast::MemberExpression::ComputedMemberExpression(computed) => {
                        let property = match &computed.expression {
                            Expression::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
                            _ => None,
                        }?;
                        Some(
                            expression_name(&computed.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or(property),
                        )
                    }
                    oxc_ast::ast::MemberExpression::PrivateFieldExpression(private) => {
                        let property = format!("#{}", private.field.name.as_str());
                        Some(
                            expression_name(&private.object)
                                .map(|object| format!("{object}.{property}"))
                                .unwrap_or(property),
                        )
                    }
                }),
        },
        _ => None,
    }
}

/// A JSX tag that names a component: `Foo`, `Foo.Bar`, `this.Foo`. Intrinsic
/// tags (`div`, `my-element`), namespaced XML names (`svg:rect`) and `<this>`
/// render no user component.
pub(super) fn jsx_component_name(name: &JSXElementName) -> Option<String> {
    match name {
        JSXElementName::IdentifierReference(reference) => {
            let name = reference.name.as_str();
            (!name.starts_with(|c: char| c.is_ascii_lowercase())).then(|| name.to_string())
        }
        JSXElementName::MemberExpression(member) => Some(jsx_member_name(member)),
        JSXElementName::Identifier(_)
        | JSXElementName::NamespacedName(_)
        | JSXElementName::ThisExpression(_) => None,
    }
}

fn jsx_member_name(member: &JSXMemberExpression) -> String {
    let object = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(reference) => {
            reference.name.as_str().to_string()
        }
        JSXMemberExpressionObject::MemberExpression(inner) => jsx_member_name(inner),
        JSXMemberExpressionObject::ThisExpression(_) => "this".to_string(),
    };
    format!("{object}.{}", member.property.name.as_str())
}

/// A base class written as a (possibly dotted) name: `Base`, `ns.sub.Base`.
pub(in crate::signatures) fn dotted_expression_name(expr: &Expression) -> Option<String> {
    match unwrap_expr(expr) {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::StaticMemberExpression(member) => Some(format!(
            "{}.{}",
            dotted_expression_name(&member.object)?,
            member.property.name.as_str()
        )),
        _ => None,
    }
}

/// A TS type reference name as written: `C`, `ns.C` (type arguments are a
/// separate node and never included).
pub(in crate::signatures) fn ts_type_name(name: &TSTypeName) -> Option<String> {
    match name {
        TSTypeName::IdentifierReference(reference) => Some(reference.name.as_str().to_string()),
        TSTypeName::QualifiedName(qualified) => Some(format!(
            "{}.{}",
            ts_type_name(&qualified.left)?,
            qualified.right.name.as_str()
        )),
        TSTypeName::ThisExpression(_) => None,
    }
}

fn expression_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::ThisExpression(_) => Some("this".to_string()),
        Expression::Super(_) => Some("super".to_string()),
        _ => None,
    }
}
