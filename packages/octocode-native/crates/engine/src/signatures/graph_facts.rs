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
mod rust_cfg;

mod arguments;
mod collect;
mod imports;
mod positions;
mod references;
mod test_adapter;
mod validity;
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
        GraphFactCode::UnsupportedExportForwarding => {
            "export-forwarding clause observed; its member binding is not assessed"
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
    argument_sites: Vec<crate::graph::GraphFactArgumentSite>,
    call_adapter: Option<super::calls::CallAdapter>,
    visibility_adapter: super::visibility::VisibilityAdapter,
    export_forwarding_node: Option<&'static str>,
    argument_adapter: Option<super::arguments::ArgumentAdapter>,
    argument_bindings: Option<super::arguments::binding::Bindings>,
    source_digest: String,
    source_templates: crate::text::analysis::SourceTemplates,
    edges: Vec<GraphFactEdge>,
    diagnostics: Vec<GraphFactDiagnostic>,
    modules: Vec<GraphFactRustModule>,
    /// Rust qualified paths outside call and `use` position.
    paths: Vec<crate::graph::GraphFactPath>,
    /// Line spans of items compiled only under test, outermost only.
    test_lines: Vec<[u32; 2]>,
    /// Start bytes of name tokens that are not value references: declaration
    /// names and the callee tokens of recorded calls (call edges).
    non_reference_tokens: std::collections::HashSet<usize>,
    /// Named namespace occurrences whose public scope is transparent to children.
    public_namespace_scopes: std::collections::HashSet<String>,
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
    test_context: test_adapter::TestContext,
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
            argument_sites: Vec::new(),
            call_adapter: None,
            visibility_adapter: super::visibility::VisibilityAdapter::TopLevel,
            export_forwarding_node: None,
            argument_adapter: None,
            argument_bindings: None,
            source_digest: String::new(),
            source_templates: Default::default(),
            edges: Vec::new(),
            modules: Vec::new(),
            paths: Vec::new(),
            test_lines: Vec::new(),
            non_reference_tokens: std::collections::HashSet::new(),
            public_namespace_scopes: std::collections::HashSet::new(),
            macro_bodies: Vec::new(),
            declaration_spans: Vec::new(),
            name_tokens: None,
            declarations_only: false,
            test_context: test_adapter::TestContext::default(),
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

fn extract_graph_facts_with_metadata_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
) -> Option<super::GraphFactsExtraction> {
    extract_facts_before(content, file_path, extension, deadline, false)
}

pub(super) fn extract_facts_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
    declarations_only: bool,
) -> Option<super::GraphFactsExtraction> {
    let entry = languages::find_entry(extension)?.admitted(if declarations_only {
        crate::text::analysis::AnalysisOperation::Declarations
    } else {
        crate::text::analysis::AnalysisOperation::GraphFacts
    })?;
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
    // One spelling per language for every dispatch below: `pyi` is `py`,
    // `hpp` is `cpp`, as the language registry lists them.
    let ext = canonical_extension(extension).to_owned();
    let entry = languages::find_entry(&ext)?.admitted(if declarations_only {
        crate::text::analysis::AnalysisOperation::Declarations
    } else {
        crate::text::analysis::AnalysisOperation::GraphFacts
    })?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    acc.declarations_only = declarations_only;
    acc.argument_adapter = entry.argument_adapter;
    acc.call_adapter = entry.call_adapter;
    acc.visibility_adapter = entry.visibility_adapter;
    acc.export_forwarding_node = entry.export_forwarding_node;
    acc.source_digest = crate::digest::sha256(content.as_bytes());
    acc.source_templates = crate::text::analysis::SourceTemplates::new(
        content,
        crate::text::analysis::Producer::new(format!("tree-sitter-facts:{ext}"), "6"),
        if declarations_only {
            crate::text::analysis::AnalysisOperation::Declarations
        } else {
            crate::text::analysis::AnalysisOperation::GraphFacts
        },
    )
    .ok()?;
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
    let mut reference_counts = Vec::new();
    let mut error_lines = Vec::new();
    if let Some(tree) = tree {
        let root = tree.root_node();
        let line_index = NodePositions::new(content);
        if !declarations_only && let Some(adapter) = entry.argument_adapter {
            acc.argument_bindings = Some(super::arguments::binding::Bindings::collect(
                root, adapter, content, deadline,
            ));
        }
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
    arguments::join_bindings(&mut acc);
    for declaration in &mut acc.declarations {
        declaration.doc_line =
            super::leading_doc_line(&lines, declaration.range.start.line as usize, &ext);
    }
    let facts = GraphFactsDocument {
        source_templates: acc.source_templates,
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        argument_sites: acc.argument_sites,
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
        test_lines: acc.test_lines,
        ..super::native_graph_facts(language_label(&ext, entry.language_id), file_path, content)
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

/// The first extension of the language `ext` belongs to (`pyi` → `py`).
fn canonical_extension(ext: &str) -> &str {
    languages::find_entry(ext).map_or(ext, |entry| entry.extensions[0])
}

#[cfg(test)]
mod tests;
