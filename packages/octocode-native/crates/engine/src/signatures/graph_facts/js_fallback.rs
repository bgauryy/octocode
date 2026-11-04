//! The module facts of a JS/TS file oxc refused, read from its tree-sitter
//! parse so the file keeps its imports, re-exports and written exports. The
//! parse holds their syntax, not the file's meaning: a `type` token settles a
//! type-only statement, and any other import or re-export stays
//! [`UNKNOWN_LOAD`], because a file no parser accepted makes no load claim.

use tree_sitter::Node;

use super::GraphAccumulator;
use super::positions::NodePositions;
use crate::graph::{GraphFactExport, GraphFactImport, UNKNOWN_LOAD};
use crate::signatures::nodes::node_text;

/// Whether grammar `extension` is one oxc owns: its tree-sitter facts are
/// always the fallback lane.
pub(super) fn is_fallback_lane(extension: &str) -> bool {
    crate::text::JS_TS_EXTENSIONS.contains(&extension)
}

fn has_token(node: Node<'_>, token: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == token)
}

fn named_child_of<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

fn named_children_of<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

/// The load a whole statement states: only its own `type` token, in a part of
/// the tree that parsed clean, settles anything.
fn statement_load(node: Node<'_>) -> &'static str {
    if has_token(node, "type") && !node.has_error() {
        "type"
    } else {
        UNKNOWN_LOAD
    }
}

/// The module a statement names, without its quotes.
fn module_source(holder: Node<'_>, content: &str) -> Option<String> {
    let source = holder.child_by_field_name("source")?;
    let text = node_text(source, content)?;
    let value = text
        .strip_prefix(['"', '\''])?
        .strip_suffix(['"', '\''])
        .unwrap_or_default();
    (!value.is_empty()).then(|| value.to_owned())
}

fn text_of(node: Option<Node<'_>>, content: &str) -> Option<String> {
    node_text(node?, content).map(str::to_owned)
}

/// One written import binding.
struct Binding<'t> {
    local: Option<Node<'t>>,
    imported: Option<String>,
    imported_node: Option<Node<'t>>,
    load: &'static str,
}

