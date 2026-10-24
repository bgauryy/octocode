//! Document symbols of each declaration form: classes, interfaces, enums,
//! namespaces, modules, type aliases and variables.

use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Expression, MethodDefinitionKind, TSEnumDeclaration,
    TSEnumMemberName, TSExternalModuleDeclaration, TSGlobalDeclaration, TSInterfaceDeclaration,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSSignature, TSTypeAliasDeclaration,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_span::Span;

use crate::signatures::js_oxc_shared::{SpanPositions, property_key_name};

use super::walk::{collect_statement, function_value_children, nested_in_function, push_opt};
use super::{DocumentSymbol, TYPE_ALIAS_KIND};
use crate::lsp::symbol_kind;

pub(super) fn class_symbol(class: &Class, li: &SpanPositions) -> Option<DocumentSymbol> {
    let id = class.id.as_ref()?;
    let mut children = Vec::new();
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(m) => {
                let symbol_kind = match m.kind {
                    MethodDefinitionKind::Constructor => symbol_kind::CONSTRUCTOR,
                    _ => symbol_kind::METHOD,
                };
                if let Some((name, name_span)) = property_key_name(&m.key) {
                    children.push(container(
                        &name,
                        symbol_kind,
                        m.span,
                        name_span,
                        nested_in_function(&m.value, li),
                        li,
                    ));
                }
            }
            ClassElement::PropertyDefinition(p) => {
                if let Some((name, name_span)) = property_key_name(&p.key) {
                    let nested = p
                        .value
                        .as_ref()
                        .and_then(|value| function_value_children(value, li))
                        .unwrap_or_default();
                    // A field holding a function is a callable member.
                    let member_kind = match p.value.as_ref().map(bound_value) {
                        Some(
                            Expression::ArrowFunctionExpression(_)
                            | Expression::FunctionExpression(_),
                        ) => symbol_kind::METHOD,
                        _ => symbol_kind::PROPERTY,
                    };
                    children.push(container(&name, member_kind, p.span, name_span, nested, li));
                }
            }
            ClassElement::AccessorProperty(a) => {
                if let Some((name, name_span)) = property_key_name(&a.key) {
                    children.push(leaf(&name, symbol_kind::PROPERTY, a.span, name_span, li));
                }
            }
            _ => {}
        }
    }
    // The decorator list is part of the class even when it is written before
    // `export`, outside the class node's own span.
    let start = class
        .decorators
        .iter()
        .map(|decorator| decorator.span.start)
        .fold(class.span.start, u32::min);
    Some(container(
        id.name.as_str(),
        symbol_kind::CLASS,
        Span::new(start, class.span.end),
        id.span,
        children,
        li,
    ))
}

pub(super) fn interface_symbol(
    iface: &TSInterfaceDeclaration,
    li: &SpanPositions,
) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for signature in &iface.body.body {
        match signature {
            TSSignature::TSPropertySignature(p) => {
                if let Some((name, name_span)) = property_key_name(&p.key) {
                    children.push(leaf(&name, symbol_kind::PROPERTY, p.span, name_span, li));
                }
            }
            TSSignature::TSMethodSignature(m) => {
                if let Some((name, name_span)) = property_key_name(&m.key) {
                    children.push(leaf(&name, symbol_kind::METHOD, m.span, name_span, li));
                }
            }
            _ => {}
        }
    }
    Some(container(
        iface.id.name.as_str(),
        symbol_kind::INTERFACE,
        iface.span,
        iface.id.span,
        children,
        li,
    ))
}

