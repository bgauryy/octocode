//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use serde::Serialize;
use tree_sitter::Node;

#[cfg(test)]
use crate::text::file_extension::get_extension_internal;

use super::languages;
use super::nodes::{
    call_callee, clean_specifier, compact_identifier, declaration, declaration_name,
    import_specifier, is_call_node, is_exported_declaration, is_import_node, is_name_leaf,
    last_name_leaf, node_text,
};

mod heritage;
mod python;
mod receiver;
mod rust;

/// Caller label of calls outside any declaration (matches the OXC lane).
const MODULE_CALLER: &str = "module";

use python::collect_python_imports;
use rust::{RustContext, collect_rust_imports, rust_child_contexts, rust_inner_unsupported};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphFacts {
    kind: &'static str,
    schema_version: u32,
    source: &'static str,
    language: String,
    file: String,
    declarations: Vec<GraphDeclaration>,
    imports: Vec<GraphImport>,
    exports: Vec<GraphExport>,
    calls: Vec<GraphCall>,
    edges: Vec<GraphEdge>,
    diagnostics: Vec<String>,
    modules: Vec<GraphRustModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_root_unsupported: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphDeclaration {
    id: String,
    name: String,
    kind: &'static str,
    line: u32,
    range: Range,
    selection_range: Range,
    exported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    /// 0-based first line of the comment block directly above.
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_line: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphImport {
    id: String,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    imported_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    imported_range: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_range: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution_hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_scope: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphRustModule {
    name: String,
    line: u32,
    scope: Vec<String>,
    inline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    unsupported: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphExport {
    id: String,
    name: String,
    line: u32,
    export_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCall {
    id: String,
    caller: String,
    /// Declaration id of the enclosing caller; `None` for module-level code.
    #[serde(skip_serializing_if = "Option::is_none")]
    caller_id: Option<String>,
    callee: String,
    line: u32,
    range: Range,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    receiver_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphEdge {
    id: String,
    from: String,
    to: String,
    relation: &'static str,
    source: &'static str,
    line: u32,
    resolution: &'static str,
}

#[derive(Serialize)]
struct Position {
    line: u32,
    character: u32,
}

#[derive(Serialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphFactCapability {
    extension: String,
    language: String,
    language_id: Option<String>,
    structural_search: bool,
    signature_outline: bool,
    graph_facts: bool,
    fact_families: Vec<&'static str>,
}

/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic.
struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn range(&self, node: Node<'_>) -> Range {
        Range {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
        }
    }

    fn position(&self, byte_offset: usize) -> Position {
        let (line, character) = self.0.byte_to_position(byte_offset as u32);
        Position { line, character }
    }
}

struct GraphAccumulator {
    file_path: String,
    ext: String,
    declarations: Vec<GraphDeclaration>,
    imports: Vec<GraphImport>,
    exports: Vec<GraphExport>,
    calls: Vec<GraphCall>,
    edges: Vec<GraphEdge>,
    diagnostics: Vec<String>,
    modules: Vec<GraphRustModule>,
    /// Start bytes of name tokens that are not value references: declaration
    /// names and the callee tokens of recorded calls (call edges).
    non_reference_tokens: std::collections::HashSet<usize>,
    /// Bodies of Rust item-level macro calls (`cfg_rt! { mod x; }`) with
    /// their enclosing module scope, re-read as items after the main walk.
    macro_bodies: Vec<(tree_sitter::Range, Vec<String>)>,
    /// Byte span of each entry of `declarations`, in the same order.
    declaration_spans: Vec<(usize, usize)>,
    /// Indices into `imports` of bindings from private Rust `use` items,
    /// whose uses `rust_import_uses` attributes to declarations.
    private_use_imports: Vec<usize>,
}

impl GraphAccumulator {
    fn new(file_path: &str, ext: &str) -> Self {
        Self {
            file_path: file_path.to_owned(),
            ext: ext.to_owned(),
            declarations: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            calls: Vec::new(),
            edges: Vec::new(),
            modules: Vec::new(),
            non_reference_tokens: std::collections::HashSet::new(),
            macro_bodies: Vec::new(),
            declaration_spans: Vec::new(),
            private_use_imports: Vec::new(),
            diagnostics: vec![
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof".to_owned(),
            ],
        }
    }
}

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
    let extension = file_path.rsplit_once('.').map_or("", |(_, ext)| ext);
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "js" | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "mts"
            | "cts"
            | "vue"
            | "svelte"
            | "astro"
            | "html"
            | "htm"
    )
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

#[cfg(test)]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

#[cfg(test)]
pub(crate) fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<super::GraphFactsExtraction> {
    let extension = get_extension_internal(file_path, true, "txt");
    extract_graph_facts_with_metadata_with_extension(content, file_path, &extension)
}

pub(crate) fn extract_graph_facts_with_metadata_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<super::GraphFactsExtraction> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_graph_facts_with_metadata_inner(content, file_path, extension)
    }))
    .unwrap_or(None)
}

fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<super::GraphFactsExtraction> {
    extract_graph_facts_with_metadata_before(
        content,
        file_path,
        extension,
        std::time::Instant::now() + super::extractor::AST_EXECUTION_TIMEOUT,
    )
}

#[cfg(test)]
fn extract_graph_facts_before(
    content: &str,
    file_path: &str,
    deadline: std::time::Instant,
) -> Option<String> {
    let extension = get_extension_internal(file_path, true, "txt");
    extract_graph_facts_with_metadata_before(content, file_path, &extension, deadline)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn extract_graph_facts_with_metadata_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
) -> Option<super::GraphFactsExtraction> {
    let ext = extension.to_owned();
    if !graph_fact_extensions().iter().any(|item| item == &ext) {
        return None;
    }
    let entry = languages::find_entry(&ext)?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
    let mut reference_counts = Vec::new();
    let mut import_uses = None;
    let rust = ext == "rs";
    if let Some(tree) = super::extractor::parse_before(content, &entry.language, deadline) {
        let root = tree.root_node();
        let line_index = LineIndex::new(content);
        rust_root_unsupported = (ext == "rs").then(|| rust_inner_unsupported(root, content));
        if rust_root_unsupported == Some(true) {
            acc.diagnostics
                .push("unsupported Rust inner conditional or custom crate attributes".to_owned());
        }
        if root.has_error() {
            acc.diagnostics.push(
                "tree-sitter recovered from parse errors; graph facts may be partial".to_owned(),
            );
        }
        if !visit_node(root, content, &line_index, &mut acc, deadline, &[])
            || !visit_macro_bodies(content, &entry.language, &line_index, &mut acc, deadline)
        {
            // Facts gathered before the deadline are positive syntax facts and
            // stay. The diagnostic marks the file incomplete, so consumers must
            // not read a missing import, call or module as absent.
            acc.diagnostics.push("graph.traversal.deadlineExceeded: graph extraction exceeded its execution deadline; facts are incomplete".to_owned());
            rust_root_unsupported = (ext == "rs").then_some(true);
        } else if let Some(counts) = count_name_references(root, content, &acc, deadline) {
            // A count cut short by the deadline would undercount; leaving it
            // out makes consumers treat every declaration as escaping.
            reference_counts = acc
                .declarations
                .iter()
                .map(|declaration| crate::types::GraphReferenceCount {
                    declaration_id: declaration.id.clone(),
                    count: counts.get(&declaration.name).copied().unwrap_or(0),
                })
                .collect();
            import_uses = rust
                .then(|| rust_import_uses(root, content, &acc, deadline))
                .flatten();
        }
    } else {
        acc.diagnostics.push(
            "graph.parse.deadlineExceeded: graph parsing was interrupted; facts are incomplete"
                .to_owned(),
        );
    }

    let lines = content.lines().collect::<Vec<_>>();
    for declaration in &mut acc.declarations {
        declaration.doc_line =
            super::leading_doc_line(&lines, declaration.range.start.line as usize, &ext);
    }
    let private_use_imports = std::mem::take(&mut acc.private_use_imports);
    let facts = GraphFacts {
        kind: "graphFacts",
        schema_version: super::GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast",
        language: language_label(&ext, entry.language_id),
        file: file_path.to_owned(),
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        edges: acc.edges,
        diagnostics: acc.diagnostics,
        modules: acc.modules,
        rust_root_unsupported,
    };
    let facts_json = serde_json::to_string(&facts).ok()?;
    let mut facts = crate::graph::GraphFactsDocument::from_json(&facts_json).ok()?;
    if let Some(mut uses) = import_uses {
        for index in private_use_imports {
            if let Some(import) = facts.imports.get_mut(index)
                && let Some(local) = import.local_name.as_deref()
                && import.imported_name.as_deref() != Some("*")
            {
                import.used_in = uses.remove(local).map(|mut users| {
                    users.sort_unstable();
                    users
                });
            }
        }
    }
    Some(super::GraphFactsExtraction {
        facts,
        reference_counts,
    })
}