/// An `import` statement: one fact per binding, as the oxc lane records it.
pub(super) fn collect_import(
    node: Node<'_>,
    content: &str,
    positions: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
) {
    let statement = statement_load(node);
    let required = named_child_of(node, &["import_require_clause"]);
    let Some(specifier) = module_source(required.unwrap_or(node), content) else {
        return;
    };
    let mut bindings = Vec::new();
    if let Some(required) = required {
        bindings.push(Binding {
            local: named_child_of(required, &["identifier"]),
            imported: None,
            imported_node: None,
            load: statement,
        });
    }
    if let Some(clause) = named_child_of(node, &["import_clause"]) {
        let mut cursor = clause.walk();
        for part in clause.named_children(&mut cursor) {
            match part.kind() {
                "identifier" => bindings.push(Binding {
                    local: Some(part),
                    imported: Some("default".to_owned()),
                    imported_node: None,
                    load: statement,
                }),
                "namespace_import" => bindings.push(Binding {
                    local: named_child_of(part, &["identifier"]),
                    imported: Some("*".to_owned()),
                    imported_node: None,
                    load: statement,
                }),
                "named_imports" => {
                    for specifier in named_children_of(part, "import_specifier") {
                        let name = specifier.child_by_field_name("name");
                        bindings.push(Binding {
                            local: specifier.child_by_field_name("alias").or(name),
                            imported: text_of(name, content),
                            imported_node: name,
                            load: if statement == "type" {
                                statement
                            } else {
                                statement_load(specifier)
                            },
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let line = positions.range(node).start.line + 1;
    if bindings.is_empty() {
        // A side-effect import names nothing, but runs.
        bindings.push(Binding {
            local: None,
            imported: None,
            imported_node: None,
            load: UNKNOWN_LOAD,
        });
    }
    for (index, binding) in bindings.into_iter().enumerate() {
        acc.imports.push(GraphFactImport {
            id: format!("import:{specifier}:{line}:{index}"),
            specifier: specifier.clone(),
            line,
            import_kind: binding.load.to_owned(),
            local_name: text_of(binding.local, content),
            imported_name: binding.imported,
            imported_range: binding.imported_node.map(|name| positions.range(name)),
            local_range: binding.local.map(|local| positions.range(local)),
            resolution_hint: None,
            module_scope: None,
            scope_id: None,
        });
    }
}

/// An `export` statement: a re-export, a list of local bindings, or the
/// default. An exported declaration records itself as it is pushed.
pub(super) fn collect_export(
    node: Node<'_>,
    content: &str,
    positions: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
) {
    let line = positions.range(node).start.line + 1;
    let source = module_source(node, content);
    let load = statement_load(node);
    let mut push = |name: String, local: Option<String>, kind: &str| {
        acc.exports.push(GraphFactExport {
            id: format!("export:{name}:{line}:{}", acc.exports.len()),
            name,
            line,
            export_kind: kind.to_owned(),
            local_name: local,
            source: source.clone(),
        });
    };
    if let Some(clause) = named_child_of(node, &["export_clause"]) {
        for specifier in named_children_of(clause, "export_specifier") {
            let local = text_of(specifier.child_by_field_name("name"), content);
            let public = text_of(specifier.child_by_field_name("alias"), content).or(local.clone());
            let kind = if load == "type" {
                load
            } else {
                statement_load(specifier)
            };
            if let Some(public) = public {
                push(public, local, kind);
            }
        }
    } else if let Some(namespace) = named_child_of(node, &["namespace_export"]) {
        if let Some(name) = text_of(named_child_of(namespace, &["identifier"]), content) {
            push(name, None, load);
        }
    } else if source.is_some() && has_token(node, "*") {
        push("*".to_owned(), None, load);
    } else if has_token(node, "default") {
        let written = node
            .child_by_field_name("declaration")
            .and_then(|declaration| declaration.child_by_field_name("name"))
            .or_else(|| {
                node.child_by_field_name("value")
                    .filter(|value| value.kind() == "identifier")
            });
        push("default".to_owned(), text_of(written, content), "value");
    } else if has_token(node, "=") {
        // `export = name;`: the module's one export.
        let written = named_child_of(node, &["identifier"]);
        let name = text_of(written, content);
        push(
            name.clone().unwrap_or_else(|| "export=".to_owned()),
            name,
            "value",
        );
    }
}

/// A module-level `const f = () => {}`, `let x`, `var y`: its kind and name,
/// as the oxc lane declares it. Only a plain name binds here.
pub(super) fn variable<'t>(
    node: Node<'t>,
    ancestors: &[Node<'t>],
) -> Option<(&'static str, Node<'t>)> {
    if node.kind() != "variable_declarator" {
        return None;
    }
    let statement = *ancestors.last()?;
    if !matches!(
        statement.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        return None;
    }
    let holder = ancestors
        .len()
        .checked_sub(2)
        .map(|index| ancestors[index])?;
    let module_level = match holder.kind() {
        "program" => true,
        "export_statement" => {
            ancestors.len() >= 3 && ancestors[ancestors.len() - 3].kind() == "program"
        }
        _ => false,
    };
    if !module_level {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .filter(|name| name.kind() == "identifier")?;
    let callable = node.child_by_field_name("value").is_some_and(|value| {
        matches!(
            value.kind(),
            "arrow_function" | "function_expression" | "function" | "generator_function"
        )
    });
    let kind = if callable {
        "function"
    } else if has_token(statement, "const") {
        "constant"
    } else {
        "variable"
    };
    Some((kind, name))
}

/// `export { local as public }`, `export default local` and `export = local`
/// name their binding apart from its declaration: its flag and public names
/// settle only once the file is read.
pub(super) fn apply_local_exports(acc: &mut GraphAccumulator) {
    let mut public = std::collections::BTreeMap::<&str, Vec<String>>::new();
    for export in &acc.exports {
        if export.source.is_none()
            && let Some(local) = export.local_name.as_deref()
        {
            let names = public.entry(local).or_default();
            if !names.contains(&export.name) {
                names.push(export.name.clone());
            }
        }
    }
    for declaration in &mut acc.declarations {
        if declaration.parent.is_some() {
            continue;
        }
        let Some(names) = public.get(declaration.name.as_str()) else {
            continue;
        };
        declaration.exported = true;
        if names.iter().any(|name| *name != declaration.name) {
            declaration.exported_as.clone_from(names);
            declaration.exported_as.sort();
        }
    }
}

/// The diagnostic of a file oxc did not read: why, when oxc says, and what
/// that leaves unsettled.
pub(super) fn route_note(
    content: &str,
    file_path: &str,
    extension: &str,
) -> crate::graph::GraphFactDiagnostic {
    let mut diagnostic = super::note(crate::graph::GraphFactCode::ParserFallback);
    if let Some((reason, line)) =
        crate::signatures::js_oxc::parse_failure(content, file_path, extension)
    {
        diagnostic.message = format!("oxc reported: {reason}. {}", diagnostic.message);
        diagnostic.line = line;
    }
    diagnostic
}
