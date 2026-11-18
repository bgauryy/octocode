//! Value-reference counts for JS/TS graph declarations, from oxc semantics.
//!
//! A declaration bound in a scope (function, class, variable, enum, …) counts
//! the resolved references of *its* symbol, so a shadowing local with the same
//! name never keeps it live. Unbound declarations (class, interface and object
//! members) count member accesses with their name (`x.name`, `x["name"]`).
//! Comments and string literals are not references and never count.
//!
//! Excluded positions: the declaration itself, export clauses
//! (`export { x }`, `export default x`, `export = x`), pure writes, and call
//! targets (`x()`, `new x()`, `` x`…` ``, `a.x()`), which the call facts
//! already model as edges.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use crate::graph::{GraphFactImport, GraphReferenceCount};

use super::js_oxc_shared::SpanPositions;

/// One declaration to count: its id, display name and name-token start.
pub(super) struct CountTarget<'d> {
    pub(super) id: &'d str,
    pub(super) name: &'d str,
    pub(super) line: u32,
    pub(super) character: u32,
}

pub(super) fn value_reference_counts(
    semantic: &Semantic<'_>,
    line_index: &SpanPositions<'_>,
    targets: &[CountTarget<'_>],
) -> Vec<GraphReferenceCount> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let symbols_by_start: HashMap<u32, _> = scoping
        .symbol_ids()
        .map(|symbol| (scoping.symbol_span(symbol).start, symbol))
        .collect();
    let mut member_counts: Option<HashMap<String, u32>> = None;
    targets
        .iter()
        .map(|target| {
            let start = line_index.byte_offset(target.line, target.character);
            let count = match symbols_by_start.get(&start) {
                Some(&symbol) => crate::num::saturating_u32(
                    scoping
                        .get_resolved_references(symbol)
                        .filter(|reference| reference.is_read() || reference.is_type())
                        .filter(|reference| is_value_use(nodes, reference.node_id()))
                        .count(),
                ),
                None => member_counts
                    .get_or_insert_with(|| count_member_accesses(nodes))
                    .get(target.name)
                    .copied()
                    .unwrap_or(0),
            };
            GraphReferenceCount {
                declaration_id: target.id.to_owned(),
                count,
            }
        })
        .collect()
}

/// Marks each value import binding of a TypeScript file that no reference
/// reads or writes as a value ([`GraphFactImport::type_use_only`]). oxc
/// resolves every reference of an import binding and leaves a type position
/// (an annotation, `implements`, a `typeof` query) with the type flag only;
/// an expression, a JSX tag and an export clause read the binding as a value.
/// JavaScript emit keeps every import, so only TypeScript sources are marked.
pub(super) fn mark_type_use_only(
    semantic: &Semantic<'_>,
    line_index: &SpanPositions<'_>,
    imports: &mut [GraphFactImport],
) {
    if !semantic.source_type().is_typescript() {
        return;
    }
    let scoping = semantic.scoping();
    let mut symbols_by_start: Option<HashMap<u32, _>> = None;
    for import in imports
        .iter_mut()
        .filter(|import| import.import_kind == "value")
    {
        let Some(range) = &import.local_range else {
            continue;
        };
        let start = line_index.byte_offset(range.start.line, range.start.character);
        let symbols = symbols_by_start.get_or_insert_with(|| {
            scoping
                .symbol_ids()
                .map(|symbol| (scoping.symbol_span(symbol).start, symbol))
                .collect()
        });
        if let Some(&symbol) = symbols.get(&start) {
            import.type_use_only = !scoping
                .get_resolved_references(symbol)
                .any(|reference| reference.flags().is_value());
        }
    }
}

/// A resolved identifier reference that is neither an export clause nor a
/// call target.
fn is_value_use(nodes: &AstNodes<'_>, node_id: NodeId) -> bool {
    if matches!(
        nodes.parent_kind(node_id),
        AstKind::ExportSpecifier(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::TSExportAssignment(_)
    ) {
        return false;
    }
    !is_call_target(nodes, node_id, nodes.kind(node_id).span())
}

/// Member accesses per property name, excluding call targets.
fn count_member_accesses(nodes: &AstNodes<'_>) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for node in nodes.iter() {
        let name = match node.kind() {
            AstKind::StaticMemberExpression(member) => member.property.name.to_string(),
            AstKind::PrivateFieldExpression(member) => format!("#{}", member.field.name),
            AstKind::ComputedMemberExpression(member) => match &member.expression {
                oxc_ast::ast::Expression::StringLiteral(literal) => literal.value.to_string(),
                _ => continue,
            },
            _ => continue,
        };
        if !is_call_target(nodes, node.id(), node.kind().span()) {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts
}

/// What an expression is the target of, through parentheses and
/// TypeScript-only wrappers: TypeScript's call-hierarchy targets
/// (`isCallOrNewExpressionTarget`, `isTaggedTemplateTag`,
/// `isDecoratorTarget`, `isJsxOpeningLikeElementTagName`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Invocation {
    /// The callee of a call or a construction (`x()`, `x?.()`, `new x()`).
    Call,
    /// The tag of a tagged template (`` x`…` ``).
    Tag,
    /// A decorator's expression (`@x`).
    Decorator,
    /// The tag name of a JSX opening or self-closing element (`<X />`).
    JsxTag,
}

/// What the expression at `node_id` (spanning `span`) is invoked as, looking
/// through parentheses and TypeScript-only wrappers the way the call facts do.
fn invocation(nodes: &AstNodes<'_>, node_id: NodeId, mut span: Span) -> Option<Invocation> {
    for kind in nodes.ancestor_kinds(node_id) {
        let (target, invocation) = match kind {
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSTypeAssertion(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSInstantiationExpression(_) => {
                span = kind.span();
                continue;
            }
            AstKind::CallExpression(call) => (call.callee.span(), Invocation::Call),
            AstKind::NewExpression(call) => (call.callee.span(), Invocation::Call),
            AstKind::TaggedTemplateExpression(call) => (call.tag.span(), Invocation::Tag),
            AstKind::Decorator(decorator) => (decorator.expression.span(), Invocation::Decorator),
            AstKind::JSXOpeningElement(element) => (element.name.span(), Invocation::JsxTag),
            _ => return None,
        };
        return (target == span).then_some(invocation);
    }
    None
}

/// Whether the expression at `node_id` (spanning `span`) is the callee of a
/// call, construction or tagged template: the targets the call facts model
/// as edges.
fn is_call_target(nodes: &AstNodes<'_>, node_id: NodeId, span: Span) -> bool {
    matches!(
        invocation(nodes, node_id, span),
        Some(Invocation::Call | Invocation::Tag)
    )
}

/// Whether the name at `node_id` is a call site by TypeScript's
/// call-hierarchy rule (`convertEntryToCallSite`, services/callHierarchy.ts
/// v5.9.3): any [`Invocation`] target, the name of a property access
/// (`a.x`, `a.#x`, `<a.X />`, passed as a value too), or the argument of an
/// element access (`a["x"]`). A declaration's own name is none of them.
pub(super) fn is_call_site(nodes: &AstNodes<'_>, node_id: NodeId) -> bool {
    let span = nodes.kind(node_id).span();
    match nodes.parent_kind(node_id) {
        AstKind::StaticMemberExpression(member) if member.property.span == span => true,
        AstKind::PrivateFieldExpression(member) if member.field.span == span => true,
        AstKind::JSXMemberExpression(member) if member.property.span == span => true,
        AstKind::ComputedMemberExpression(member) if member.expression.span() == span => true,
        _ => invocation(nodes, node_id, span).is_some(),
    }
}

#[cfg(test)]
mod tests;