pub(super) fn enum_symbol(decl: &TSEnumDeclaration, li: &SpanPositions) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for member in &decl.body.members {
        if let Some((name, name_span)) = enum_member_name(&member.id) {
            children.push(leaf(
                &name,
                symbol_kind::ENUM_MEMBER,
                member.span,
                name_span,
                li,
            ));
        }
    }
    Some(container(
        decl.id.name.as_str(),
        symbol_kind::ENUM,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

pub(super) fn namespace_symbol(
    decl: &TSNamespaceDeclaration,
    li: &SpanPositions,
) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    match &decl.body {
        TSNamespaceDeclarationBody::TSModuleBlock(block) => {
            for stmt in &block.body {
                collect_statement(stmt, li, &mut children);
            }
        }
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
            push_opt(&mut children, namespace_symbol(inner, li));
        }
    }
    Some(container(
        decl.id.name.as_str(),
        symbol_kind::NAMESPACE,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

pub(super) fn external_module_symbol(
    decl: &TSExternalModuleDeclaration,
    li: &SpanPositions,
) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    if let Some(body) = &decl.body {
        for stmt in &body.body {
            collect_statement(stmt, li, &mut children);
        }
    }
    Some(container(
        decl.id.value.as_str(),
        symbol_kind::NAMESPACE,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

pub(super) fn global_symbol(
    decl: &TSGlobalDeclaration,
    li: &SpanPositions,
) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for stmt in &decl.body.body {
        collect_statement(stmt, li, &mut children);
    }
    Some(container(
        "global",
        symbol_kind::NAMESPACE,
        decl.span,
        decl.global_span,
        children,
        li,
    ))
}

pub(super) fn type_alias_symbol(
    decl: &TSTypeAliasDeclaration,
    li: &SpanPositions,
) -> Option<DocumentSymbol> {
    Some(leaf(
        decl.id.name.as_str(),
        TYPE_ALIAS_KIND,
        decl.span,
        decl.id.span,
        li,
    ))
}

/// Emit one document symbol per identifier bound by a destructuring pattern,
/// recursing through nested object/array patterns, rest elements, and defaults.
fn push_pattern_leaves(
    pattern: &BindingPattern,
    symbol_kind: u8,
    li: &SpanPositions,
    out: &mut Vec<DocumentSymbol>,
) {
    match pattern {
        BindingPattern::BindingIdentifier(id) => {
            out.push(leaf(id.name.as_str(), symbol_kind, id.span, id.span, li));
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                push_pattern_leaves(&property.value, symbol_kind, li, out);
            }
            if let Some(rest) = &object.rest {
                push_pattern_leaves(&rest.argument, symbol_kind, li, out);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                push_pattern_leaves(element, symbol_kind, li, out);
            }
            if let Some(rest) = &array.rest {
                push_pattern_leaves(&rest.argument, symbol_kind, li, out);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            push_pattern_leaves(&assignment.left, symbol_kind, li, out);
        }
    }
}

/// The value a binding holds, through parentheses and type-only wrappers
/// (`as`, `satisfies`, `!`, `<T>x`), which change no runtime value.
fn bound_value<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression.without_parentheses() {
        Expression::TSAsExpression(inner) => bound_value(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => bound_value(&inner.expression),
        Expression::TSNonNullExpression(inner) => bound_value(&inner.expression),
        Expression::TSTypeAssertion(inner) => bound_value(&inner.expression),
        other => other,
    }
}

pub(super) fn collect_variable(
    decl: &VariableDeclaration,
    li: &SpanPositions,
    out: &mut Vec<DocumentSymbol>,
) {
    let is_const = matches!(
        decl.kind,
        VariableDeclarationKind::Const
            | VariableDeclarationKind::Using
            | VariableDeclarationKind::AwaitUsing
    );
    for declarator in &decl.declarations {
        match &declarator.id {
            BindingPattern::BindingIdentifier(id) => {
                let symbol_kind = match declarator.init.as_ref().map(bound_value) {
                    Some(Expression::ArrowFunctionExpression(_))
                    | Some(Expression::FunctionExpression(_)) => symbol_kind::FUNCTION,
                    Some(Expression::ClassExpression(_)) => symbol_kind::CLASS,
                    _ if is_const => symbol_kind::CONSTANT,
                    _ => symbol_kind::VARIABLE,
                };
                let nested = declarator
                    .init
                    .as_ref()
                    .and_then(|init| function_value_children(init, li))
                    .unwrap_or_default();
                out.push(container(
                    id.name.as_str(),
                    symbol_kind,
                    declarator.span,
                    id.span,
                    nested,
                    li,
                ));
            }
            // Destructuring binds several names; emit each as its own symbol
            // instead of dropping the whole declarator.
            pattern => {
                let symbol_kind = if is_const {
                    symbol_kind::CONSTANT
                } else {
                    symbol_kind::VARIABLE
                };
                push_pattern_leaves(pattern, symbol_kind, li, out);
            }
        }
    }
}

fn enum_member_name(name: &TSEnumMemberName) -> Option<(String, Span)> {
    match name {
        TSEnumMemberName::Identifier(id) => Some((id.name.as_str().to_string(), id.span)),
        TSEnumMemberName::String(s) => Some((s.value.as_str().to_string(), s.span)),
        _ => None, // computed members
    }
}

fn leaf(name: &str, kind: u8, full: Span, selection: Span, li: &SpanPositions) -> DocumentSymbol {
    container(name, kind, full, selection, Vec::new(), li)
}

pub(super) fn container(
    name: &str,
    kind: u8,
    full: Span,
    selection: Span,
    children: Vec<DocumentSymbol>,
    li: &SpanPositions,
) -> DocumentSymbol {
    DocumentSymbol {
        name: name.to_string(),
        kind,
        range: li.range(full),
        selection_range: li.range(selection),
        children,
    }
}
