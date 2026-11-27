//! Names a `require` binds. A module-scope binding links calls by name across
//! the whole file, so it is admitted only when that name provably holds the
//! loaded module (or one property of it) everywhere. A binding in a function
//! or block holds only inside that scope, and carries the scope's extent.
//! A guarded load (`E || require(x)`, `E ?? require(x)`) binds the module
//! only on the branch where the guard fails: it carries the extent of the
//! scope that holds it even at module scope, so its links stay at most
//! medium, as a lazy load's do. A later write of the guard's own source text
//! (`_ = root._`, compared as text, never by name) keeps the binding: it
//! assigns the guard's value, one of the two values the declaration already
//! admits. That value need not be the loaded module at that point (after
//! `_.noConflict()` restores an earlier global it is not), which is why the
//! links stay medium.

use std::collections::HashMap;

use oxc_ast::{AstKind, ast::*};
use oxc_semantic::{AstNode, ScopeId, Scoping, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use super::inner;
use crate::graph::{GraphFactCommonJs, GraphFactCommonJsBinding, GraphFactDeclaration};
use crate::signatures::js_oxc_calls::receiver_label;
use crate::signatures::js_oxc_shared::SpanPositions;
use crate::text::analysis::ObservedSpan;

/// Attach each proven binding to the admitted load (by call span) it names.
/// Admitted: a declarator whose whole initializer is `require(x)` or
/// `require(x).k`, or such a load as the fallback of `||` or `??`, bound to an identifier or to a static object pattern of
/// identifiers (no defaults, no rest), never written. At module scope the
/// declaration is a Program statement, each name is declared once in the
/// file and the outline lists it. In a function or block the declaration
/// sits directly in the scope that holds it and no scope inside declares the
/// name again. An inline `require(x).k()` binds its receiver for that call.
pub(super) fn attach(
    semantic: &Semantic<'_>,
    li: &SpanPositions,
    declarations: &[GraphFactDeclaration],
    admitted: &HashMap<Span, usize>,
    loads: &mut [GraphFactCommonJs],
) {
    if admitted.is_empty() {
        return;
    }
    let scoping = semantic.scoping();
    let mut by_name = HashMap::<&str, Vec<SymbolId>>::new();
    for symbol in scoping.symbol_ids() {
        by_name
            .entry(scoping.symbol_name(symbol))
            .or_default()
            .push(symbol);
    }
    let proof = Proof {
        semantic,
        scoping,
        by_name,
        li,
        declarations,
    };
    for node in semantic.nodes().iter() {
        let found = match node.kind() {
            AstKind::VariableDeclarator(declarator) => proof.declarator(node, declarator),
            AstKind::CallExpression(call) => inline(call).into_iter().collect(),
            _ => continue,
        };
        for (load_call, binding) in found {
            if let Some(&load) = admitted.get(&load_call) {
                loads[load].bindings.push(binding);
            }
        }
    }
}

struct Proof<'s, 'a> {
    semantic: &'s Semantic<'a>,
    scoping: &'s Scoping,
    by_name: HashMap<&'s str, Vec<SymbolId>>,
    li: &'s SpanPositions<'s>,
    declarations: &'s [GraphFactDeclaration],
}

/// Where a proven binding holds: its outline holder, if any, and its scope
/// extent (`None`: the whole file).
type Holding = (Option<String>, Option<ObservedSpan>);

impl Proof<'_, '_> {
    /// The bindings one declarator proves, each with its load call span.
    fn declarator(
        &self,
        node: &AstNode<'_>,
        declarator: &VariableDeclarator<'_>,
    ) -> Vec<(Span, GraphFactCommonJsBinding)> {
        let Some((call, property, guarded)) = declarator.init.as_ref().and_then(guarded_load)
        else {
            return Vec::new();
        };
        let Some(pattern) = pattern_bindings(&declarator.id, property) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (id, imported) in pattern {
            let Some(symbol) = id.symbol_id.get() else {
                continue;
            };
            if (self.scoping.symbol_is_mutated(symbol)
                && !guarded.is_some_and(|guard| self.rewrites_only(symbol, guard)))
                || !self.scoping.symbol_redeclarations(symbol).is_empty()
            {
                continue;
            }
            let held_in = self.scoping.symbol_scope_id(symbol);
            let holding = if held_in == self.scoping.root_scope_id() {
                self.module_binding(node, id).map(|(holder, scope)| {
                    if guarded.is_some() {
                        (holder, Some(self.extent(held_in)))
                    } else {
                        (holder, scope)
                    }
                })
            } else {
                self.scope_binding(node, symbol, held_in, id)
            };
            if let Some((declaration_id, scope)) = holding {
                found.push((
                    call,
                    GraphFactCommonJsBinding {
                        local: id.name.as_str().to_owned(),
                        imported,
                        declaration_id,
                        scope,
                    },
                ));
            }
        }
        found
    }

    /// A Program-statement binding declared once in the file, held by the
    /// top-level declaration the outline emits for it.
    fn module_binding(&self, node: &AstNode<'_>, id: &BindingIdentifier) -> Option<Holding> {
        let nodes = self.semantic.nodes();
        let statement = nodes.parent_node(node.id());
        let in_program = matches!(nodes.parent_kind(statement.id()), AstKind::Program(_));
        let once = self.by_name.get(id.name.as_str()).map_or(0, Vec::len) == 1;
        let holder = self.declaration_of(id, true)?;
        (in_program && once).then_some((Some(holder), None))
    }

    /// A binding declared directly in the scope that holds it, which no
    /// scope inside redeclares: it holds over that scope's extent.
    fn scope_binding(
        &self,
        node: &AstNode<'_>,
        symbol: SymbolId,
        held_in: ScopeId,
        id: &BindingIdentifier,
    ) -> Option<Holding> {
        if node.scope_id() != held_in || self.shadowed(symbol, held_in) {
            return None;
        }
        Some((self.declaration_of(id, false), Some(self.extent(held_in))))
    }

    /// Every write of `symbol` is `symbol = <the text at guard>`.
    fn rewrites_only(&self, symbol: SymbolId, guard: Span) -> bool {
        let source = self.semantic.source_text();
        let text = |span: Span| source.get(span.start as usize..span.end as usize);
        let nodes = self.semantic.nodes();
        self.scoping
            .get_resolved_references(symbol)
            .filter(|reference| reference.is_write())
            .all(|reference| {
                matches!(nodes.parent_kind(reference.node_id()),
                    AstKind::AssignmentExpression(assignment)
                        if assignment.operator == AssignmentOperator::Assign
                            && text(inner(&assignment.right).span()) == text(guard))
            })
    }

    /// The source extent of `scope`.
    fn extent(&self, scope: ScopeId) -> ObservedSpan {
        let span = self
            .semantic
            .nodes()
            .kind(self.scoping.get_node_id(scope))
            .span();
        ObservedSpan::root(span.start, span.end)
    }

    /// Another symbol of the same name is declared in `scope` or inside it.
    fn shadowed(&self, symbol: SymbolId, scope: ScopeId) -> bool {
        self.by_name
            .get(self.scoping.symbol_name(symbol))
            .into_iter()
            .flatten()
            .filter(|other| **other != symbol)
            .any(|other| {
                self.scoping
                    .scope_ancestors(self.scoping.symbol_scope_id(*other))
                    .any(|ancestor| ancestor == scope)
            })
    }

    /// The outline declaration of `id`, selected by its name and position
    /// (`top_level`: only a declaration without a parent).
    fn declaration_of(&self, id: &BindingIdentifier, top_level: bool) -> Option<String> {
        let start = self.li.range(id.span).start;
        self.declarations
            .iter()
            .find(|declaration| {
                (!top_level || declaration.parent.is_none())
                    && declaration.name == id.name.as_str()
                    && declaration.selection_range.start == start
            })
            .map(|declaration| declaration.id.clone())
    }
}

/// `require(x).k(..)`: the receiver as the call extractor spells it, bound
/// to the whole module over the callee `require(x).k` only, so a call in
/// the arguments is governed by its own receiver.
fn inline(call: &CallExpression<'_>) -> Option<(Span, GraphFactCommonJsBinding)> {
    let Expression::StaticMemberExpression(member) = inner(&call.callee) else {
        return None;
    };
    if member.optional || call.optional {
        return None;
    }
    let Expression::CallExpression(load) = inner(&member.object) else {
        return None;
    };
    Some((
        load.span,
        GraphFactCommonJsBinding {
            local: receiver_label(&member.object),
            imported: None,
            declaration_id: None,
            scope: Some(ObservedSpan::root(member.span.start, member.span.end)),
        },
    ))
}

/// [`load_of`] for `initializer`, or for the right side of `E || load` and
/// `E ?? load`, and the guard `E` that decides whether the load runs.
fn guarded_load<'a>(
    initializer: &'a Expression<'a>,
) -> Option<(Span, Option<&'a str>, Option<Span>)> {
    if let Expression::LogicalExpression(logical) = inner(initializer)
        && matches!(
            logical.operator,
            LogicalOperator::Or | LogicalOperator::Coalesce
        )
    {
        let (call, property) = load_of(&logical.right)?;
        return Some((call, property, Some(inner(&logical.left).span())));
    }
    let (call, property) = load_of(initializer)?;
    Some((call, property, None))
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