/// Where each binding of a private Rust `use` is named, by local name: the id
/// of the innermost declaration around each name token outside `use` items,
/// or `IMPORT_USE_MODULE`. Name matching is syntactic, so a shadowing local
/// only adds uses. Scopes whose code runs without a by-name caller (trait and
/// inherent `impl` blocks, traits, macro definitions, `#[test]`/`#[cfg(test)]`
/// or bench items) count as module-level. A name with no use at all is left
/// out, because a trait brought into scope for its methods is never named;
/// consumers treat the missing entry as used. `None` when the deadline cut
/// the walk short.
fn rust_import_uses(
    root: Node<'_>,
    content: &str,
    acc: &GraphAccumulator,
    deadline: std::time::Instant,
) -> Option<std::collections::HashMap<String, Vec<String>>> {
    let names = acc
        .private_use_imports
        .iter()
        .filter_map(|index| acc.imports.get(*index)?.local_name.as_deref())
        .filter(|name| *name != "_")
        .collect::<std::collections::HashSet<_>>();
    let mut uses: std::collections::HashMap<String, Vec<String>> = Default::default();
    if names.is_empty() {
        return Some(uses);
    }
    let innermost = |at: usize| {
        acc.declaration_spans
            .iter()
            .zip(&acc.declarations)
            .filter(|((start, end), _)| *start <= at && at < *end)
            .min_by_key(|((start, end), _)| end - start)
            .map_or(crate::graph::IMPORT_USE_MODULE, |(_, declaration)| {
                declaration.id.as_str()
            })
    };
    let mut record = |name: &str, at: usize, module_level: bool| {
        if names.contains(name) {
            let user = if module_level {
                crate::graph::IMPORT_USE_MODULE
            } else {
                innermost(at)
            };
            let users = uses.entry(name.to_owned()).or_default();
            if !users.iter().any(|known| known == user) {
                users.push(user.to_owned());
            }
        }
    };
    // (node, inside a module-level scope, parent is a macro token tree)
    let mut pending = vec![(root, false, false)];
    let mut cursor = root.walk();
    let mut steps = 0_u32;
    while let Some((node, module_level, in_tokens)) = pending.pop() {
        if polls_deadline(&mut steps) && std::time::Instant::now() >= deadline {
            return None;
        }
        let kind = node.kind();
        if kind == "use_declaration" {
            continue;
        }
        if is_name_leaf(node) {
            if let Some(text) = node_text(node, content) {
                record(text, node.start_byte(), module_level);
            }
            continue;
        }
        if in_tokens && kind == "string_literal" {
            for name in node_text(node, content)
                .map(inline_format_captures)
                .unwrap_or_default()
            {
                record(name, node.start_byte(), module_level);
            }
            continue;
        }
        let scope = module_level || matches!(kind, "impl_item" | "trait_item" | "macro_definition");
        // Outer attributes are siblings before their item.
        let mut test_item = false;
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "attribute_item" => {
                    let text = node_text(child, content).unwrap_or_default();
                    test_item |= text.contains("test") || text.contains("bench");
                    pending.push((child, scope, false));
                }
                "line_comment" | "block_comment" => {}
                _ => {
                    pending.push((child, scope || test_item, kind == "token_tree"));
                    test_item = false;
                }
            }
        }
    }
    Some(uses)
}

/// Per-name counts of identifier-kind tokens that reference a declared name,
/// skipping declaration names and call-edge callee tokens. Comments and string
/// contents are never identifier tokens, so they cannot count; Rust inline
/// format captures (`"{name}"` inside a macro) are the one string form that
/// names a binding and are counted. No scope resolution: equal names share a
/// count. `None` when the deadline cut the walk short.
fn count_name_references(
    root: Node<'_>,
    content: &str,
    acc: &GraphAccumulator,
    deadline: std::time::Instant,
) -> Option<std::collections::HashMap<String, u32>> {
    let mut counts: std::collections::HashMap<String, u32> = acc
        .declarations
        .iter()
        .map(|declaration| (declaration.name.clone(), 0))
        .collect();
    if counts.is_empty() {
        return Some(counts);
    }
    let rust = acc.ext == "rs";
    let mut pending = vec![root];
    let mut cursor = root.walk();
    let mut steps = 0_u32;
    while let Some(node) = pending.pop() {
        if polls_deadline(&mut steps) && std::time::Instant::now() >= deadline {
            return None;
        }
        if is_name_leaf(node) {
            if !acc.non_reference_tokens.contains(&node.start_byte())
                && let Some(count) = node_text(node, content).and_then(|text| counts.get_mut(text))
            {
                *count += 1;
            }
            continue;
        }
        if rust
            && node.kind() == "string_literal"
            && node
                .parent()
                .is_some_and(|parent| parent.kind() == "token_tree")
        {
            for name in node_text(node, content)
                .map(inline_format_captures)
                .unwrap_or_default()
            {
                if let Some(count) = counts.get_mut(name) {
                    *count += 1;
                }
            }
            continue;
        }
        pending.extend(node.named_children(&mut cursor));
    }
    Some(counts)
}

/// Whether a node walk should read the clock at this step: the first step
/// and then once per 256 nodes. Per-node work is bounded (no parent climbs),
/// so a coarse poll overshoots the deadline by at most a few hundred nodes.
fn polls_deadline(steps: &mut u32) -> bool {
    let poll = steps.is_multiple_of(256);
    *steps = steps.wrapping_add(1);
    poll
}

/// Identifiers captured by a Rust format string: `name` in `{name}` or
/// `{name:?}`; `{{` escapes and positional `{0}`/`{}` are skipped.
fn inline_format_captures(literal: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = literal;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(escaped) = rest.strip_prefix('{') {
            rest = escaped;
            continue;
        }
        let Some(end) = rest.find(['}', ':']) else {
            break;
        };
        let name = &rest[..end];
        if name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            names.push(name);
        }
        rest = &rest[end..];
    }
    names
}

pub fn graph_fact_extensions() -> Vec<String> {
    let mut exts: Vec<String> = languages::signature_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

pub fn graph_fact_capabilities_json() -> String {
    let graph_exts = graph_fact_extensions();
    let capabilities: Vec<GraphFactCapability> = graph_exts
        .iter()
        .filter_map(|ext| {
            let entry = languages::find_entry(ext)?;
            Some(GraphFactCapability {
                extension: ext.clone(),
                language: language_label(ext, entry.language_id),
                language_id: entry.language_id.map(str::to_owned),
                structural_search: true,
                signature_outline: true,
                graph_facts: true,
                fact_families: fact_families_for_extension(ext),
            })
        })
        .collect();
    serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned())
}

/// A macro call in item position: directly in a file or module body, or as
/// an `expression_statement` there (`make_items!();`).
fn is_item_level(node: Node<'_>) -> bool {
    let item_parent = |node: Node<'_>| matches!(node.kind(), "source_file" | "declaration_list");
    node.parent().is_some_and(|parent| {
        item_parent(parent)
            || parent.kind() == "expression_statement" && parent.parent().is_some_and(item_parent)
    })
}

