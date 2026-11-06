//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use crate::graph::{
    GraphFactCall, GraphFactCode, GraphFactDeclaration, GraphFactDiagnostic, GraphFactEdge,
    GraphFactExport, GraphFactImport, GraphFactRustModule,
};

use super::languages;

pub(super) mod c_family;
mod go;
mod heritage;
mod js_fallback;
mod python;
mod receiver;
mod rust;
mod rust_cfg;
mod rust_macro_args;
mod scala;

mod arguments;
mod collect;
mod declarations;
mod from_tree;
mod imports;
mod macro_bodies;
mod positions;
mod references;
mod test_adapter;
mod validity;
mod walk;

use from_tree::facts_from_tree;
use imports::push_language_import;
use macro_bodies::MacroBody;
use positions::NodePositions;
#[cfg(test)]
use walk::visit_node;

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
        // `js_oxc` states which CommonJS export write it could not order.
        GraphFactCode::UnsupportedCommonJsExports => {
            "CommonJS export shape is not provable; no CommonJS names are exported"
        }
        // `js_oxc` states its parser's own errors.
        GraphFactCode::ParseError => "the parser reported a syntax error",
        GraphFactCode::BuildExcluded => {
            "a `//go:build` constraint no build satisfies keeps this file out of its package: it links to nothing and nothing links to it"
        }
        GraphFactCode::ParserFallback => {
            "oxc did not read this file, so its facts come from the tree-sitter lane: module load is unknown unless a `type` token states it, and class fields and interface members are not declared"
        }
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
    /// A JS/TS file oxc did not read: [`js_fallback`] owns its module facts.
    js_fallback: bool,
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
    /// Bodies of Rust item-level macro calls (`cfg_rt! { mod x; }`) and of
    /// admitted expression macros (`ready!(f(cx))`) with their enclosing
    /// module scope and declaration (an `impl` holding `cfg_io_util! { pub
    /// fn … }`), re-read after the main walk.
    macro_bodies: Vec<MacroBody>,
    /// While an expression macro body is read: the macros that hold it,
    /// outermost first; stamped on its calls.
    macro_chain: Option<Vec<String>>,
    /// While such a body of a recovered declaration is read: that
    /// assessment, stamped on its calls and argument sites.
    macro_recovered: Option<crate::graph::GraphParseAssessment>,
    /// Byte span of each entry of `declarations`, in the same order.
    declaration_spans: Vec<(usize, usize)>,
    /// Name tokens the main walk passed, for `count_name_references`;
    /// `None` outside that walk (macro bodies are not counted).
    name_tokens: Option<Vec<NameToken>>,
    /// Outline extraction: skip call facts, which an outline never reads and
    /// which are most of a large file's walk and facts.
    declarations_only: bool,
    /// When set, the walk enters only the nodes that spell this word.
    only_spelling: Option<crate::signatures::spelled::Spelled>,
    test_context: test_adapter::TestContext,
    /// Python: the file's names for `typing.TYPE_CHECKING`.
    typing: python::TypingBindings,
    /// C++ `namespace al = a::b;` seen so far: alias → what it names.
    namespace_aliases: std::collections::HashMap<String, String>,
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
            js_fallback: false,
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
            namespace_aliases: std::collections::HashMap::new(),
            macro_chain: None,
            macro_recovered: None,
            declaration_spans: Vec::new(),
            name_tokens: None,
            declarations_only: false,
            only_spelling: None,
            test_context: test_adapter::TestContext::default(),
            typing: python::TypingBindings::default(),
            diagnostics: vec![note(GraphFactCode::SyntaxOnly)],
        }
    }
}

pub use from_tree::NamedSites;
pub(crate) use from_tree::{named_heritage_from_tree, named_sites_from_tree};

pub(crate) fn extract_graph_facts_with_metadata_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<super::GraphFactsExtraction> {
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
        super::extractor::whole_file_deadline(),
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

pub fn graph_fact_extensions() -> Vec<String> {
    let mut exts: Vec<String> = languages::supported_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

/// Whether the heritage clauses of the grammar `extension` names can name a
/// supertype (`extends` / `implements`); Go's only record `embeds`.
pub(crate) fn names_supertypes(extension: &str) -> bool {
    heritage::Clauses::of(canonical_extension(extension)).names_supertypes()
}

/// The first extension of the language `ext` belongs to (`pyi` → `py`).
fn canonical_extension(ext: &str) -> &str {
    languages::find_entry(ext).map_or(ext, |entry| entry.extensions[0])
}

#[cfg(test)]
mod tests;
