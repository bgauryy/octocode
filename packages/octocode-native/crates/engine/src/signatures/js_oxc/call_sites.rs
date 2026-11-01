//! Reference sites of one JS/TS file, classified from a single oxc parse:
//! whether each is a call site by TypeScript's call-hierarchy rule (see
//! [`is_call_site`]), and the callable of the file's outline enclosing it.
//! Language-server references name positions; this answers what the server's
//! own call hierarchy would make of each one, without a `documentSymbol`
//! request.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_semantic::{NodeId, SemanticBuilder};
use oxc_span::GetSpan;

use crate::graph::{GraphPosition, GraphRange};
use crate::signatures::js_oxc_references::is_call_site;
use crate::signatures::js_oxc_shared::SpanPositions;
use crate::symbol_kind;

use super::DocumentSymbol;
use super::walk::{collect_member_functions, collect_program};

/// The callable of the outline that holds a site: LSP `DocumentSymbol`
/// fields, 0-based UTF-16 positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnclosingCallable {
    pub name: String,
    /// LSP `SymbolKind`: method, constructor or function.
    pub kind: u8,
    pub range: GraphRange,
    pub selection_range: GraphRange,
}

/// One classified reference start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceSite {
    /// Whether the name there is a call site; `None` when the position starts
    /// no name of the parse (a comment, a string, a recovered region).
    pub is_call: Option<bool>,
    /// Whether the name there binds or re-exports the symbol from another
    /// module (`import`, `export … from`, `import x = require`); `None` as
    /// for [`Self::is_call`].
    pub is_import: Option<bool>,
    /// The file's own binding the name there resolves to (a declaration,
    /// parameter or import of the file, by its scope analysis), as an id
    /// equal across the sites of one parse: sites with one binding name one
    /// thing. `None` for a property name, a shorthand property (it names a
    /// binding and a property at once), a global, or an undecided site.
    pub binding: Option<u32>,
    /// The innermost method, constructor or function holding the site;
    /// `None` at module top level.
    pub caller: Option<EnclosingCallable>,
}

/// The kinds of outline symbols that own call sites.
const CALLABLE_KINDS: [u8; 3] = [
    symbol_kind::METHOD,
    symbol_kind::CONSTRUCTOR,
    symbol_kind::FUNCTION,
];

/// Each of `starts` (0-based line, UTF-16 character of a name's first unit)
/// classified, in order; `None` when `content` is not JS/TS oxc can parse.
#[must_use]
pub fn reference_sites(
    content: &str,
    file_path: &str,
    starts: &[GraphPosition],
) -> Option<Vec<ReferenceSite>> {
    let starts = starts.to_vec();
    super::on_oxc_worker(content, file_path, move |content, file_path| {
        classify(content, file_path, &starts)
    })
}

fn classify(
    content: &str,
    file_path: &str,
    starts: &[GraphPosition],
) -> Option<Vec<ReferenceSite>> {
    super::with_js_program::<false, _>(content, file_path, |_, parser_ret| {
        let positions = SpanPositions::new(content);
        let mut outline = Vec::new();
        collect_program(&parser_ret.program, &positions, &mut outline);
        collect_member_functions(&parser_ret.program.body, &positions, &mut outline);
        let semantic = SemanticBuilder::new()
            .with_build_nodes(true)
            .build(&parser_ret.program)
            .semantic;
        let nodes = semantic.nodes();
        let offsets = starts
            .iter()
            .map(|start| positions.byte_offset(start.line, start.character))
            .collect::<Vec<_>>();
        let wanted = offsets
            .iter()
            .map(|offset| (*offset, Vec::new()))
            .collect::<HashMap<u32, Vec<NodeId>>>();
        let names = names_at(nodes, wanted);
        Some(
            starts
                .iter()
                .zip(&offsets)
                .map(|(start, offset)| ReferenceSite {
                    is_call: names
                        .get(offset)
                        .filter(|ids| !ids.is_empty())
                        .map(|ids| ids.iter().any(|id| is_call_site(nodes, *id))),
                    is_import: names
                        .get(offset)
                        .filter(|ids| !ids.is_empty())
                        .map(|ids| ids.iter().any(|id| is_import_site(nodes, *id))),
                    binding: names
                        .get(offset)
                        .and_then(|ids| one_binding(&semantic, ids)),
                    caller: enclosing(&outline, start),
                })
                .collect(),
        )
    })
}

