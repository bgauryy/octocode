//! Module-scope names a `require` binds. The graph links calls to these by
//! name across the whole file, so a binding is admitted only when that name
//! provably holds the loaded module (or one property of it) everywhere.

use std::collections::HashMap;

use oxc_ast::ast::*;
use oxc_semantic::Semantic;
use oxc_span::Span;

use super::inner;
use crate::graph::{GraphFactCommonJs, GraphFactCommonJsBinding, GraphFactDeclaration};
use crate::signatures::js_oxc_shared::SpanPositions;

/// Attach each proven binding to the admitted load (by call span) it names.
/// Admitted: a Program-scope declarator whose whole initializer is
/// `require(x)` or `require(x).k`, bound to an identifier or to a static
/// object pattern of identifiers (no defaults, no rest), where each name is
/// declared once in the file and never written.
pub(super) fn attach(
    semantic: &Semantic<'_>,
    program: &Program,
    li: &SpanPositions,
    declarations: &[GraphFactDeclaration],
    admitted: &HashMap<Span, usize>,
    loads: &mut [GraphFactCommonJs],
) {
    if admitted.is_empty() {
        return;
    }
    let scoping = semantic.scoping();
    let mut declared = HashMap::<&str, usize>::new();
    for name in scoping.symbol_names() {
        *declared.entry(name).or_default() += 1;
    }
    let proven = |id: &BindingIdentifier| {
        id.symbol_id.get().is_some_and(|symbol| {
            !scoping.symbol_is_mutated(symbol)
                && scoping.symbol_redeclarations(symbol).is_empty()
                && declared.get(id.name.as_str()) == Some(&1)
        })
    };
    // The outline emits one top-level declaration per bound identifier,
    // selected by its name.
    let declaration_of = |id: &BindingIdentifier| {
        let start = li.range(id.span).start;
        declarations
            .iter()
            .find(|declaration| {
                declaration.parent.is_none()
                    && declaration.name == id.name.as_str()
                    && declaration.selection_range.start == start
            })
            .map(|declaration| declaration.id.clone())
    };
    for statement in &program.body {
        let Statement::VariableDeclaration(declaration) = statement else {
            continue;
        };
        for declarator in &declaration.declarations {
            let Some((call, property)) = declarator.init.as_ref().and_then(load_of) else {
                continue;
            };
            let Some(&load) = admitted.get(&call) else {
                continue;
            };
            let Some(pattern) = pattern_bindings(&declarator.id, property) else {
                continue;
            };
            for (id, imported) in pattern {
                if !proven(id) {
                    continue;
                }
                let Some(declaration_id) = declaration_of(id) else {
                    continue;
                };
                loads[load].bindings.push(GraphFactCommonJsBinding {
                    local: id.name.as_str().to_owned(),
                    imported,
                    declaration_id,
                });
            }
        }
    }
}

/// The load call span of `require(x)` or `require(x).k`, and `k`.
fn load_of<'a>(initializer: &'a Expression<'a>) -> Option<(Span, Option<&'a str>)> {
    match inner(initializer) {
        Expression::CallExpression(call) => Some((call.span, None)),
        Expression::StaticMemberExpression(member) if !member.optional => {
            match inner(&member.object) {
                Expression::CallExpression(call) => {
                    Some((call.span, Some(member.property.name.as_str())))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The identifiers a declarator pattern binds and the property each holds
/// (`None`: the whole module); `None` when any part of it is not a plain
/// static binding.
fn pattern_bindings<'p>(
    pattern: &'p BindingPattern<'p>,
    property: Option<&str>,
) -> Option<Vec<(&'p BindingIdentifier<'p>, Option<String>)>> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(vec![(&**id, property.map(str::to_owned))]),
        BindingPattern::ObjectPattern(object) if property.is_none() && object.rest.is_none() => {
            object
                .properties
                .iter()
                .map(|item| {
                    let key = (!item.computed).then(|| item.key.static_name()).flatten()?;
                    match &item.value {
                        BindingPattern::BindingIdentifier(id) => {
                            Some((&**id, Some(key.into_owned())))
                        }
                        _ => None,
                    }
                })
                .collect()
        }
        _ => None,
    }
}
