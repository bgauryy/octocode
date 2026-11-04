//! The declaration fact a node introduces, and what pushing one adds: its
//! `contains` edge, export and heritage facts.

use super::positions::NodePositions;
use super::walk::method_kind;
use super::{GraphAccumulator, heritage};
use tree_sitter::Node;

use crate::graph::{GraphFactDeclaration, GraphFactEdge, GraphFactExport};

use crate::signatures::languages::SCRIPT_BEARING_MARKUP;
use crate::signatures::nodes::{
    declaration_extent_from_parents, declaration_label, declaration_under,
};
use crate::text::{JS_TS_EXTENSIONS, extension_of};

/// Statement keywords that error recovery turns into declaration names:
/// `if (x) { .. }` after a syntax error reads as a method `if() {}`.
const STATEMENT_KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "do", "switch", "case", "catch", "try", "finally", "return",
    "throw", "new", "typeof", "function", "var", "let", "const", "with",
];

/// Grammars whose error recovery reads a statement as a declaration. In every
/// other language these words are ordinary names (`fn new`, `def new`,
/// `def case`), so filtering them silently drops real declarations.
fn recovers_statements_as_declarations(file_path: &str) -> bool {
    let extension = extension_of(file_path, true, "");
    JS_TS_EXTENSIONS.contains(&extension.as_str())
        || SCRIPT_BEARING_MARKUP.contains(&extension.as_str())
}

/// In a JS/TS-family file a keyword-named declaration is a recovery artifact
/// unless it is a class member (`class A { if() {} }` is legal) parsed without
/// errors around it. Membership comes from the raw node kind: the normalized
/// declaration kind collapses methods into `function`.
fn is_recovered_keyword_declaration(node: Node<'_>, name: &str, file_path: &str) -> bool {
    if !STATEMENT_KEYWORDS.contains(&name) || !recovers_statements_as_declarations(file_path) {
        return false;
    }
    let member = matches!(
        node.kind(),
        "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "public_field_definition"
            | "property_signature"
    );
    !member || node.has_error() || node.parent().is_some_and(|parent| parent.is_error())
}

/// The declaration `node` introduces, if any; marks its name token as a
/// non-reference.
pub(super) fn declaration_fact(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) -> Option<GraphFactDeclaration> {
    // The walk carries the parent; only its own root may sit inside a tree.
    let parent = ancestors.last().copied().or_else(|| node.parent());
    declaration_under(node, parent, content)
        .or_else(|| {
            acc.js_fallback
                .then(|| super::js_fallback::variable(node, ancestors))
                .flatten()
        })
        .and_then(|(kind, name_token)| {
            named_declaration(
                kind,
                name_token,
                node,
                ancestors,
                content,
                line_index,
                acc,
                active_decl,
            )
        })
}

/// The declaration fact of `node` as `kind`, named by `name_token`.
pub(super) fn named_declaration(
    kind: &'static str,
    name_token: Node<'_>,
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) -> Option<GraphFactDeclaration> {
    let kind = method_kind(kind, node, active_decl);
    let test =
        super::test_adapter::test_identity(node, content, &acc.ext, name_token, &acc.test_context);
    let name_token = test
        .as_ref()
        .map_or(name_token, |(_, selection)| *selection);
    let test_identity = test.map(|(identity, _)| identity);
    test_identity
        .as_ref()
        .map(crate::graph::GraphTestIdentity::display_name)
        .or_else(|| declaration_label(node, name_token, content))
        .filter(|name| !is_recovered_keyword_declaration(node, name, &acc.file_path))
        .map(|name| {
            let extent = declaration_extent_from_parents(node, ancestors.iter().rev().copied());
            let range = line_index.declaration_range(extent);
            // `line` stays where the declaration node itself starts; the
            // range also holds its template or decorator header.
            let line = line_index.range(node).start.line + 1;
            // This identifies a declaration occurrence, not a canonical binding.
            // Location distinguishes overloads, impl blocks and equal names in scopes.
            let id = format!(
                "declaration:{}#{}@{}:{}",
                acc.file_path,
                name,
                node.start_byte(),
                kind
            );
            let exported = acc.visibility_adapter.exported(
                node,
                ancestors,
                content,
                &name,
                active_decl,
                active_decl.is_some_and(|parent| acc.public_namespace_scopes.contains(parent)),
            );
            let parent = active_decl.map(str::to_owned);
            let (return_type, try_type) = if acc.ext == "rs" {
                super::receiver::rust_return_types(node, ancestors, content)
            } else {
                (None, None)
            };
            acc.non_reference_tokens.insert(name_token.start_byte());
            GraphFactDeclaration {
                id,
                name,
                kind: kind.to_owned(),
                line,
                range,
                selection_range: line_index.range(name_token),
                source_range: Some(super::validity::byte_range(extent)),
                selection_byte_range: Some(super::validity::byte_range(name_token)),
                parse_assessment: super::validity::assess(node, ancestors, line_index),
                test_identity,
                exported,
                parent,
                owner: match acc.visibility_adapter {
                    _ if acc.ext == "go" => super::go::method_owner(node, content),
                    crate::signatures::visibility::VisibilityAdapter::CFamily => {
                        super::c_family::out_of_line_scope(node, content)
                    }
                    _ => None,
                },
                doc_line: None,
                exported_as: Vec::new(),
                prototype: matches!(
                    acc.visibility_adapter,
                    crate::signatures::visibility::VisibilityAdapter::CFamily
                ) && super::c_family::is_prototype(node, kind),
                return_type,
                try_type,
            }
        })
}

/// Push `declaration` with its `contains` edge, export and heritage facts;
/// returns its `(id, name)`.
pub(super) fn push_declaration(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    declaration: GraphFactDeclaration,
) -> (String, String) {
    let id = declaration.id.clone();
    let name = declaration.name.clone();
    let line = declaration.line;
    let exported = declaration.exported;
    if let Some(parent) = &declaration.parent {
        acc.edges.push(GraphFactEdge {
            id: format!("{parent}->{id}:contains"),
            from: parent.clone(),
            to: id.clone(),
            relation: "contains".to_owned(),
            source: "ast".to_owned(),
            line,
            resolution: "syntactic".to_owned(),
        });
    }
    if exported {
        acc.exports.push(GraphFactExport {
            id: format!("export:{}:{}", name, line),
            name: name.clone(),
            line,
            export_kind: "language-public".to_owned(),
            local_name: Some(name.clone()),
            source: None,
        });
    }
    if declaration.exported && acc.visibility_adapter.transparent_namespace(node) {
        acc.public_namespace_scopes.insert(id.clone());
    }
    acc.declarations.push(declaration);
    acc.declaration_spans
        .push((node.start_byte(), node.end_byte()));
    heritage::collect_heritage(node, content, line_index, acc, &id);
    (id, name)
}
