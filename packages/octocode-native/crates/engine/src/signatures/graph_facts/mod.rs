//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use crate::graph::{
    GraphFactCall, GraphFactCode, GraphFactDeclaration, GraphFactDiagnostic, GraphFactEdge,
    GraphFactExport, GraphFactImport, GraphFactRustModule, GraphFactsDocument,
};

use super::languages;

mod heritage;
mod python;
mod receiver;
mod rust;

mod collect;
mod imports;
mod positions;
mod references;
mod walk;

use rust::rust_inner_unsupported;

use imports::push_language_import;
use positions::{NodePositions, syntax_error_lines};
use references::count_name_references;
use walk::{visit_macro_bodies, visit_node};

/// The diagnostic of `code`, with the message this module states for it.
fn note(code: GraphFactCode) -> GraphFactDiagnostic {
    let message = match code {
        GraphFactCode::SyntaxOnly => {
            "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof"
        }
        GraphFactCode::RecoveredParse => {
            "tree-sitter recovered from parse errors; graph facts may be partial"
        }
        GraphFactCode::ParseDeadline => "graph parsing was interrupted; facts are incomplete",
        GraphFactCode::TraversalDeadline => {
            "graph extraction exceeded its execution deadline; facts are incomplete"
        }
        GraphFactCode::RustMacroExpansion => {
            "unsupported Rust macro expansion: macro-generated imports are not linked"
        }
        GraphFactCode::RustModuleAttributes => {
            "unsupported Rust conditional or custom module attributes"
        }
        GraphFactCode::RustCrateAttributes => {
            "unsupported Rust inner conditional or custom crate attributes"
        }
        // `js_oxc` states its parser's own errors.
        GraphFactCode::ParseError => "the parser reported a syntax error",
    };
    GraphFactDiagnostic {
        code,
        message: message.to_owned(),
        line: None,
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
    diagnostics: Vec<GraphFactDiagnostic>,
    modules: Vec<GraphFactRustModule>,
    /// Rust qualified paths outside call and `use` position.
    paths: Vec<crate::graph::GraphFactPath>,
    /// Start bytes of name tokens that are not value references: declaration
    /// names and the callee tokens of recorded calls (call edges).
    non_reference_tokens: std::collections::HashSet<usize>,
    /// Bodies of Rust item-level macro calls (`cfg_rt! { mod x; }`) with
    /// their enclosing module scope and declaration (an `impl` holding
    /// `cfg_io_util! { pub fn … }`), re-read as items after the main walk.
    macro_bodies: Vec<MacroBody>,
    /// Byte span of each entry of `declarations`, in the same order.
    declaration_spans: Vec<(usize, usize)>,
    /// Name tokens the main walk passed, for `count_name_references`;
    /// `None` outside that walk (macro bodies are not counted).
    name_tokens: Option<Vec<NameToken>>,
    /// Outline extraction: skip call facts, which an outline never reads and
    /// which are most of a large file's walk and facts.
    declarations_only: bool,
}

/// A token `count_name_references` reads: a name leaf, or (`format`) a Rust
/// string literal inside a macro token tree, whose inline captures name
/// bindings.
struct NameToken {
    start: usize,
    end: usize,
    format: bool,
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
            paths: Vec::new(),
            non_reference_tokens: std::collections::HashSet::new(),
            macro_bodies: Vec::new(),
            declaration_spans: Vec::new(),
            name_tokens: None,
            declarations_only: false,
            diagnostics: vec![note(GraphFactCode::SyntaxOnly)],
        }
    }
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

/// The graph facts without call facts, reference counts or import uses:
/// what outlines and enclosing names read, at a fraction of the walk.
pub(crate) fn extract_declaration_facts_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<GraphFactsDocument> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_facts_before(
            content,
            file_path,
            extension,
            std::time::Instant::now() + super::extractor::AST_EXECUTION_TIMEOUT,
            true,
        )
    }))
    .unwrap_or(None)
    .map(|extraction| extraction.facts)
}

fn extract_graph_facts_with_metadata_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
) -> Option<super::GraphFactsExtraction> {
    extract_facts_before(content, file_path, extension, deadline, false)
}

fn extract_facts_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
    declarations_only: bool,
) -> Option<super::GraphFactsExtraction> {
    let entry = graph_fact_entry(extension)?;
    let tree = super::extractor::parse_before(content, &entry.language, deadline);
    facts_from_tree(
        content,
        file_path,
        extension,
        deadline,
        declarations_only,
        tree.as_ref(),
    )
}

