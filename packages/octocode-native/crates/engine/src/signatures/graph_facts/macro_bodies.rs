//! Rust macro bodies re-read after the main walk: item-level bodies as
//! items (`cfg_rt! { mod x; }`), and the arguments of admitted expression
//! macros (`ready!(poll_proceed(cx))`, see [`super::rust_macro_args`]) as
//! expressions.

use std::collections::HashSet;

use super::GraphAccumulator;
use super::positions::NodePositions;
use super::rust_macro_args::{MacroArgs, foreign_use, shadowed};
use super::walk::{push_macro_gap, visit_node};
use crate::graph::GraphParseAssessment;
use tree_sitter::Node;

/// A Rust macro body to re-read: its range, the enclosing `mod` names
/// (outermost first), and the enclosing declaration's `(id, name)`, so what
/// it holds nests where the macro call sits.
#[derive(Clone)]
pub(super) struct MacroBody {
    pub(super) range: tree_sitter::Range,
    pub(super) scope: Vec<String>,
    pub(super) container: Option<(String, String)>,
    pub(super) kind: MacroBodyKind,
}

#[derive(Clone)]
pub(super) enum MacroBodyKind {
    /// An item-level body, without its delimiters, read as items.
    Items,
    /// An admitted expression macro's token tree, with its delimiters (so
    /// `(a, b)` reads as a tuple and `[x; 3]` as an array), read as
    /// expressions. `chain` names the macros that hold it, outermost first,
    /// this one last. `recovered` is the assessment of a macro call in a
    /// recovered declaration, which its calls keep.
    Expression {
        chain: Vec<String>,
        args: MacroArgs,
        recovered: Option<GraphParseAssessment>,
    },
}

/// Read recorded Rust macro bodies (nested bodies too), in the order the walk
/// met them, each under the declaration that holds the macro call. Item
/// bodies come first, so their units number as in a declarations-only read,
/// which reads no expression body (an expression body never queues an item
/// body). An item body that does not parse as items stays an explicit gap;
/// an expression body that does not parse cleanly as a whole yields nothing,
/// since a body with an error node is no proof its call nodes are calls.
/// Declarations found in bodies are then put back in source order (a stable
/// sort by start byte keeps parents before their members).
pub(super) fn visit_macro_bodies(
    content: &str,
    language: &tree_sitter::Language,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
) -> bool {
    const MAX_ITEM_MACRO_BODIES: usize = 256;
    let declared_before = acc.declarations.len();
    // `(caller id, line, callee)` of each call read from an expression body:
    // the outer walk recorded its callee path as flat tokens.
    let mut macro_calls = HashSet::<(Option<String>, u32, String)>::new();
    for items in [true, false] {
        let mut visited = 0;
        let mut items_visited = 0;
        while visited < acc.macro_bodies.len() {
            visited += 1;
            if matches!(acc.macro_bodies[visited - 1].kind, MacroBodyKind::Items) != items {
                continue;
            }
            let body = acc.macro_bodies[visited - 1].clone();
            if items {
                items_visited += 1;
                if items_visited > MAX_ITEM_MACRO_BODIES {
                    push_macro_gap(acc);
                    continue;
                }
            }
            let Some(calls) = visit_body(content, language, line_index, acc, deadline, &body)
            else {
                return false;
            };
            macro_calls.extend(calls);
        }
    }
    if !macro_calls.is_empty() {
        // A called path is a call fact, never also a use (as in plain code).
        acc.paths.retain(|path| {
            !macro_calls.contains(&(path.caller_id.clone(), path.line, path.path.clone()))
        });
    }
    if acc.declarations.len() > declared_before {
        let mut order: Vec<usize> = (0..acc.declarations.len()).collect();
        order.sort_by_key(|&index| acc.declaration_spans[index].0);
        let mut declarations = std::mem::take(&mut acc.declarations)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        acc.declarations = order
            .iter()
            .filter_map(|&index| declarations[index].take())
            .collect();
        acc.declaration_spans = order
            .iter()
            .map(|&index| acc.declaration_spans[index])
            .collect();
    }
    true
}