fn push_macro_gap(acc: &mut GraphAccumulator) {
    let message = "unsupported Rust macro expansion: macro-generated imports are not linked";
    if !acc
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic == message)
    {
        acc.diagnostics.push(message.to_owned());
    }
}

/// Read recorded Rust item-level macro bodies as items (nested bodies too,
/// bounded). A body that does not parse as items stays an explicit gap.
fn visit_macro_bodies(
    content: &str,
    language: &tree_sitter::Language,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
) -> bool {
    const MAX_MACRO_BODIES: usize = 256;
    let mut visited = 0;
    while let Some((range, scope)) = acc.macro_bodies.pop() {
        visited += 1;
        let parsed = (visited <= MAX_MACRO_BODIES)
            .then(|| super::extractor::parse_ranges_before(content, language, &[range], deadline))
            .flatten();
        match parsed {
            Some(tree) if !tree.root_node().has_error() => {
                if !visit_node(tree.root_node(), content, line_index, acc, deadline, &scope) {
                    return false;
                }
            }
            _ => {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                push_macro_gap(acc);
            }
        }
    }
    true
}

fn visit_node(
    root: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
    outer_scope: &[String],
) -> bool {
    enum Frame<'tree> {
        /// A node, its carried Rust context, and its depth below the root.
        Enter(Node<'tree>, RustContext, usize),
        ExitDeclaration,
        ExitModule,
    }

    let rust = acc.ext == "rs";
    let mut receivers = receiver::ReceiverTypes::new(&acc.ext, root);
    let mut frames = vec![Frame::Enter(root, RustContext::default(), 0)];
    let mut declarations: Vec<(String, String)> = Vec::new();
    // Names of the enclosing `mod` items, outermost first.
    let mut module_scope: Vec<String> = outer_scope.to_vec();
    let mut children = Vec::new();
    let mut cursor = root.walk();
    let mut steps = 0_u32;
    // Ancestors of the node being entered, root first: the traversal keeps
    // them so no collector has to climb `Node::parent()`.
    let mut path: Vec<Node<'_>> = Vec::new();
    while let Some(frame) = frames.pop() {
        if polls_deadline(&mut steps) && std::time::Instant::now() >= deadline {
            return false;
        }
        let (node, mut context) = match frame {
            Frame::Enter(node, context, depth) => {
                path.truncate(depth);
                (node, context)
            }
            Frame::ExitDeclaration => {
                declarations.pop();
                continue;
            }
            Frame::ExitModule => {
                module_scope.pop();
                continue;
            }
        };
        if rust {
            context.unsupported |= rust_inner_unsupported(node, content);
        }
        let active = declarations.last();
        let rust_node = rust.then_some(RustNodeContext {
            context: &context,
            module_scope: &module_scope,
        });
        let calls_before = acc.calls.len();
        let identity = collect_node_facts(
            node,
            content,
            line_index,
            acc,
            active.map(|(id, _)| id.as_str()),
            active.map(|(_, name)| name.as_str()),
            rust_node,
            deadline,
        );
        path.push(node);
        if acc.calls.len() > calls_before
            && let Some(receivers) = receivers.as_mut()
            && let Some(call) = acc.calls.last_mut()
        {
            call.receiver_type = receivers.receiver_type(node, &path, content);
        }
        let depth = path.len();
        if let Some(identity) = identity {
            declarations.push(identity);
            frames.push(Frame::ExitDeclaration);
        }
        if rust
            && node.kind() == "mod_item"
            && let Some(name) = declaration_name(node, content)
        {
            module_scope.push(name);
            frames.push(Frame::ExitModule);
        }

        // Preserve preorder and declaration lifetimes without using the call stack.
        // A cursor enumerates wide sibling lists without repeated child indexing.
        children.clear();
        cursor.reset(node);
        children.extend(node.named_children(&mut cursor));
        let children_start = frames.len();
        if rust {
            let inherited = context.for_children(node);
            let contexts = rust_child_contexts(&children, content, &inherited);
            frames.extend(
                children
                    .iter()
                    .zip(contexts)
                    .map(|(child, context)| Frame::Enter(*child, context, depth)),
            );
        } else {
            frames.extend(
                children
                    .iter()
                    .map(|child| Frame::Enter(*child, RustContext::default(), depth)),
            );
        }
        frames[children_start..].reverse();
    }
    std::time::Instant::now() < deadline
}

/// Rust traversal state for one node: its carried context and the names of
/// the `mod` items that enclose it (outermost first, the node itself excluded).
#[derive(Clone, Copy)]
struct RustNodeContext<'a> {
    context: &'a RustContext,
    module_scope: &'a [String],
}

/// A Go `import ( ... )` / `import "x"` declaration: its `import_spec`
/// children are the imports; the declaration itself would duplicate the
/// first one on the `import` line.
fn is_grouped_go_import(node: Node<'_>) -> bool {
    if node.kind() != "import_declaration" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| matches!(child.kind(), "import_spec" | "import_spec_list"))
}

