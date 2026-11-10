//! The declaration fact a node introduces, and what pushing one adds: its
//! `contains` edge, export and heritage facts.

use super::positions::NodePositions;
use super::walk::method_kind;
use super::{GraphAccumulator, heritage};
use tree_sitter::Node;

use crate::graph::{GraphFactDeclaration, GraphFactEdge, GraphFactExport};

use crate::signatures::languages::SCRIPT_BEARING_MARKUP;
use crate::signatures::nodes::{
    FunctionTypes, declaration_extent_from_parents, declaration_in, declaration_label,
    function_typed, init_attribute_names, instance_attribute, member_field,
};
use crate::signatures::visibility::VisibilityAdapter;
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
    let declared = declaration_in(node, ancestors, content)
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
        })?;
    if acc.ext != "py" {
        return Some(declared);
    }
    python_member(declared, node, ancestors, content, acc, active_decl)
}

/// A Python class's data members are declared once each, by the first
/// declaration of the name: its class-body attribute or `__init__`'s
/// assignment, in source order, then another method's assignment of a name
/// neither declares. An instance attribute is a member of the method's
/// class.
fn python_member(
    declared: GraphFactDeclaration,
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) -> Option<GraphFactDeclaration> {
    let attribute = (node.kind() == "attribute")
        .then(|| instance_attribute(node, ancestors.iter().rev().copied(), content))
        .flatten();
    let declared = match &attribute {
        Some(attribute) => {
            let class = acc
                .declarations
                .iter()
                .rev()
                .find(|method| Some(method.id.as_str()) == active_decl)
                .and_then(|method| method.parent.clone());
            if !attribute.in_init
                && acc
                    .init_attributes
                    .entry(attribute.class.id())
                    .or_insert_with(|| init_attribute_names(attribute.class, content))
                    .contains(&declared.name)
            {
                return None;
            }
            GraphFactDeclaration {
                parent: class,
                ..declared
            }
        }
        None => declared,
    };
    let class_member = attribute.is_some()
        || (matches!(declared.kind.as_str(), "property" | "constant")
            && declared
                .parent
                .as_deref()
                .is_some_and(|parent| parent.ends_with(":class")));
    if class_member
        && let Some(class) = declared.parent.clone()
        && !acc.class_members.insert((class, declared.name.clone()))
    {
        return None;
    }
    Some(declared)
}

/// Whether the declaration `owner` names is a type that is not exported: its
/// members are reached through it, so none is exported whatever its own modifier
/// says (a `pub` field of a `pub(super)` struct). `impl` blocks and modules do not
/// hide their items: those carry their own visibility.
fn hidden_by_owner(acc: &GraphAccumulator, owner: Option<&str>) -> bool {
    let Some(owner) = owner else { return false };
    acc.declarations
        .iter()
        .rfind(|declared| declared.id == owner)
        .is_some_and(|declared| {
            !declared.exported
                && matches!(
                    declared.kind.as_str(),
                    "struct" | "class" | "enum" | "interface" | "trait" | "union" | "record"
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
            // Annotations beside the node belong to its header only when the
            // node is its own extent: a wrapper already spans its header.
            let header = acc
                .leading_start
                .filter(|(id, _)| *id == extent.id())
                .map(|(_, start)| start);
            let range = line_index.declaration_range(extent, header);
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
            // A data member's visibility is written on its field declaration.
            let written = member_field(node, ancestors.iter().rev().copied()).unwrap_or(node);
            let exported = acc.visibility_adapter.exported(
                written,
                ancestors,
                content,
                &name,
                active_decl,
                active_decl.is_some_and(|parent| acc.public_namespace_scopes.contains(parent)),
            ) && !hidden_by_owner(acc, active_decl);
            let parent = active_decl.map(str::to_owned);
            let function_types = match acc.visibility_adapter {
                VisibilityAdapter::CFamily => Some(FunctionTypes::CFamily),
                #[cfg(feature = "tree-sitter-scala")]
                VisibilityAdapter::Scala => Some(FunctionTypes::Scala),
                #[cfg(feature = "tree-sitter-c-sharp")]
                VisibilityAdapter::CSharp => Some(FunctionTypes::CSharp),
                _ => None,
            };
            let function_typed = matches!(kind, "property" | "constant" | "variable")
                && function_types
                    .is_some_and(|language| function_typed(language, node, written, content));
            let (return_type, try_type) = match acc.ext.as_str() {
                "rs" => super::receiver::rust_return_types(node, ancestors, content),
                "go" => (super::receiver::go_return_type(node, content), None),
                _ => (None, None),
            };
            acc.non_reference_tokens.insert(name_token.start_byte());
            GraphFactDeclaration {
                id,
                name,
                kind: kind.to_owned(),
                line,
                range,
                selection_range: line_index.range(name_token),
                source_range: Some(super::validity::header_byte_range(extent, header)),
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
                function_typed,
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