/// The registry entry of a grammar graph facts are extracted for: every
/// registry grammar ([`graph_fact_extensions`]).
fn graph_fact_entry(extension: &str) -> Option<&'static languages::LanguageEntry> {
    languages::find_entry(extension)
}

/// The declarations-only facts of `content` from `tree`, a parse of it in
/// the grammar of `extension` another reader already made (the symbols
/// outline): one parse serves both (`None`: the parse ran out of time, and
/// the facts say they are incomplete, as an own parse would). `None` when
/// graph facts have no such grammar.
pub(crate) fn declaration_facts_from_tree(
    content: &str,
    file_path: &str,
    extension: &str,
    tree: Option<&tree_sitter::Tree>,
    deadline: std::time::Instant,
) -> Option<GraphFactsDocument> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        facts_from_tree(content, file_path, extension, deadline, true, tree)
    }))
    .unwrap_or(None)
    .map(|extraction| extraction.facts)
}

/// Graph facts from `tree` (`None`: the parse ran out of time, and the
/// facts say they are incomplete).
fn facts_from_tree(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
    declarations_only: bool,
    tree: Option<&tree_sitter::Tree>,
) -> Option<super::GraphFactsExtraction> {
    let ext = extension.to_owned();
    let entry = graph_fact_entry(&ext)?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    acc.declarations_only = declarations_only;
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
    let mut reference_counts = Vec::new();
    let mut error_lines = Vec::new();
    if let Some(tree) = tree {
        let root = tree.root_node();
        let line_index = NodePositions::new(content);
        rust_root_unsupported = (ext == "rs").then(|| rust_inner_unsupported(root, content));
        if rust_root_unsupported == Some(true) {
            acc.diagnostics
                .push(note(GraphFactCode::RustCrateAttributes));
        }
        if root.has_error() {
            acc.diagnostics.push(note(GraphFactCode::RecoveredParse));
            error_lines = syntax_error_lines(root);
        }
        acc.name_tokens = (!declarations_only).then(Vec::new);
        let visited = visit_node(root, content, &line_index, &mut acc, deadline, &[], None);
        let name_tokens = acc.name_tokens.take().unwrap_or_default();
        if !visited
            || !visit_macro_bodies(content, &entry.language, &line_index, &mut acc, deadline)
        {
            // Facts gathered before the deadline are positive syntax facts and
            // stay. The diagnostic marks the file incomplete, so consumers must
            // not read a missing import, call or module as absent.
            acc.diagnostics.push(note(GraphFactCode::TraversalDeadline));
            rust_root_unsupported = (ext == "rs").then_some(true);
        } else if !declarations_only {
            let counts = count_name_references(content, &acc, &name_tokens);
            reference_counts = acc
                .declarations
                .iter()
                .map(|declaration| crate::types::GraphReferenceCount {
                    declaration_id: declaration.id.clone(),
                    count: counts.get(&declaration.name).copied().unwrap_or(0),
                })
                .collect();
        }
    } else {
        acc.diagnostics.push(note(GraphFactCode::ParseDeadline));
    }

    let lines = content.lines().collect::<Vec<_>>();
    for declaration in &mut acc.declarations {
        declaration.doc_line =
            super::leading_doc_line(&lines, declaration.range.start.line as usize, &ext);
    }
    let facts = GraphFactsDocument {
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        edges: acc.edges,
        diagnostics: acc.diagnostics,
        error_lines,
        modules: acc.modules,
        paths: {
            let mut paths = acc.paths;
            // An item-level macro body is read both as tokens and as items.
            paths.sort_unstable();
            paths.dedup();
            paths
        },
        rust_root_unsupported,
        ..super::native_graph_facts(language_label(&ext, entry.language_id), file_path)
    };
    Some(super::GraphFactsExtraction {
        facts,
        reference_counts,
    })
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

/// A Rust item-level macro body to re-read as items: its inner range, the
/// enclosing `mod` names (outermost first), and the enclosing declaration's
/// `(id, name)`, so the items it holds nest where the macro call sits.
type MacroBody = (tree_sitter::Range, Vec<String>, Option<(String, String)>);

fn language_label(ext: &str, language_id: Option<&str>) -> String {
    language_id
        .unwrap_or_else(|| canonical_extension(ext))
        .to_owned()
}

fn canonical_extension(ext: &str) -> &str {
    languages::find_entry(ext).map_or(ext, |entry| entry.extensions[0])
}

#[cfg(test)]
mod tests;