/// Emits declaration/edge facts for `node`.
///
/// Note the intentional coordinate-basis split on every emitted `GraphDeclaration`:
/// `line` is 1-based (human-facing, computed as `range.start.line + 1`) while
/// `range`/`selection_range` are 0-based (`LineIndex` coordinates). Consumers must
/// not mix the two bases — use `range` for zero-based math and `line` for display.
fn collect_node_facts(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    rust: Option<RustNodeContext<'_>>,
    deadline: std::time::Instant,
) -> Option<(String, String)> {
    // Only item-level macro calls can generate `mod`/`use` items; their
    // bodies are re-read as Rust items after the main walk.
    if acc.ext == "rs"
        && node.kind() == "macro_invocation"
        && is_item_level(node)
        && let Some(rust) = rust.as_ref()
    {
        let mut cursor = node.walk();
        let body = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "token_tree")
            .filter(|body| body.end_byte() > body.start_byte() + 2);
        if body.is_none() {
            // `make_imports!();` expands from its definition, which is not
            // visible here: the generated items stay an explicit gap.
            push_macro_gap(acc);
        }
        if let Some(body) = body {
            let inner = tree_sitter::Range {
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
            };
            acc.macro_bodies.push((inner, rust.module_scope.to_vec()));
        }
    }
    let decl = declaration(node, content).and_then(|(kind, name_token)| {
        node_text(name_token, content)
            .and_then(compact_identifier)
            .filter(|name| !is_recovered_keyword_declaration(node, name, &acc.file_path))
            .map(|name| {
                let range = line_index.range(node);
                let line = range.start.line + 1;
                // This identifies a declaration occurrence, not a canonical binding.
                // Location distinguishes overloads, impl blocks and equal names in scopes.
                let id = format!(
                    "declaration:{}#{}@{}:{}",
                    acc.file_path,
                    name,
                    node.start_byte(),
                    kind
                );
                let exported = is_exported_declaration(&acc.ext, node, content, &name, active_decl);
                let parent = active_decl.map(str::to_owned);
                acc.non_reference_tokens.insert(name_token.start_byte());
                GraphDeclaration {
                    id,
                    name,
                    kind,
                    line,
                    range,
                    selection_range: line_index.range(name_token),
                    exported,
                    parent,
                    doc_line: None,
                }
            })
    });

    // Keep the new declaration id alive for the entire child traversal so we
    // can pass it as &str without any per-child heap allocation.
    let next_decl_identity: Option<(String, String)> = if let Some(declaration) = decl {
        let id = declaration.id.clone();
        let name = declaration.name.clone();
        let line = declaration.line;
        let exported = declaration.exported;
        if let Some(parent) = &declaration.parent {
            acc.edges.push(GraphEdge {
                id: format!("{parent}->{id}:contains"),
                from: parent.clone(),
                to: id.clone(),
                relation: "contains",
                source: "ast",
                line,
                resolution: "syntactic",
            });
        }
        if exported {
            acc.exports.push(GraphExport {
                id: format!("export:{}:{}", name, line),
                name: name.clone(),
                line,
                export_kind: "language-public",
                local_name: Some(name.clone()),
                source: None,
            });
        }
        acc.declarations.push(declaration);
        acc.declaration_spans
            .push((node.start_byte(), node.end_byte()));
        heritage::collect_heritage(node, content, line_index, acc, &id);
        Some((id, name))
    } else {
        None
    };
    // Inherit the parent scope when no new declaration was established.
    let next_decl = next_decl_identity
        .as_ref()
        .map(|(id, _)| id.as_str())
        .or(active_decl);
    let next_name = next_decl_identity
        .as_ref()
        .map(|(_, name)| name.as_str())
        .or(active_name);

    if let Some(rust) = rust
        && node.kind() == "use_declaration"
    {
        if let Some(argument) = node.child_by_field_name("argument") {
            let first = acc.imports.len();
            collect_rust_imports(
                argument,
                rust.module_scope,
                content,
                line_index,
                acc,
                rust.context.unsupported,
                deadline,
            );
            // A `pub use` re-exports: its binding is used by whoever imports
            // this module, which no local reference shows.
            let mut cursor = node.walk();
            if !node
                .named_children(&mut cursor)
                .any(|child| child.kind() == "visibility_modifier")
            {
                acc.private_use_imports.extend(first..acc.imports.len());
            }
        }
    } else if let Some(rust) = rust
        && node.kind() == "mod_item"
    {
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
                acc.diagnostics.push(format!(
                    "unsupported Rust conditional or custom module attributes at line {line}"
                ));
            }
            acc.modules.push(GraphRustModule {
                name: name.clone(),
                line,
                scope: scope.clone(),
                inline,
                path,
                unsupported,
            });
            if !inline {
                acc.imports.push(GraphImport {
                    id: format!("module:{}:{line}", name),
                    specifier: format!("self::{name}"),
                    line,
                    import_kind: "module",
                    local_name: Some(name.clone()),
                    imported_name: Some(name),
                    imported_range: None,
                    local_range: None,
                    resolution_hint: unsupported.then_some("unsupported"),
                    module_scope: Some(scope),
                });
            }
        }
    } else if matches!(acc.ext.as_str(), "py" | "pyi")
        && matches!(node.kind(), "import_statement" | "import_from_statement")
    {
        collect_python_imports(node, content, line_index, acc);
    } else if node.kind() == "preproc_include" {
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
    } else if acc.ext == "cs" && node.kind() == "using_directive" {
        collect_csharp_using(node, content, line_index, acc);
    } else if is_import_node(node.kind())
        && !is_grouped_go_import(node)
        && let Some(specifier) = import_specifier(node, content)
    {
        let line = line_index.range(node).start.line + 1;
        acc.imports.push(GraphImport {
            id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
            specifier,
            line,
            import_kind: "value",
            local_name: None,
            imported_name: None,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
        });
    }

    if is_call_node(node.kind())
        && let Some((callee, callee_node)) = call_callee(node, content)
    {
        let target = callee
            .rsplit(['.', ':'])
            .find(|segment| !segment.is_empty());
        if let Some(token) = target.and_then(|name| last_name_leaf(callee_node, content, name)) {
            acc.non_reference_tokens.insert(token.start_byte());
        }
        let range = line_index.range(node);
        let line = range.start.line + 1;
        // Module-level code (a Python decorator or `main()` guard, a Go
        // package `var` initializer, a Rust item-level macro) has no
        // enclosing declaration: it is owned by the `module` placeholder
        // with no caller id, the same shape the OXC lane emits.
        let caller_name = match next_decl {
            Some(caller) => next_name.unwrap_or(caller).to_owned(),
            None => MODULE_CALLER.to_owned(),
        };
        let from = next_decl.map_or_else(|| format!("file:{}", acc.file_path), str::to_owned);
        let id = format!("call:{}:{}:{}", caller_name, callee, acc.calls.len());
        acc.calls.push(GraphCall {
            id: id.clone(),
            caller: caller_name,
            caller_id: next_decl.map(str::to_owned),
            callee: callee.to_owned(),
            line,
            range,
            kind: "calls",
            receiver_type: None,
        });
        acc.edges.push(GraphEdge {
            id: format!("{from}->{callee}:calls:{line}:{}", acc.edges.len()),
            from,
            to: format!(
                "reference:{}@{}:{}",
                acc.file_path,
                node.start_byte(),
                callee
            ),
            relation: "calls",
            source: "ast",
            line,
            resolution: "unresolved",
        });
    }

    next_decl_identity
}

/// C# `using A.B;`, `using static A.B;`, `global using A.B;` and the alias
/// form `using X = A.B;`: the specifier is the namespace/type path as written,
/// the alias (when present) is the local name.
fn collect_csharp_using(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
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
    acc.imports.push(GraphImport {
        id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
        specifier,
        line,
        import_kind: "value",
        local_name,
        imported_name: None,
        imported_range: None,
        local_range: alias.map(|alias| line_index.range(alias)),
        resolution_hint: None,
        module_scope: None,
    });
}

fn push_language_import<'a>(
    acc: &'a mut GraphAccumulator,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    local_name: Option<String>,
    imported_name: Option<String>,
    hint: &'static str,
) -> &'a mut GraphImport {
    let index = acc.imports.len();
    acc.imports.push(GraphImport {
        id: format!("import:{}:{}", line, acc.imports.len()),
        specifier,
        line,
        import_kind,
        local_name,
        imported_name,
        imported_range: None,
        local_range: None,
        resolution_hint: Some(hint),
        module_scope: None,
    });
    &mut acc.imports[index]
}

fn language_label(ext: &str, language_id: Option<&str>) -> String {
    language_id
        .unwrap_or_else(|| canonical_extension(ext))
        .to_owned()
}

fn canonical_extension(ext: &str) -> &str {
    languages::find_entry(ext).map_or(ext, |entry| entry.extensions[0])
}

