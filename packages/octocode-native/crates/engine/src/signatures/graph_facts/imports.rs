//! Import and module facts: Rust `use`, `mod` and `include!`, C/C++ `#include`, C# `using`.

use super::positions::NodePositions;
use super::rust::{collect_rust_imports, rust_inner_unsupported};
use super::walk::RustNodeContext;
use super::{GraphAccumulator, note};
use tree_sitter::Node;

use crate::graph::{GraphFactCode, GraphFactDiagnostic, GraphFactImport, GraphFactRustModule};

use crate::signatures::nodes::{clean_specifier, declaration_name, node_text};

/// Rust `use` imports.
pub(super) fn collect_rust_use(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    rust: RustNodeContext<'_>,
    deadline: std::time::Instant,
) {
    if let Some(argument) = node.child_by_field_name("argument") {
        collect_rust_imports(
            argument,
            rust.module_scope,
            content,
            line_index,
            acc,
            rust.context.unsupported,
            deadline,
        );
    }
}

/// A Rust `mod` item: its module fact and, for `mod name;`, a module import.
pub(super) fn collect_rust_module(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    rust: RustNodeContext<'_>,
) {
    if let Some(name) = declaration_name(node, content) {
        let line = line_index.range(node).start.line + 1;
        let path = rust.context.attributes.path.clone();
        let mut unsupported = rust.context.attributes.unsupported;
        unsupported |= rust.context.block_local;
        unsupported |= node
            .child_by_field_name("body")
            .is_some_and(|body| rust_inner_unsupported(body, content));
        let scope = rust.module_scope.to_vec();
        let inline = node.child_by_field_name("body").is_some();
        if unsupported {
            acc.diagnostics.push(GraphFactDiagnostic {
                line: Some(line),
                ..note(GraphFactCode::RustModuleAttributes)
            });
        }
        acc.modules.push(GraphFactRustModule {
            name: name.clone(),
            line,
            scope: scope.clone(),
            inline,
            path,
            unsupported,
        });
        if !inline {
            acc.imports.push(GraphFactImport {
                id: format!("module:{}:{line}", name),
                specifier: format!("self::{name}"),
                line,
                import_kind: "module".to_owned(),
                local_name: Some(name.clone()),
                imported_name: Some(name),
                imported_range: None,
                local_range: None,
                resolution_hint: unsupported.then(|| "unsupported".to_owned()),
                module_scope: Some(scope),
            });
        }
    }
}

/// A C/C++ `#include`: `"x"` resolves relative, `<x>` as a system header.
pub(super) fn collect_c_include(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
) {
    let path = node.child_by_field_name("path");
    let hint = match path.map(|item| item.kind()) {
        Some("string_literal") => "c-relative",
        Some("system_lib_string") => "c-system",
        _ => "unsupported",
    };
    if let Some(specifier) = path
        .and_then(|item| node_text(item, content))
        .and_then(clean_specifier)
    {
        push_language_import(
            acc,
            specifier,
            line_index.range(node).start.line + 1,
            "include",
            None,
            None,
            hint,
        );
    }
}

/// C# `using A.B;`, `using static A.B;`, `global using A.B;` and the alias
/// form `using X = A.B;`: the specifier is the namespace/type path as written,
/// the alias (when present) is the local name.
pub(super) fn collect_csharp_using(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
) {
    let alias = node.child_by_field_name("name");
    let mut cursor = node.walk();
    let target = node
        .named_children(&mut cursor)
        .find(|child| Some(*child) != alias && child.kind() != "comment");
    let (target, alias) = match (target, alias) {
        (Some(target), alias) => (target, alias),
        // `using System;` may surface its only name in the `name` field.
        (None, Some(name)) => (name, None),
        (None, None) => return,
    };
    let Some(specifier) =
        node_text(target, content).map(|text| text.split_whitespace().collect::<String>())
    else {
        return;
    };
    if specifier.is_empty() {
        return;
    }
    let line = line_index.range(node).start.line + 1;
    let local_name = alias
        .and_then(|alias| node_text(alias, content))
        .map(str::to_owned);
    acc.imports.push(GraphFactImport {
        id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
        specifier,
        line,
        import_kind: "value".to_owned(),
        local_name,
        imported_name: None,
        imported_range: None,
        local_range: alias.map(|alias| line_index.range(alias)),
        resolution_hint: None,
        module_scope: None,
    });
}

pub(super) fn push_language_import<'a>(
    acc: &'a mut GraphAccumulator,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    local_name: Option<String>,
    imported_name: Option<String>,
    hint: &'static str,
) -> &'a mut GraphFactImport {
    let index = acc.imports.len();
    acc.imports.push(GraphFactImport {
        id: format!("import:{}:{}", line, acc.imports.len()),
        specifier,
        line,
        import_kind: import_kind.to_owned(),
        local_name,
        imported_name,
        imported_range: None,
        local_range: None,
        resolution_hint: Some(hint.to_owned()),
        module_scope: None,
    });
    &mut acc.imports[index]
}
/// A file-inclusion macro (`include!`, `include_str!`, `include_bytes!`)
/// whose one argument is a plain string literal: an import of that path,
/// which the linker resolves against this file's directory. `include!`
/// splices code (`include`); `include_str!` and `include_bytes!` embed the
/// file as data (`include-data`). A computed path
/// (`concat!(env!("OUT_DIR"), …)`) names no file and is skipped.
pub(super) fn collect_rust_include(
    node: Node<'_>,
    content: &str,
    index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
) {
    let name = node
        .child_by_field_name("macro")
        .and_then(|name| node_text(name, content))
        .map(|name| name.rsplit("::").next().unwrap_or(name).trim());
    let import_kind = match name {
        Some("include") => "include",
        Some("include_str" | "include_bytes") => "include-data",
        _ => return,
    };
    let mut cursor = node.walk();
    let Some(arguments) = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
    else {
        return;
    };
    let mut cursor = arguments.walk();
    let mut literals = arguments.named_children(&mut cursor);
    let (Some(literal), None) = (literals.next(), literals.next()) else {
        return;
    };
    let Some(specifier) = (literal.kind() == "string_literal")
        .then(|| node_text(literal, content))
        .flatten()
        .and_then(|text| text.strip_prefix('"')?.strip_suffix('"'))
        .filter(|path| !path.is_empty() && !path.contains('\\'))
        .map(str::to_owned)
    else {
        return;
    };
    let line = index.range(node).start.line + 1;
    acc.imports.push(GraphFactImport {
        id: format!("include:{specifier}:{line}:{}", acc.imports.len()),
        specifier,
        line,
        import_kind: import_kind.to_owned(),
        local_name: None,
        imported_name: None,
        imported_range: None,
        local_range: None,
        resolution_hint: None,
        module_scope: None,
    });
}