/// The name nodes starting at each `wanted` byte offset. A private name
/// (`#x`) and a string literal (`a["x"]`) also start at their first inner
/// unit, where a reference to the bare name points.
fn names_at(
    nodes: &oxc_semantic::AstNodes<'_>,
    mut wanted: HashMap<u32, Vec<NodeId>>,
) -> HashMap<u32, Vec<NodeId>> {
    for node in nodes.iter() {
        let span = node.kind().span();
        let starts: &[u32] = match node.kind() {
            AstKind::IdentifierReference(_)
            | AstKind::IdentifierName(_)
            | AstKind::BindingIdentifier(_)
            | AstKind::LabelIdentifier(_)
            | AstKind::JSXIdentifier(_) => &[span.start],
            AstKind::PrivateIdentifier(_) | AstKind::StringLiteral(_) => {
                &[span.start, span.start + 1]
            }
            _ => continue,
        };
        for start in starts {
            if let Some(ids) = wanted.get_mut(start) {
                ids.push(node.id());
            }
        }
    }
    wanted
}

/// The one binding the names at a site resolve to (see
/// [`ReferenceSite::binding`]); `None` when they resolve to none or to more
/// than one.
fn one_binding(semantic: &oxc_semantic::Semantic<'_>, ids: &[NodeId]) -> Option<u32> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let mut found = None;
    for id in ids {
        let symbol = match nodes.kind(*id) {
            AstKind::IdentifierReference(reference) => {
                if matches!(nodes.parent_kind(*id), AstKind::ObjectProperty(property) if property.shorthand)
                {
                    return None;
                }
                reference
                    .reference_id
                    .get()
                    .and_then(|reference| scoping.get_reference(reference).symbol_id())
            }
            AstKind::BindingIdentifier(binding) => binding.symbol_id.get(),
            _ => None,
        };
        let Some(symbol) = symbol else { continue };
        let symbol = u32::try_from(symbol.index()).ok()?;
        if found.is_some_and(|known| known != symbol) {
            return None;
        }
        found = Some(symbol);
    }
    found
}

/// A name inside a declaration that brings a binding from another module:
/// an import, a re-export with a source, or `import x = require(…)`.
fn is_import_site(nodes: &oxc_semantic::AstNodes<'_>, id: NodeId) -> bool {
    nodes.ancestor_kinds(id).any(|kind| {
        matches!(
            kind,
            AstKind::ImportDeclaration(_)
                | AstKind::ExportFromDeclaration(_)
                | AstKind::ExportAllDeclaration(_)
                | AstKind::TSImportEqualsDeclaration(_)
        )
    })
}

/// The innermost callable of `outline` whose range holds `at`.
fn enclosing(outline: &[DocumentSymbol], at: &GraphPosition) -> Option<EnclosingCallable> {
    let mut best: Option<&DocumentSymbol> = None;
    let mut level = outline;
    // Ranges nest: descend into the child holding the site, keeping the
    // deepest callable met on the way.
    while let Some(symbol) = level
        .iter()
        .filter(|symbol| symbol.range.start <= *at && *at <= symbol.range.end)
        .max_by(|a, b| a.range.start.cmp(&b.range.start))
    {
        if CALLABLE_KINDS.contains(&symbol.kind) {
            best = Some(symbol);
        }
        level = &symbol.children;
    }
    best.map(|symbol| EnclosingCallable {
        name: symbol.name.clone(),
        kind: symbol.kind,
        range: symbol.range.clone(),
        selection_range: symbol.selection_range.clone(),
    })
}

#[cfg(test)]
mod tests;