fn fact_families_for_extension(ext: &str) -> Vec<&'static str> {
    if canonical_extension(ext) == "asm" {
        // The generic Assembly grammar provides reliable labels but no lexical
        // function scopes or dialect-neutral call semantics.
        return vec!["declarations"];
    }
    let mut families = vec!["declarations", "contains", "calls"];
    match canonical_extension(ext) {
        // JS/TS (oxc lane) already emit import/export facts — advertise them so
        // `getGraphFactCapabilities` matches what `extractGraphFacts` returns.
        "ts" | "tsx" | "js" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "cu" | "scala" | "cs" => {
            families.push("imports");
            families.push("exports");
        }
        _ => {}
    }
    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Reference count of the first declaration named `name`.
    fn reference_count(source: &str, path: &str, name: &str) -> u32 {
        let extraction = extract_graph_facts_with_metadata(source, path).expect("graph facts");
        let id = &extraction
            .facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("declaration {name}"))
            .id;
        extraction
            .reference_counts
            .iter()
            .find(|count| &count.declaration_id == id)
            .expect("counted")
            .count
    }

    /// `used_in` of the Rust `use` binding `local` in `source`.
    fn rust_import_users(source: &str, local: &str) -> Option<Vec<String>> {
        let facts = extract_graph_facts_with_metadata(source, "src/app.rs")
            .expect("graph facts")
            .facts;
        let short = |id: &String| id.split(['#', '@']).nth(1).unwrap_or(id).to_owned();
        facts
            .imports
            .into_iter()
            .find(|import| import.local_name.as_deref() == Some(local))
            .expect("import")
            .used_in
            .map(|users| users.iter().map(short).collect())
    }

    #[test]
    fn rust_use_bindings_record_their_enclosing_declarations() {
        let source = "use crate::util::{run, Shape, Tr, other as alias};\npub use crate::util::exported;\nfn live() { run(); let _: Shape; println!(\"{alias}\"); }\nfn dead() { run(); }\nstatic S: fn() = run;\nimpl Fmt for X { fn go() { alias(); } }\n#[test]\nfn t() { Shape; }\n";
        assert_eq!(
            rust_import_users(source, "run"),
            Some(vec!["S".into(), "dead".into(), "live".into()])
        );
        // Test items count as module-level code.
        assert_eq!(
            rust_import_users(source, "Shape"),
            Some(vec!["live".into(), "module".into()])
        );
        // The alias is named by an inline format capture and a trait impl.
        assert_eq!(
            rust_import_users(source, "alias"),
            Some(vec!["live".into(), "module".into()])
        );
        // Never named (a trait in scope for its methods) or a `pub use`
        // re-export: unknown.
        assert_eq!(rust_import_users(source, "Tr"), None);
        assert_eq!(rust_import_users(source, "exported"), None);
    }

    #[test]
    fn rust_comments_and_strings_are_not_references() {
        let source = "/// `helper` is documented; see helper.\npub fn helper() {}\n// helper\n/* helper */\npub fn caller() -> &'static str { \"helper\" }\n";
        assert_eq!(reference_count(source, "lib.rs", "helper"), 0);
    }

    #[test]
    fn rust_value_uses_and_format_captures_count_but_calls_do_not() {
        let source = "pub fn callback() {}\npub fn called() {}\npub const LIMIT: u32 = 1;\npub struct Svc;\nimpl Svc { pub fn run(&self) {} pub fn go(&self) { self.run(); called(); register(callback); println!(\"{LIMIT} {{LIMIT}}\"); } }\n";
        assert_eq!(reference_count(source, "lib.rs", "callback"), 1);
        assert_eq!(reference_count(source, "lib.rs", "called"), 0);
        assert_eq!(reference_count(source, "lib.rs", "run"), 0);
        assert_eq!(reference_count(source, "lib.rs", "LIMIT"), 1);
        assert_eq!(
            reference_count(source, "lib.rs", "Svc"),
            0,
            "the impl names the struct as a declaration, not a use"
        );
    }

    #[test]
    fn inline_format_captures_skip_escapes_and_positional_arguments() {
        assert_eq!(
            inline_format_captures("\"{a} {b:?} {{c}} {0} {} {_d:>4}\""),
            ["a", "b", "_d"]
        );
    }

    #[test]
    fn rust_import_ranges_do_not_invent_synthetic_name_tokens() {
        let value = facts(
            "use crate::origin::{self as module_alias, *};\nuse crate::origin::plain;\n",
            "imports.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(imports[0].get("importedRange").is_none());
        assert!(imports[0].get("localRange").is_some());
        assert!(imports[1].get("importedRange").is_none());
        assert!(imports[1].get("localRange").is_none());
        assert_eq!(imports[2]["localRange"], imports[2]["importedRange"]);
        assert!(imports[2].get("importedRange").is_some());
    }

    #[test]
    fn rust_import_binding_ranges_are_exact_utf16() {
        let value = facts(
            "/*😀*/ use crate::origin::{target as first, target as second};\nuse crate::origin::{\n target as third,\n};\nuse crate::origin::target as fourth;\n",
            "aliases.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 4);
        for (index, (line, imported, local, length)) in [
            (0, 27, 37, 5),
            (0, 44, 54, 6),
            (2, 1, 11, 5),
            (4, 19, 29, 6),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                imports[index]["importedRange"],
                serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
            );
            assert_eq!(
                imports[index]["localRange"],
                serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
            );
        }
    }

    #[test]
    fn python_import_ranges_do_not_invent_synthetic_name_tokens() {
        let value = facts(
            "import package.sub as alias\nfrom origin import plain\nfrom origin import *\n",
            "imports.py",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(imports[0].get("importedRange").is_none());
        assert_eq!(
            imports[0]["localRange"],
            serde_json::json!({"start":{"line":0,"character":22},"end":{"line":0,"character":27}})
        );
        assert_eq!(imports[1]["localRange"], imports[1]["importedRange"]);
        assert!(imports[2].get("importedRange").is_none());
        assert!(imports[2].get("localRange").is_none());
    }

    #[test]
    fn python_import_binding_ranges_are_exact_utf16() {
        let value = facts(
            "marker = \"😀\"; from origin import target as first, target as second\nfrom origin import (\n    target as third,\n)\n",
            "aliases.py",
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 3);
        for (index, (line, imported, local, length)) in
            [(0, 34, 44, 5), (0, 51, 61, 6), (2, 4, 14, 5)]
                .into_iter()
                .enumerate()
        {
            assert_eq!(
                imports[index]["importedRange"],
                serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
            );
            assert_eq!(
                imports[index]["localRange"],
                serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
            );
        }
    }

    #[test]
    fn rust_cfg_and_path_attributes_survive_comments_and_inner_attributes() {
        let value = facts(
            "#[cfg(feature = \"x\")]\n// note\nmod child;\n#[path = \"actual.rs\"]\n/// documented\nmod alias;\nmod gated { #![cfg(feature = \"x\")] use super::Thing; }",
            "src/lib.rs",
        );
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "child" && module["unsupported"] != true)
        );
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "alias" && module["path"] == "actual.rs")
        );
        // `cfg` gates compilation, not the module's file: the edge stays.
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "gated" && module["unsupported"] != true)
        );
        let root = facts("#![cfg(feature = \"x\")]\nmod child;", "src/lib.rs");
        assert_ne!(root["rustRootUnsupported"], true);
        // A conditional attribute that rewrites the path is still unknown.
        let rewritten = facts(
            "#[cfg_attr(unix, path = \"u.rs\")] mod child;",
            "src/lib.rs",
        );
        assert!(
            rewritten["modules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|module| module["unsupported"] == true)
        );
        let local = facts(
            "fn f() { mod hidden { #[path = \"child.rs\"] mod child; } }",
            "src/lib.rs",
        );
        assert!(
            local["modules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|module| module["unsupported"] == true)
        );
    }

    #[test]
    fn rust_modules_preserve_literal_paths_inline_scopes_and_unknown_cfg() {
        let value = facts(
            "#[path = \"actual.rs\"] mod alias;\nmod inside { use super::Thing; #[path = \"nested.rs\"] mod child; }\n#[cfg(feature = \"x\")] mod conditional;",
            "src/lib.rs",
        );
        let modules = value["modules"].as_array().unwrap();
        assert!(modules.iter().any(|module| module["name"] == "alias"
            && module["path"] == "actual.rs"
            && module["unsupported"] != true));
        assert!(modules.iter().any(|module| module["name"] == "child"
            && module["scope"] == serde_json::json!(["inside"])
            && module["path"] == "nested.rs"));
        assert!(
            modules
                .iter()
                .any(|module| module["name"] == "conditional" && module["unsupported"] != true)
        );
        assert!(
            value["imports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|import| import["specifier"] == "super::Thing"
                    && import["moduleScope"] == serde_json::json!(["inside"])
                    && import["resolutionHint"].is_null())
        );
    }

    #[test]
    fn declaration_occurrences_do_not_alias_equal_names_or_impl_blocks() {
        for (source, path) in [
            (
                "struct A; impl A { fn run() { work(); } } struct B; impl B { fn run() { work(); } }",
                "names.rs",
            ),
            (
                "class A:\n def run(self): work()\nclass B:\n def run(self): work()\n",
                "names.py",
            ),
        ] {
            let value = facts(source, path);
            let declarations = value["declarations"].as_array().unwrap();
            let ids: std::collections::HashSet<_> = declarations
                .iter()
                .map(|d| d["id"].as_str().unwrap())
                .collect();
            assert_eq!(ids.len(), declarations.len());
            for edge in value["edges"].as_array().unwrap() {
                if edge["relation"] == "calls" {
                    assert!(ids.contains(edge["from"].as_str().unwrap()));
                    assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
                    assert_eq!(edge["resolution"], "unresolved");
                }
            }
        }
    }

    #[test]
    fn rust_use_trees_expand_multiline_groups_aliases_and_modules() {
        let value = facts(
            "mod child;\nuse super::{\n types::{Thing as Alias, Other},\n language::AgLanguage,\n};\n",
            "src/structural/files.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        for expected in [
            "self::child",
            "super::types::Thing",
            "super::types::Other",
            "super::language::AgLanguage",
        ] {
            assert!(
                imports.iter().any(|i| i["specifier"] == expected),
                "missing {expected}: {imports:?}"
            );
        }
        assert!(
            imports
                .iter()
                .any(|i| i["localName"] == "Alias" && i["importedName"] == "Thing")
        );
    }

    #[test]
    fn use_heavy_rust_file_keeps_its_imports_inside_the_deadline() {
        // Per-`use` `parent()`/sibling walks made this O(n²) and the
        // walk hit the deadline, discarding every import.
        let mut source = String::from("mod item0 { pub struct Name; }\n");
        for index in 0..6_000 {
            source.push_str(&format!("use crate::item{index}::Name;\n"));
        }
        let started = std::time::Instant::now();
        let value = facts(&source, "src/lib.rs");
        let elapsed = started.elapsed();
        let diagnostics = value["diagnostics"].as_array().unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.as_str().unwrap().contains("deadlineExceeded")),
            "{diagnostics:?}"
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 6_000);
        assert_eq!(imports[5_999]["specifier"], "crate::item5999::Name");
        assert_eq!(imports[0]["moduleScope"], serde_json::json!([]));
        assert!(imports.iter().all(|i| i.get("resolutionHint").is_none()));
        assert!(
            elapsed < super::super::extractor::AST_EXECUTION_TIMEOUT / 2,
            "took {elapsed:?}"
        );
    }

    #[test]
    fn rust_item_macro_bodies_are_read_as_items_and_expression_macros_are_not_gaps() {
        // tokio-style `cfg_rt! { ... }` wrappers hold real mod/use items.
        let value = facts(
            "cfg_rt! {\n    pub mod runtime;\n    pub use crate::runtime::Handle;\n}\nmod outer { cfg_net! { mod tcp; } }\nfn f() { println!(\"{}\", 1); let v = vec![1]; }\n",
            "src/lib.rs",
        );
        let modules = value["modules"].as_array().unwrap();
        assert!(
            modules
                .iter()
                .any(|m| m["name"] == "runtime" && m["line"] == 2),
            "{modules:?}"
        );
        assert!(
            modules
                .iter()
                .any(|m| m["name"] == "tcp" && m["scope"] == serde_json::json!(["outer"])),
            "{modules:?}"
        );
        assert!(
            value["imports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["specifier"] == "crate::runtime::Handle")
        );
        assert!(
            value["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| !d.as_str().unwrap().contains("macro expansion")),
            "{}",
            value["diagnostics"]
        );
    }

    #[test]
    fn rust_nonconventional_modules_and_macros_remain_explicitly_unsupported() {
        let value = facts(
            "#[cfg_attr(unix, path = \"other.rs\")] mod child;\nmod inline { use super::Thing; }\nmake_imports!();",
            "src/lib.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(
            imports.iter().any(|item| item["specifier"] == "self::child"
                && item["resolutionHint"] == "unsupported")
        );
        assert!(
            imports
                .iter()
                .any(|item| item["specifier"] == "super::Thing"
                    && item["moduleScope"] == serde_json::json!(["inline"]))
        );
        assert!(
            value["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("macro expansion"))
        );
    }

    #[cfg(feature = "tree-sitter-cpp")]
    #[test]
    fn cpp_class_owns_its_method_declarations() {
        let value = facts(
            "class Fixture { public: int target(int value) { return value; } };",
            "fixture.cpp",
        );
        let declarations = value["declarations"].as_array().unwrap();
        let class = declarations
            .iter()
            .find(|item| item["name"] == "Fixture")
            .expect("class declaration");
        let method = declarations
            .iter()
            .find(|item| item["name"] == "target")
            .expect("method declaration");
        assert_eq!(class["kind"], "class");
        assert_eq!(method["parent"], class["id"]);
        assert!(
            value["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|edge| edge["relation"] == "contains"
                    && edge["from"] == class["id"]
                    && edge["to"] == method["id"])
        );
    }

    fn facts(src: &str, path: &str) -> Value {
        let raw = extract_graph_facts(src, path).expect("graph facts expected");
        serde_json::from_str(&raw).expect("valid graph json")
    }

    #[test]
    fn expired_graph_budget_reports_incomplete_rust_root() {
        let raw = extract_graph_facts_before(
            "mod child; fn main() { work(); }",
            "lib.rs",
            std::time::Instant::now(),
        )
        .unwrap();
        let graph: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(graph["rustRootUnsupported"], true);
        assert_eq!(graph["imports"], serde_json::json!([]));
        assert!(
            graph["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d.as_str().unwrap().contains("graph.parse.deadlineExceeded"))
        );
    }

    #[test]
    fn expired_graph_walk_does_not_emit_complete_facts() {
        let source = "fn main() { work(); }";
        let language = tree_sitter_rust::LANGUAGE.into();
        let tree = super::super::extractor::parse_before(
            source,
            &language,
            std::time::Instant::now() + super::super::extractor::AST_EXECUTION_TIMEOUT,
        )
        .unwrap();
        let index = LineIndex::new(source);
        let mut acc = GraphAccumulator::new("main.rs", "rs");
        assert!(!visit_node(
            tree.root_node(),
            source,
            &index,
            &mut acc,
            std::time::Instant::now(),
            &[]
        ));
        assert!(acc.declarations.is_empty());
        assert!(acc.calls.is_empty());
    }

    #[test]
    fn deeply_nested_rust_use_groups_do_not_recurse_on_the_native_stack() {
        let source = format!("use {}std{};", "{".repeat(10_000), "}".repeat(10_000));
        let graph = facts(&source, "imports.rs");
        assert_eq!(graph["imports"].as_array().unwrap().len(), 1);
        assert_eq!(graph["imports"][0]["specifier"], "std");
        assert!(
            !graph["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d.as_str().unwrap().contains("parse errors"))
        );
    }

    /// `depth` nested blocks, each calling `s.save()` on a typed local.
    fn deeply_nested_rust_receivers(depth: usize) -> String {
        let mut source = String::from("fn main() { let s: Store = Store::new(); ");
        for _ in 0..depth {
            source.push_str("{ s.save(); ");
        }
        source.push_str(&"} ".repeat(depth));
        source.push('}');
        source
    }

    #[test]
    fn deeply_nested_receiver_lookups_stay_inside_the_deadline() {
        // A receiver lookup climbed `Node::parent()` (a root-down search per
        // hop) from every member call to its function: O(depth²) per call.
        let source = deeply_nested_rust_receivers(1_500);
        let started = std::time::Instant::now();
        let graph = facts(&source, "deep_receivers.rs");
        let elapsed = started.elapsed();
        let diagnostics = graph["diagnostics"].as_array().unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.as_str().unwrap().contains("deadlineExceeded")),
            "{diagnostics:?}"
        );
        let saves = graph["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["callee"] == "s.save")
            .collect::<Vec<_>>();
        assert_eq!(saves.len(), 1_500);
        assert!(saves.iter().all(|call| call["receiverType"] == "Store"));
        assert!(
            elapsed < super::super::extractor::AST_EXECUTION_TIMEOUT / 2,
            "took {elapsed:?}"
        );
    }

    #[test]
    fn receiver_facts_are_unchanged_on_an_ordinary_file() {
        // Pinned output of the receiver lane on a normal file: locals,
        // shadowing, nested fns, `self.field`, and a constructor binding.
        let source = "struct Store { cache: Cache }\nimpl Store {\n    fn run(&self, db: Db) {\n        self.cache.get();\n        db.query();\n        let w = Writer::new();\n        w.flush();\n        {\n            let db = make();\n            db.query();\n        }\n        db.close();\n        fn inner() { w.flush(); }\n    }\n}\n";
        let graph = facts(source, "store.rs");
        let receivers = graph["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| {
                format!(
                    "{}@{}={}",
                    call["callee"].as_str().unwrap(),
                    call["line"],
                    call.get("receiverType")
                        .and_then(Value::as_str)
                        .unwrap_or("-")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            receivers,
            vec![
                "self.cache.get@4=Cache",
                "db.query@5=Db",
                "Writer::new@6=-",
                "w.flush@7=Writer",
                "make@9=-",
                "db.query@10=-",
                "db.close@12=Db",
                "w.flush@13=-",
            ]
        );
    }

    #[test]
    fn deeply_nested_graph_traversal_preserves_calls() {
        let source = format!(
            "fn main() {{ let x = {}probe(){}; after(); }}",
            "[".repeat(10_000),
            "]".repeat(10_000)
        );
        let graph = facts(&source, "deep.rs");
        assert_eq!(
            graph["diagnostics"],
            serde_json::json!([
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof"
            ])
        );
        let calls = graph["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["caller"], "main");
        assert_eq!(calls[0]["callee"], "probe");
        assert_eq!(calls[1]["caller"], "main");
        assert_eq!(calls[1]["callee"], "after");
    }

    #[test]
    fn graph_traversal_restores_declaration_context_after_nested_scopes() {
        let graph = facts(
            "fn outer() { fn inner() { inside(); } after(); } fn sibling() { next(); }",
            "scopes.rs",
        );
        let calls: Vec<_> = graph["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| {
                (
                    call["caller"].as_str().unwrap(),
                    call["callee"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            calls,
            [("inner", "inside"), ("outer", "after"), ("sibling", "next")]
        );
        let declarations = graph["declarations"].as_array().unwrap();
        assert_eq!(declarations[0]["name"], "outer");
        assert_eq!(declarations[1]["name"], "inner");
        assert_eq!(declarations[1]["parent"], declarations[0]["id"]);
        assert_eq!(declarations[2]["name"], "sibling");
        assert!(declarations[2]["parent"].is_null());
    }

    #[test]
    fn rust_graph_facts_include_pub_declarations_and_calls() {
        let src = r#"
use crate::other::helper;

pub struct Point {
    x: f64,
}

pub fn distance(point: Point) -> f64 {
    helper(point.x)
}
"#;
        let graph = facts(src, "geo.rs");
        assert_eq!(graph["schemaVersion"], 1);
        assert!(
            graph
                .get("language")
                .is_some_and(|language| language == "rust")
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls
                    .iter()
                    .any(|decl| decl.get("name").is_some_and(|name| name == "Point")))
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls.iter().any(|decl| decl
                    .get("name")
                    .is_some_and(|name| name == "distance")
                    && decl
                        .get("exported")
                        .is_some_and(|exported| exported == true)))
        );
        assert!(
            graph
                .get("imports")
                .and_then(Value::as_array)
                .is_some_and(|imports| imports.iter().any(|import| import
                    .get("specifier")
                    .and_then(Value::as_str)
                    .is_some_and(|specifier| specifier.contains("crate::other"))))
        );
        assert!(
            graph
                .get("calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| calls
                    .iter()
                    .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
        );
    }

    #[test]
    fn python_import_facts_preserve_module_paths_and_alias_bindings() {
        let graph = facts(
            "import os, pkg.worker as worker\nfrom .target import run as execute\nfrom . import sibling\n",
            "pkg/service.py",
        );
        let imports = graph["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 4);
        assert_eq!(imports[0]["specifier"], "os");
        assert_eq!(imports[1]["specifier"], "pkg.worker");
        assert_eq!(imports[1]["localName"], "worker");
        assert_eq!(imports[1]["resolutionHint"], "python-absolute");
        assert_eq!(imports[2]["specifier"], ".target");
        assert_eq!(imports[2]["importedName"], "run");
        assert_eq!(imports[2]["localName"], "execute");
        assert_eq!(imports[2]["resolutionHint"], "python-relative");
        assert_eq!(imports[3]["specifier"], ".");
        assert_eq!(imports[3]["importedName"], "sibling");
    }

    #[test]
    fn c_import_facts_distinguish_quoted_system_and_computed_headers() {
        let graph = facts(
            "#include \"local.h\"\n#include <system.h>\n#include HEADER\n",
            "entry.c",
        );
        let imports = graph["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0]["specifier"], "local.h");
        assert_eq!(imports[0]["resolutionHint"], "c-relative");
        assert_eq!(imports[1]["resolutionHint"], "c-system");
        assert_eq!(imports[2]["resolutionHint"], "unsupported");
    }

    #[test]
    fn python_graph_facts_include_module_public_defs() {
        let src = r#"
import os

class Service:
    def run(self):
        helper()

def helper():
    return os.getcwd()
"#;
        let graph = facts(src, "service.py");
        assert!(
            graph
                .get("language")
                .is_some_and(|language| language == "python")
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls
                    .iter()
                    .any(|decl| decl.get("name").is_some_and(|name| name == "Service")))
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls.iter().any(|decl| decl
                    .get("name")
                    .is_some_and(|name| name == "helper")
                    && decl
                        .get("exported")
                        .is_some_and(|exported| exported == true)))
        );
        assert!(
            graph
                .get("imports")
                .and_then(Value::as_array)
                .is_some_and(|imports| imports.iter().any(|import| import
                    .get("specifier")
                    .and_then(Value::as_str)
                    .is_some_and(|specifier| specifier.contains("os"))))
        );
        assert!(
            graph
                .get("calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| calls
                    .iter()
                    .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
        );
    }

    fn facts_json(source: &str, path: &str) -> Value {
        serde_json::from_str(&extract_graph_facts(source, path).expect("graph facts"))
            .expect("facts JSON")
    }

    fn declared_names(source: &str, path: &str) -> Vec<String> {
        facts_json(source, path)["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .filter_map(|declaration| declaration["name"].as_str().map(str::to_owned))
            .collect()
    }

    #[test]
    fn keyword_named_declarations_survive_outside_the_js_family() {
        let rust = declared_names(
            "struct Foo;\nimpl Foo {\n    pub fn new() -> Self { Foo }\n    pub fn build() -> Self { Foo }\n}\n",
            "a.rs",
        );
        for name in ["new", "build"] {
            assert!(
                rust.iter().any(|found| found == name),
                "rust {name}: {rust:?}"
            );
        }
        let python = declared_names(
            "def new():\n    pass\n\nclass K:\n    def case(self):\n        pass\n",
            "a.py",
        );
        for name in ["new", "case"] {
            assert!(
                python.iter().any(|found| found == name),
                "python {name}: {python:?}"
            );
        }
    }

    #[test]
    fn keyword_named_class_members_stay_legal_in_clean_js() {
        let js = declared_names("class A {\n  if() {}\n  build() {}\n}\n", "a.js");
        assert!(js.iter().any(|found| found == "build"), "{js:?}");
        assert!(js.iter().any(|found| found == "if"), "{js:?}");
    }

    /// `(relation, from declaration name, to, line)` for every heritage edge.
    fn heritage(source: &str, path: &str) -> Vec<(String, String, String, u64)> {
        let facts = facts_json(source, path);
        let declarations = facts["declarations"].as_array().expect("declarations");
        facts["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter(|edge| matches!(edge["relation"].as_str(), Some("extends" | "implements")))
            .map(|edge| {
                assert_eq!(edge["source"], "tree-sitter");
                assert_eq!(edge["resolution"], "syntax");
                let from = declarations
                    .iter()
                    .find(|declaration| declaration["id"] == edge["from"])
                    .unwrap_or_else(|| panic!("edge from is a declaration id: {edge}"));
                (
                    edge["relation"].as_str().unwrap_or_default().to_owned(),
                    from["name"].as_str().unwrap_or_default().to_owned(),
                    edge["to"].as_str().unwrap_or_default().to_owned(),
                    edge["line"].as_u64().unwrap_or_default(),
                )
            })
            .collect()
    }

    fn edge(relation: &str, from: &str, to: &str, line: u64) -> (String, String, String, u64) {
        (relation.to_owned(), from.to_owned(), to.to_owned(), line)
    }

    #[test]
    fn rust_heritage_links_impl_trait_and_supertraits() {
        let source = "trait Shape: Clone + std::fmt::Debug + 'static {}\nstruct Square;\nimpl std::fmt::Display for Square {}\nimpl<T> From<T> for Square {}\nimpl Square {}\n";
        assert_eq!(
            heritage(source, "lib.rs"),
            vec![
                edge("extends", "Shape", "Clone", 1),
                edge("extends", "Shape", "std::fmt::Debug", 1),
                edge("implements", "Square", "std::fmt::Display", 3),
                edge("implements", "Square", "From", 4),
            ]
        );
        let facts = facts_json(source, "lib.rs");
        let impl_ids: Vec<&Value> = facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .filter(|declaration| declaration["kind"] == "impl")
            .map(|declaration| &declaration["id"])
            .collect();
        let froms: Vec<&Value> = facts["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter(|edge| edge["relation"] == "implements")
            .map(|edge| &edge["from"])
            .collect();
        assert_eq!(
            froms,
            impl_ids[..2].to_vec(),
            "from is the impl declaration id"
        );
    }

    #[test]
    fn python_heritage_skips_keywords_splats_and_object() {
        let source = "class A(Base, pkg.Mixin, Generic[T], metaclass=Meta):\n    pass\nclass B(object):\n    pass\nclass C(*bases, **kw):\n    pass\n";
        assert_eq!(
            heritage(source, "m.py"),
            vec![
                edge("extends", "A", "Base", 1),
                edge("extends", "A", "pkg.Mixin", 1),
                edge("extends", "A", "Generic", 1),
            ]
        );
    }

    #[test]
    fn java_heritage_separates_extends_and_implements() {
        let source = "class A extends Base<String> implements Runnable, java.io.Serializable {}\ninterface I extends J, K<T> {}\nenum E implements I {}\nrecord R(int x) implements I {}\n";
        assert_eq!(
            heritage(source, "A.java"),
            vec![
                edge("extends", "A", "Base", 1),
                edge("implements", "A", "Runnable", 1),
                edge("implements", "A", "java.io.Serializable", 1),
                edge("extends", "I", "J", 2),
                edge("extends", "I", "K", 2),
                edge("implements", "E", "I", 3),
                edge("implements", "R", "I", 4),
            ]
        );
    }

    #[test]
    fn cpp_heritage_reads_the_base_class_clause() {
        let source = "class A : public Base, private ns::Mixin<int> {};\nstruct S : virtual Base {};\nclass Plain {};\n";
        assert_eq!(
            heritage(source, "a.cpp"),
            vec![
                edge("extends", "A", "Base", 1),
                edge("extends", "A", "ns::Mixin", 1),
                edge("extends", "S", "Base", 2),
            ]
        );
    }

    #[test]
    fn csharp_heritage_treats_the_first_class_base_as_extends() {
        let source = "class A : Base, IDisposable, IList<int> {}\ninterface I : J, K {}\nstruct S : IEquatable<S> {}\nrecord R(int X) : Base(X);\n";
        assert_eq!(
            heritage(source, "A.cs"),
            vec![
                edge("extends", "A", "Base", 1),
                edge("implements", "A", "IDisposable", 1),
                edge("implements", "A", "IList", 1),
                edge("extends", "I", "J", 2),
                edge("extends", "I", "K", 2),
                edge("implements", "S", "IEquatable", 3),
                edge("extends", "R", "Base", 4),
            ]
        );
    }

    #[test]
    fn heritage_edges_ingest_as_unresolved_targets() {
        let extraction =
            extract_graph_facts_with_metadata("class A(Base):\n    pass\n", "m.py").expect("facts");
        let mut builder = crate::graph::CodeGraphBuilder::new("/fixture", 1);
        builder
            .ingest_facts("m.py", "digest", &extraction.facts)
            .expect("ingest");
        let graph = builder.finish_without_digest();
        let extends = graph
            .edges
            .values()
            .find(|edge| edge.kind == crate::graph::EdgeKind::Syntactic("extends".to_owned()))
            .expect("extends edge");
        assert!(extends.from.0.starts_with("symbol:m.py#declaration:"));
        assert_eq!(extends.to.0, "occurrence:m.py#Base");
        assert_eq!(
            graph.nodes[&extends.to].kind,
            crate::graph::NodeKind::UnresolvedTarget
        );
    }

    #[test]
    fn module_level_calls_have_no_caller_id() {
        let source = "import app\n\n@app.route('/')\ndef index():\n    helper()\n\nsetup()\nif __name__ == '__main__':\n    index()\n";
        let facts = facts_json(source, "main.py");
        let calls: Vec<(String, String, bool)> = facts["calls"]
            .as_array()
            .expect("calls")
            .iter()
            .map(|call| {
                (
                    call["caller"].as_str().unwrap_or_default().to_owned(),
                    call["callee"].as_str().unwrap_or_default().to_owned(),
                    call.get("callerId").is_some(),
                )
            })
            .collect();
        assert_eq!(
            calls,
            vec![
                ("module".to_owned(), "app.route".to_owned(), false),
                ("index".to_owned(), "helper".to_owned(), true),
                ("module".to_owned(), "setup".to_owned(), false),
                ("module".to_owned(), "index".to_owned(), false),
            ]
        );
        let module_edge = facts["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .find(|edge| edge["relation"] == "calls" && edge["line"] == 7)
            .expect("module call edge");
        assert_eq!(module_edge["from"], "file:main.py");
        // A module-level call target is an edge, not a value reference.
        assert_eq!(reference_count(source, "main.py", "index"), 0);
    }

    #[test]
    fn module_level_calls_cover_go_rust_and_c() {
        let callers = |source: &str, path: &str| -> Vec<(String, String)> {
            facts_json(source, path)["calls"]
                .as_array()
                .expect("calls")
                .iter()
                .map(|call| {
                    (
                        call["caller"].as_str().unwrap_or_default().to_owned(),
                        call["callee"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        };
        assert!(
            callers("package p\n\nvar x = build()\n", "p.go")
                .contains(&("module".to_owned(), "build".to_owned()))
        );
        assert!(
            callers(
                "lazy_static! { static ref X: u8 = 1; }\nfn f() { g(); }\n",
                "lib.rs"
            )
            .contains(&("module".to_owned(), "lazy_static".to_owned()))
        );
        assert!(
            callers("int f(void);\nint x = f();\n", "a.cpp")
                .contains(&("module".to_owned(), "f".to_owned()))
        );
    }

    #[test]
    fn csharp_using_directives_are_imports() {
        let source = "using System.Text;\nusing static System.Math;\nglobal using System;\nusing Json = Newtonsoft.Json.JsonConvert;\nnamespace App { using Inner.Pkg; class A { void M() { using (var x = Open()) {} } } }\n";
        let facts = facts_json(source, "A.cs");
        let imports: Vec<(String, Option<String>, u64)> = facts["imports"]
            .as_array()
            .expect("imports")
            .iter()
            .map(|import| {
                (
                    import["specifier"].as_str().unwrap_or_default().to_owned(),
                    import["localName"].as_str().map(str::to_owned),
                    import["line"].as_u64().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            imports,
            vec![
                ("System.Text".to_owned(), None, 1),
                ("System.Math".to_owned(), None, 2),
                ("System".to_owned(), None, 3),
                (
                    "Newtonsoft.Json.JsonConvert".to_owned(),
                    Some("Json".to_owned()),
                    4
                ),
                ("Inner.Pkg".to_owned(), None, 5),
            ]
        );
    }

    #[test]
    fn graph_fact_capabilities_include_rust_and_python() {
        let json = graph_fact_capabilities_json();
        assert!(json.contains("\"extension\":\"rs\""));
        assert!(json.contains("\"extension\":\"py\""));
    }
}
