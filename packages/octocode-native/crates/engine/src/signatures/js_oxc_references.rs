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

use crate::types::GraphReferenceCount;

use super::js_oxc_shared::LineIndex;

/// One declaration to count: its id, display name and name-token start.
pub(super) struct CountTarget<'d> {
    pub(super) id: &'d str,
    pub(super) name: &'d str,
    pub(super) line: u32,
    pub(super) character: u32,
}

pub(super) fn value_reference_counts(
    semantic: &Semantic<'_>,
    line_index: &LineIndex<'_>,
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
                Some(&symbol) => scoping
                    .get_resolved_references(symbol)
                    .filter(|reference| reference.is_read() || reference.is_type())
                    .filter(|reference| is_value_use(nodes, reference.node_id()))
                    .count() as u32,
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

/// Fill `used_in` of every named and default import binding: for each
/// resolved reference to the binding's symbol, the id of the innermost
/// declaration whose range contains it, or `IMPORT_USE_MODULE` outside every
/// declaration (module-level code and export clauses). All reference kinds
/// count (calls, values, types, JSX). Namespace imports, `import x =`
/// bindings and bindings without a resolvable symbol keep `used_in` absent.
pub(super) fn record_import_uses(
    semantic: &Semantic<'_>,
    line_index: &LineIndex<'_>,
    facts: &mut crate::graph::GraphFactsDocument,
) {
    let crate::graph::GraphFactsDocument {
        declarations,
        imports,
        ..
    } = facts;
    let mut targets = imports
        .iter_mut()
        .filter(|import| {
            import
                .imported_name
                .as_deref()
                .is_some_and(|name| name != "*")
        })
        .filter_map(|import| {
            let start = import.local_range.as_ref()?.start.clone();
            Some((line_index.byte_offset(start.line, start.character), import))
        })
        .peekable();
    if targets.peek().is_none() {
        return;
    }
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let symbols_by_start: HashMap<u32, _> = scoping
        .symbol_ids()
        .map(|symbol| (scoping.symbol_span(symbol).start, symbol))
        .collect();
    let spans = declarations
        .iter()
        .map(|declaration| {
            let range = &declaration.range;
            (
                line_index.byte_offset(range.start.line, range.start.character),
                line_index.byte_offset(range.end.line, range.end.character),
                declaration.id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    for (start, import) in targets {
        let Some(&symbol) = symbols_by_start.get(&start) else {
            continue;
        };
        let mut users = scoping
            .get_resolved_references(symbol)
            .map(|reference| {
                let at = nodes.kind(reference.node_id()).span().start;
                spans
                    .iter()
                    .filter(|(start, end, _)| *start <= at && at < *end)
                    .min_by_key(|(start, end, _)| end - start)
                    .map_or(crate::graph::IMPORT_USE_MODULE, |(_, _, id)| *id)
                    .to_owned()
            })
            .collect::<Vec<_>>();
        users.sort_unstable();
        users.dedup();
        import.used_in = Some(users);
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

/// Whether the expression at `node_id` (spanning `span`) is the callee of a
/// call, construction or tagged template, looking through parentheses and
/// TypeScript-only wrappers the way the call facts do.
fn is_call_target(nodes: &AstNodes<'_>, node_id: NodeId, mut span: Span) -> bool {
    for kind in nodes.ancestor_kinds(node_id) {
        match kind {
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSTypeAssertion(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSInstantiationExpression(_) => span = kind.span(),
            AstKind::CallExpression(call) => return call.callee.span() == span,
            AstKind::NewExpression(call) => return call.callee.span() == span,
            AstKind::TaggedTemplateExpression(call) => return call.tag.span() == span,
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    /// Reference count of the top-level declaration `name` in `source`.
    fn count(source: &str, name: &str) -> u32 {
        let extraction = super::super::js_oxc::extract_graph_facts_with_metadata(source, "mod.ts")
            .expect("graph facts");
        let id = &extraction
            .facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name && declaration.parent.is_none())
            .unwrap_or_else(|| panic!("declaration {name}"))
            .id;
        extraction
            .reference_counts
            .iter()
            .find(|count| &count.declaration_id == id)
            .expect("counted")
            .count
    }

    #[test]
    fn import_bindings_record_their_enclosing_declarations() {
        let facts = super::super::js_oxc::extract_graph_facts_with_metadata(
            "import { run, Shape } from './a'\nimport def from './b'\nimport * as ns from './c'\nimport { unused } from './d'\nexport function live(s: Shape) { return run() }\nfunction dead() { return [run, def, ns.x] }\nexport { def }\n",
            "app.ts",
        )
        .expect("graph facts")
        .facts;
        let users = |local: &str| {
            facts
                .imports
                .iter()
                .find(|import| import.local_name.as_deref() == Some(local))
                .expect("import")
                .used_in
                .as_ref()
                .map(|users| {
                    users
                        .iter()
                        .map(|id| id.split(['#', '@']).nth(1).unwrap_or(id).to_owned())
                        .collect::<Vec<_>>()
                })
        };
        assert_eq!(users("run"), Some(vec!["dead".into(), "live".into()]));
        assert_eq!(users("Shape"), Some(vec!["live".into()]));
        assert_eq!(users("def"), Some(vec!["dead".into(), "module".into()]));
        assert_eq!(users("unused"), Some(vec![]));
        assert_eq!(users("ns"), None, "namespace imports stay unknown");
    }

    #[test]
    fn comments_and_strings_are_not_references() {
        let source = "export function helper() {}\n// helper is documented here\n/* helper */\nconst label = 'helper';\nconst tpl = `helper ${label}`;\n";
        assert_eq!(count(source, "helper"), 0);
    }

    #[test]
    fn value_uses_count_but_calls_and_export_clauses_do_not() {
        let source = "function cb() {}\nfunction called() {}\nclass Built {}\nlet slot = 0;\nregister(cb);\nconst alias = cb;\ncalled(); (called)(); new Built();\nslot = 1;\nexport { cb, called as renamed };\nexport default slot;\n";
        assert_eq!(count(source, "cb"), 2, "argument + assignment");
        assert_eq!(count(source, "called"), 0, "call targets are edges");
        assert_eq!(count(source, "Built"), 0, "construction is an edge");
        assert_eq!(count(source, "slot"), 0, "a write and an export clause");
        assert_eq!(count(source, "alias"), 0);
    }

    #[test]
    fn a_shadowing_local_does_not_reference_the_export() {
        let source = "export function run() { return 1 }\nexport function other() { const run = () => 2; register(run); }\n";
        assert_eq!(count(source, "run"), 0);
        let escaped =
            "export function run() { return 1 }\nexport function other() { register(run); }\n";
        assert_eq!(count(escaped, "run"), 1);
    }

    #[test]
    fn type_positions_reference_type_declarations() {
        let source = "export interface Shape {}\nexport type Unused = number;\nexport function area(s: Shape) { return s }\n";
        assert_eq!(count(source, "Shape"), 1);
        assert_eq!(count(source, "Unused"), 0);
    }

    #[test]
    fn members_count_non_call_member_accesses() {
        let extraction = super::super::js_oxc::extract_graph_facts_with_metadata(
            "export class Svc { go() {} stop() {} tick() {}\n  start() { this.go(); setTimeout(this.stop); } }\n// this.tick\n",
            "svc.ts",
        )
        .expect("graph facts");
        let member = |name: &str| {
            let id = &extraction
                .facts
                .declarations
                .iter()
                .find(|declaration| declaration.name == name)
                .expect("member")
                .id;
            extraction
                .reference_counts
                .iter()
                .find(|count| &count.declaration_id == id)
                .expect("counted")
                .count
        };
        assert_eq!(member("go"), 0, "a method call is an edge");
        assert_eq!(member("stop"), 1, "a method passed as a value escapes");
        assert_eq!(member("tick"), 0, "a comment is not a reference");
    }
}