/// Read one body in its own source unit; `None` when the deadline passed.
/// Returns `(caller id, line, callee)` of each call an expression body held.
fn visit_body(
    content: &str,
    language: &tree_sitter::Language,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
    body: &MacroBody,
) -> Option<Vec<(Option<String>, u32, String)>> {
    let (producer, chain, recovered) = match &body.kind {
        MacroBodyKind::Items => ("rust-item-macro-body", None, None),
        MacroBodyKind::Expression {
            chain, recovered, ..
        } => {
            let declared = acc
                .declarations
                .iter()
                .filter(|declaration| declaration.kind == "macro")
                .map(|declaration| declaration.name.as_str());
            let imported = acc
                .imports
                .iter()
                .filter(|import| import.import_kind == "value" && foreign_use(&import.specifier))
                .filter_map(|import| import.local_name.as_deref());
            if shadowed(chain, declared.chain(imported)) {
                return Some(Vec::new());
            }
            (
                "rust-expression-macro-body",
                Some(chain.clone()),
                recovered.clone(),
            )
        }
    };
    let parsed = if chain.is_none() {
        crate::signatures::extractor::parse_ranges_before(
            content,
            language,
            &[body.range],
            deadline,
        )
    } else {
        // A delimited group is an expression statement only with its `;`,
        // which the parser reads after the body.
        let mut included = body.range;
        included.end_byte += 1;
        included.end_point.column += 1;
        crate::signatures::extractor::parse_ranges_ending(
            &content[..body.range.end_byte],
            ";",
            language,
            &[included],
            deadline,
        )
    };
    let Some(tree) = parsed.filter(|tree| !tree.root_node().has_error()) else {
        if std::time::Instant::now() >= deadline {
            return None;
        }
        if chain.is_none() {
            push_macro_gap(acc);
        }
        return Some(Vec::new());
    };
    let starts = (
        acc.declarations.len(),
        acc.calls.len(),
        acc.argument_sites.len(),
    );
    let operation = acc.source_templates.units[0].operation;
    let unit = acc
        .source_templates
        .add_unit(
            crate::text::analysis::Producer::new(producer, "1"),
            operation,
            crate::text::analysis::RepresentationHandle(0),
            vec![crate::text::analysis::ByteRange::new(
                crate::num::saturating_u32(body.range.start_byte),
                crate::num::saturating_u32(body.range.end_byte),
            )],
        )
        .ok()?;
    let root = match &body.kind {
        MacroBodyKind::Items => Some(tree.root_node()),
        MacroBodyKind::Expression { args, .. } => arguments(tree.root_node(), *args),
    };
    // An expression body keeps the file's bindings: they hold the enclosing
    // function's locals, and the macro call already makes its block an
    // unknown scope. An item body binds its own items.
    let outer_bindings = chain
        .is_none()
        .then(|| acc.argument_bindings.take())
        .flatten();
    if chain.is_none()
        && let Some(adapter) = acc.argument_adapter
    {
        acc.argument_bindings = Some(
            crate::signatures::arguments::binding::Bindings::collect(
                tree.root_node(),
                adapter,
                content,
                deadline,
            )
            .in_region(unit),
        );
    }
    let outer_chain = std::mem::replace(&mut acc.macro_chain, chain);
    let outer_recovered = std::mem::replace(&mut acc.macro_recovered, recovered.clone());
    let completed = root.is_none_or(|root| {
        visit_node(
            root,
            content,
            line_index,
            acc,
            deadline,
            &body.scope,
            body.container.as_ref(),
        )
    });
    let chain = std::mem::replace(&mut acc.macro_chain, outer_chain);
    acc.macro_recovered = outer_recovered;
    if chain.is_none() {
        acc.argument_bindings = outer_bindings;
    }
    if !completed {
        return None;
    }
    for declaration in &mut acc.declarations[starts.0..] {
        if let Some(span) = &mut declaration.source_range {
            span.unit = unit;
        }
        if let Some(span) = &mut declaration.selection_byte_range {
            span.unit = unit;
        }
    }
    let mut macro_calls = Vec::new();
    for call in &mut acc.calls[starts.1..] {
        if let Some(span) = &mut call.source_range {
            span.unit = unit;
        }
        if chain.is_some() {
            macro_calls.push((call.caller_id.clone(), call.line, call.callee.clone()));
        }
        if let Some(recovered) = &recovered {
            call.parse_assessment = recovered.clone();
        }
    }
    for site in &mut acc.argument_sites[starts.2..] {
        site.set_unit(unit);
        if let Some(recovered) = &recovered {
            site.parse_assessment = recovered.clone();
        }
    }
    Some(macro_calls)
}

/// The node an expression body's calls are read from: the delimited group
/// itself (`(a, b)`, `[x; 3]`, `{ … }`, under `source_file >
/// expression_statement`), or for a `matches!`-style body only its first
/// element (the whole group when it holds one expression).
fn arguments(root: Node<'_>, args: MacroArgs) -> Option<Node<'_>> {
    let group = root.named_child(0)?.named_child(0)?;
    match args {
        MacroArgs::Forwarding => Some(group),
        MacroArgs::Scrutinee if matches!(group.kind(), "tuple_expression" | "array_expression") => {
            group.named_child(0)
        }
        MacroArgs::Scrutinee => Some(group),
    }
}

/// Record a Rust macro call's body to re-read: an item-level call's body as
/// items (an explicit gap when it has none), or an admitted expression
/// macro's arguments. `expression` is that macro's path as written and how
/// it reads its arguments; `None` for an item-level call.
pub(super) fn record_macro_body(
    node: Node<'_>,
    acc: &mut GraphAccumulator,
    container: Option<(String, String)>,
    module_scope: &[String],
    expression: Option<(String, MacroArgs, Option<GraphParseAssessment>)>,
) {
    let mut cursor = node.walk();
    let body = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
        .filter(|body| body.end_byte() > body.start_byte() + 2);
    let Some(body) = body else {
        if expression.is_none() {
            // `make_imports!();` expands from its definition, which is not
            // visible here: the generated items stay an explicit gap.
            push_macro_gap(acc);
        }
        return;
    };
    let (range, kind) = match expression {
        Some((path, args, recovered)) => {
            let mut chain = acc.macro_chain.clone().unwrap_or_default();
            chain.push(path);
            let kind = MacroBodyKind::Expression {
                chain,
                args,
                recovered,
            };
            (body.range(), kind)
        }
        None => (
            tree_sitter::Range {
                start_byte: body.start_byte() + 1,
                end_byte: body.end_byte() - 1,
                start_point: tree_sitter::Point {
                    row: body.start_position().row,
                    column: body.start_position().column + 1,
                },
                end_point: tree_sitter::Point {
                    row: body.end_position().row,
                    column: body.end_position().column.saturating_sub(1),
                },
            },
            MacroBodyKind::Items,
        ),
    };
    acc.macro_bodies.push(MacroBody {
        range,
        scope: module_scope.to_vec(),
        container,
        kind,
    });
}
