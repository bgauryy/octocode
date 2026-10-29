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
        term_binding: false,
        self_template: None,
        macros: Vec::new(),
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
        Expression::StaticMemberExpression(member) => Some(format!(
            "{}.{}",
            receiver_label(&member.object),
            member.property.name.as_str()
        )),
        Expression::ComputedMemberExpression(member) => Some(format!(
            "{}.{}",
            receiver_label(&member.object),
            computed_key(&member.expression)?
        )),
        Expression::ParenthesizedExpression(paren) => callee_name(&paren.expression),
        Expression::TSAsExpression(ts) => callee_name(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => callee_name(&ts.expression),
        Expression::TSTypeAssertion(ts) => callee_name(&ts.expression),
        Expression::TSNonNullExpression(ts) => callee_name(&ts.expression),
        Expression::TSInstantiationExpression(ts) => callee_name(&ts.expression),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(_) => None,
            ChainElement::TSNonNullExpression(non_null) => callee_name(&non_null.expression),
            other => other.as_member_expression().and_then(member_callee),
        },
        _ => None,
    }
}

fn member_callee(member: &MemberExpression) -> Option<String> {
    let property = match member {
        MemberExpression::StaticMemberExpression(member) => member.property.name.as_str().into(),
        MemberExpression::ComputedMemberExpression(member) => computed_key(&member.expression)?,
        MemberExpression::PrivateFieldExpression(member) => {
            format!("#{}", member.field.name.as_str())
        }
    };
    Some(format!("{}.{property}", receiver_label(member.object())))
}

/// A computed member key written as a constant string: `a['b']`, `` a[`b`] ``.
fn computed_key(key: &Expression) -> Option<String> {
    match key {
        Expression::StringLiteral(lit) => Some(lit.value.as_str().to_string()),
        Expression::TemplateLiteral(lit) if lit.expressions.is_empty() => lit
            .quasis
            .first()
            .and_then(|q| q.value.cooked.as_ref())
            .map(|cooked| cooked.as_str().to_string()),
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

/// A member call's receiver as written, never dropped: a chained receiver
/// keeps its path (`this.app.scene`), and argument lists, indexes and values
/// without a name are elided to their brackets (`get()`, `list[]`, `()`), as
/// the tree-sitter lanes elide them.
fn receiver_label(expr: &Expression) -> String {
    match expr {
        Expression::Identifier(identifier) => identifier.name.as_str().to_string(),
        Expression::ThisExpression(_) => "this".to_string(),
        Expression::Super(_) => "super".to_string(),
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => expr
            .as_member_expression()
            .map_or_else(|| "()".to_string(), member_label),
        Expression::CallExpression(call) => format!("{}()", receiver_label(&call.callee)),
        Expression::NewExpression(construct) => {
            format!("new {}()", receiver_label(&construct.callee))
        }
        Expression::ArrayExpression(_) => "[]".to_string(),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => format!("{}()", receiver_label(&call.callee)),
            ChainElement::TSNonNullExpression(non_null) => receiver_label(&non_null.expression),
            other => other
                .as_member_expression()
                .map_or_else(|| "()".to_string(), member_label),
        },
        Expression::ParenthesizedExpression(paren) => receiver_label(&paren.expression),
        Expression::TSAsExpression(ts) => receiver_label(&ts.expression),
        Expression::TSSatisfiesExpression(ts) => receiver_label(&ts.expression),
        Expression::TSTypeAssertion(ts) => receiver_label(&ts.expression),
        Expression::TSNonNullExpression(ts) => receiver_label(&ts.expression),
        Expression::TSInstantiationExpression(ts) => receiver_label(&ts.expression),
        _ => "()".to_string(),
    }
}

fn member_label(member: &MemberExpression) -> String {
    let object = receiver_label(member.object());
    match member {
        MemberExpression::StaticMemberExpression(member) => {
            format!("{object}.{}", member.property.name.as_str())
        }
        MemberExpression::ComputedMemberExpression(member) => {
            match computed_key(&member.expression) {
                Some(key) => format!("{object}.{key}"),
                None => format!("{object}[]"),
            }
        }
        MemberExpression::PrivateFieldExpression(member) => {
            format!("{object}.#{}", member.field.name.as_str())
        }
    }
}
