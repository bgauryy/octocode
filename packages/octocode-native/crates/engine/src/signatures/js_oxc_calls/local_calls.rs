//! What oxc semantics bind a bare callee to: a declaration the outline lists
//! (that one, not a same-named one), or a parameter, a local variable or a
//! closure (that value, never a declaration of the same name).

use std::collections::{HashMap, HashSet};

use oxc_ast::{AstKind, ast::*};
use oxc_semantic::Semantic;
use oxc_span::Span;

use super::names::unwrap_expr;
use crate::graph::{GraphFactCall, GraphFactCommonJs, GraphFactDeclaration};

/// For each call, construction, tagged template or JSX render whose callee is
/// a bare name oxc semantics bind: `bound_declaration` when the symbol is a
/// declaration the outline lists; else `term_binding` when it lives in a
/// function or block scope and is no import and no CommonJS load the linker
/// follows. A call the receiver walk respelled (`const f = ns.g; f()` is
/// recorded as `ns.g`) keeps its spelling, and a file with `with` or a
/// direct `eval` is not assessed.
pub(in crate::signatures) fn mark_local_calls(
    semantic: &Semantic<'_>,
    declarations: &[GraphFactDeclaration],
    common_js: &[GraphFactCommonJs],
    calls: &mut [GraphFactCall],
) {
    if calls.is_empty() || crate::signatures::js_oxc_arguments::dynamic_scope(semantic) {
        return;
    }
    let declared = declarations
        .iter()
        .filter_map(|declaration| {
            declaration
                .selection_byte_range
                .as_ref()
                .map(|range| ((range.start_byte, range.end_byte), declaration.id.as_str()))
        })
        .collect::<HashMap<_, _>>();
    let scoping = semantic.scoping();
    let mut bound = HashMap::new();
    let mut local = HashSet::new();
    for node in semantic.nodes().iter() {
        let (span, callee) = match node.kind() {
            AstKind::CallExpression(call) => (call.span, identifier(&call.callee)),
            AstKind::NewExpression(call) => (call.span, identifier(&call.callee)),
            AstKind::TaggedTemplateExpression(call) => (call.span, identifier(&call.tag)),
            AstKind::JSXOpeningElement(element) => (
                element.span,
                match &element.name {
                    JSXElementName::IdentifierReference(name) => Some(&**name),
                    _ => None,
                },
            ),
            _ => continue,
        };
        let Some(callee) = callee else {
            continue;
        };
        let Some(symbol) = callee
            .reference_id
            .get()
            .and_then(|id| scoping.get_reference(id).symbol_id())
        else {
            continue;
        };
        let symbol_span = scoping.symbol_span(symbol);
        let name = callee.name.as_str();
        if let Some(id) = declared.get(&(symbol_span.start, symbol_span.end)) {
            bound.insert((span.start, span.end, name), *id);
        } else if scoping.symbol_scope_id(symbol) != scoping.root_scope_id()
            && !scoping.symbol_flags(symbol).is_import()
            && !loads_module(common_js, name, span)
        {
            local.insert((span.start, span.end, name));
        }
    }
    for call in calls {
        let Some(range) = &call.source_range else {
            continue;
        };
        let key = (range.start_byte, range.end_byte, call.callee.as_str());
        call.term_binding |= local.contains(&key);
        if let Some(id) = bound.get(&key) {
            call.bound_declaration = Some((*id).to_owned());
        }
    }
}

fn identifier<'a>(expression: &'a Expression<'a>) -> Option<&'a IdentifierReference<'a>> {
    match unwrap_expr(expression) {
        Expression::Identifier(identifier) => Some(&**identifier),
        _ => None,
    }
}

/// Whether a CommonJS load binds `name` where `span` reads it: the linker
/// follows that binding to the loaded module.
fn loads_module(common_js: &[GraphFactCommonJs], name: &str, span: Span) -> bool {
    common_js
        .iter()
        .flat_map(|load| &load.bindings)
        .filter(|binding| binding.local == name)
        .any(|binding| {
            binding
                .scope
                .as_ref()
                .is_none_or(|scope| scope.start_byte <= span.start && span.end <= scope.end_byte)
        })
}
