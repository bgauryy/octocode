//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use tree_sitter::Node;

use crate::graph::{
    GraphFactCall, GraphFactDeclaration, GraphFactEdge, GraphFactExport, GraphFactImport,
    GraphFactRustModule, GraphFactsDocument, GraphPosition, GraphRange,
};

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

/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic.
struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn range(&self, node: Node<'_>) -> GraphRange {
        GraphRange {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
        }
    }

    fn position(&self, byte_offset: usize) -> GraphPosition {
        let (line, character) = self.0.byte_to_position(byte_offset as u32);
        GraphPosition { line, character }
    }
}

struct GraphAccumulator {
    file_path: String,
    ext: String,
    declarations: Vec<GraphFactDeclaration>,
    imports: Vec<GraphFactImport>,
    exports: Vec<GraphFactExport>,
    calls: Vec<GraphFactCall>,
    edges: Vec<GraphFactEdge>,
    diagnostics: Vec<String>,
    modules: Vec<GraphFactRustModule>,
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

/// Error spans past this many collapse to the whole file.
const MAX_ERROR_SPANS: usize = 64;

/// 1-based line spans of the `ERROR` and missing nodes of a recovered parse,
/// merged; past [`MAX_ERROR_SPANS`] one span covers every line.
fn syntax_error_lines(root: Node<'_>) -> Vec<[u32; 2]> {
    let mut spans = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let line = |row: usize| u32::try_from(row + 1).unwrap_or(u32::MAX);
            spans.push((
                line(node.start_position().row),
                line(node.end_position().row),
            ));
            continue;
        }
        if node.has_error() {
            stack.extend(node.children(&mut cursor));
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last[1].saturating_add(1) => last[1] = last[1].max(end),
            _ => merged.push([start, end]),
        }
    }
    if merged.len() > MAX_ERROR_SPANS {
        return vec![[1, u32::MAX]];
    }
    merged
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
    let mut error_lines = Vec::new();
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
            error_lines = syntax_error_lines(root);
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
    let mut facts = GraphFactsDocument {
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        edges: acc.edges,
        diagnostics: acc.diagnostics,
        error_lines,
        modules: acc.modules,
        rust_root_unsupported,
        ..super::native_graph_facts(language_label(&ext, entry.language_id), file_path)
    };
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
    let mut exts: Vec<String> = languages::supported_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    exts.sort();
    exts.dedup();
    exts
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

/// A function declared directly in a type body (class, struct, trait,
/// interface, impl, enum) is a `method`, as the JS/TS extractor labels it;
/// Go/Java/C# method syntax always is. The enclosing declaration's kind is
/// the suffix of its id (`declaration:…@byte:kind`).
fn method_kind(kind: &'static str, node: Node<'_>, active_decl: Option<&str>) -> &'static str {
    if kind != "function" {
        return kind;
    }
    if matches!(
        node.kind(),
        "method_declaration" | "method_definition" | "singleton_method"
    ) {
        return "method";
    }
    let in_type = active_decl
        .and_then(|id| id.rsplit_once(':'))
        .is_some_and(|(_, parent)| {
            matches!(
                parent,
                "class" | "struct" | "trait" | "interface" | "impl" | "enum"
            )
        });
    if in_type { "method" } else { kind }
}

/// Emits declaration/edge facts for `node`.
///
/// Note the intentional coordinate-basis split on every emitted `GraphFactDeclaration`:
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
        let kind = method_kind(kind, node, active_decl);
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
                GraphFactDeclaration {
                    id,
                    name,
                    kind: kind.to_owned(),
                    line,
                    range,
                    selection_range: line_index.range(name_token),
                    exported,
                    parent,
                    doc_line: None,
                    exported_as: Vec::new(),
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
                    used_in: None,
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
        acc.imports.push(GraphFactImport {
            id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
            specifier,
            line,
            import_kind: "value".to_owned(),
            local_name: None,
            imported_name: None,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
            used_in: None,
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
        acc.calls.push(GraphFactCall {
            id: id.clone(),
            caller: caller_name,
            caller_id: next_decl.map(str::to_owned),
            callee: callee.to_owned(),
            line,
            range,
            kind: "calls".to_owned(),
            receiver_type: None,
        });
        acc.edges.push(GraphFactEdge {
            id: format!("{from}->{callee}:calls:{line}:{}", acc.edges.len()),
            from,
            to: format!(
                "reference:{}@{}:{}",
                acc.file_path,
                node.start_byte(),
                callee
            ),
            relation: "calls".to_owned(),
            source: "ast".to_owned(),
            line,
            resolution: "unresolved".to_owned(),
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
        used_in: None,
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
        used_in: None,
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
