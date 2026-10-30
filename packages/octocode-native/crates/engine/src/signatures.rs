/// Largest source the parsers accept (declarations, graph facts, oxc,
/// syntax trees). Separate from the minifier's 1 MiB guard: parsing a real
/// monolithic source (TypeScript's 3 MB checker.ts) is cheap and bounded,
/// and the runtime tools share this bound.
pub const MAX_PARSE_SIZE: usize = 8 * 1024 * 1024;

mod arguments;
mod calls;
pub(crate) mod extractor;
pub mod graph_facts;
pub(crate) mod js_oxc;
mod nodes;
mod visibility;

mod js_oxc_arguments;
mod js_oxc_calls;
mod js_oxc_commonjs;
mod js_oxc_receiver;
mod js_oxc_references;
mod js_oxc_shared;
pub(crate) mod receiver_fields;

/// v7: CommonJS `require` bindings and `module.exports` export shapes.
/// v8: Scala heritage, one site per curried chain, generic callees without
/// type arguments, receiver types and term bindings.
/// v9: Scala self-alias templates and import scopes; React-wrapped default
/// exports. v10: calls read from Rust expression-macro arguments.
/// v11: CommonJS exports defined in place are declarations; C/C++
/// prototypes and out-of-line owners, C++ `.h` headers, callees without
/// template arguments, statements beside a preprocessor-recovered one.
/// v12: Go method owners, embedded types and import aliases; Python
/// decorator calls and `TYPE_CHECKING` imports; JS chained receivers and
/// object-literal members; C# local functions; Scala `new` constructions.
/// v13: missing argument names remain unsupported observations, without
/// manufacturing references to an empty target.
pub const GRAPH_FACTS_SCHEMA_VERSION: u32 = 13;

/// The declaration kinds an `extends`/`implements` clause can name or
/// declare: the nodes of a type hierarchy. One list for the graph's linking
/// and the hierarchy walks.
pub const TYPE_DECLARATION_KINDS: &[&str] =
    &["class", "interface", "struct", "trait", "type", "enum"];

/// An empty native syntax facts document for `file`, parsed as `language`.
#[expect(
    clippy::expect_used,
    reason = "OXC admission rejects input above MAX_PARSE_SIZE before this private constructor"
)]
fn native_graph_facts(
    language: String,
    file: &str,
    content: &str,
) -> crate::graph::GraphFactsDocument {
    crate::graph::GraphFactsDocument {
        kind: "graphFacts".to_owned(),
        schema_version: GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast".to_owned(),
        source_templates: crate::text::analysis::SourceTemplates::new(
            content,
            crate::text::analysis::Producer::new(format!("oxc-facts:{language}"), "6"),
            crate::text::analysis::AnalysisOperation::GraphFacts,
        )
        .expect("parser input is bounded to MAX_PARSE_SIZE"),
        language,
        file: file.to_owned(),
        source_digest: crate::digest::sha256(content.as_bytes()),
        ..Default::default()
    }
}

pub(crate) struct GraphFactsExtraction {
    pub facts: crate::graph::GraphFactsDocument,
    /// One syntax-aware value-reference count per declaration id, so the
    /// dead-code analysis can tell value escapes from declarations and calls.
    pub reference_counts: Vec<crate::types::GraphReferenceCount>,
}

pub(crate) fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
) -> Option<GraphFactsExtraction> {
    js_oxc::extract_graph_facts_with_metadata(content, file_path)
        .or_else(|| tree_sitter_graph_facts(content, file_path))
}

/// Tree-sitter graph facts with the grammar chosen from the path, except that
/// Flow-typed JavaScript is read with the TSX grammar: the JS grammar recovers
/// Flow annotations as garbage declarations (`if` "functions").
fn tree_sitter_graph_facts(content: &str, file_path: &str) -> Option<GraphFactsExtraction> {
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, grammar)
}

/// 0-based line where the comment block directly above a declaration
/// starting at `start` (0-based) begins, if any. Blank lines end the block;
/// Rust `#[...]` attributes between the comment and the item are skipped.
/// Comments follow the grammar's [`CommentStyle`] (so C/C++ `#include` and
/// `#define` are code), and a `*` continuation run must reach its `/*` opener.
pub(crate) fn leading_doc_line(lines: &[&str], start: usize, ext: &str) -> Option<u32> {
    let style = languages::find_entry(ext).map_or(CommentStyle::C, |entry| entry.comment_style);
    let is_comment = |line: &str| renderer::is_pure_comment(line, style);
    let mut top = start.min(lines.len());
    while ext == "rs" && top > 0 && lines[top - 1].trim_start().starts_with("#[") {
        top -= 1;
    }
    let mut doc = top;
    while doc > 0 && is_comment(lines[doc - 1]) {
        doc -= 1;
    }
    // A block's continuation line alone (` * …`) is not where it starts.
    if doc == top || (style != CommentStyle::Hash && lines[doc].trim_start().starts_with('*')) {
        return None;
    }
    u32::try_from(doc).ok()
}

/// Declarations-only facts for outlines: the light oxc path for JS/TS; other
/// languages' single tree-sitter walk already costs about the same.
#[must_use]
pub fn extract_declarations_document(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    js_oxc::extract_declarations(content, file_path)
        .or_else(|| tree_sitter_declarations(content, file_path))
        .map(|mut facts| {
            if let Some(unit) = facts.source_templates.units.first_mut() {
                unit.operation = crate::text::analysis::AnalysisOperation::Declarations;
            }
            facts
        })
}

/// [`extract_declarations_document`] parsed with the grammar of `extension`
/// (a `language` override) instead of the path's own: declarations only, no
/// call facts, reference counts or import uses, which is what outlines and
/// enclosing names read, at a fraction of the walk.
#[must_use]
pub fn extract_declarations_document_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    if content.len() > MAX_PARSE_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        graph_facts::extract_facts_before(
            content,
            file_path,
            extension,
            std::time::Instant::now() + extractor::AST_EXECUTION_TIMEOUT,
            true,
        )
    }))
    .unwrap_or(None)
    .map(|extraction| extraction.facts)
}

/// The symbols outline of `content` ([`extract_signatures_inner`]) and its
/// declarations-only facts ([`extract_declarations_document`]) from ONE
/// tree-sitter parse wherever both read the same grammar; JS/TS
/// declarations come from oxc, the outline from tree-sitter. Runs on the
/// shared deep parser stack.
pub fn extract_signatures_with_declarations(
    content: &str,
    file_path: &str,
) -> (
    Result<String, OutlineUnavailable>,
    Option<crate::graph::GraphFactsDocument>,
) {
    let (content, file_path) = (content.to_owned(), file_path.to_owned());
    crate::deep_stack::run_on_deep_stack(move || {
        Some(signatures_with_declarations(&content, &file_path))
    })
    .unwrap_or((Err(OutlineUnavailable::ParseFailed), None))
}

fn signatures_with_declarations(
    content: &str,
    file_path: &str,
) -> (
    Result<String, OutlineUnavailable>,
    Option<crate::graph::GraphFactsDocument>,
) {
    if content.len() > MAX_PARSE_SIZE {
        return (Err(OutlineUnavailable::TooLarge), None);
    }
    if let Some(mut declarations) = js_oxc::extract_declarations(content, file_path) {
        if let Some(unit) = declarations.source_templates.units.first_mut() {
            unit.operation = crate::text::analysis::AnalysisOperation::Declarations;
        }
        return (
            extract_signatures_inner(content, file_path),
            Some(declarations),
        );
    }
    let shared = std::panic::catch_unwind(|| {
        let ext = extension_of(file_path, true, "txt");
        let grammar = crate::text::file_extension::grammar_extension(content, &ext);
        let entry = languages::find_entry(grammar).ok_or(OutlineUnavailable::UnsupportedLanguage);
        // Each reader keeps the deadline it has alone; only the parse is shared.
        let deadline = || std::time::Instant::now() + extractor::AST_EXECUTION_TIMEOUT;
        let tree = entry
            .ok()
            .and_then(|entry| extractor::parse_before(content, &entry.language, deadline()));
        let declarations = graph_facts::declaration_facts_from_tree(
            content,
            file_path,
            grammar,
            tree.as_ref(),
            deadline(),
        );
        let skeleton = entry.and_then(|entry| {
            let cfg = LangExtractConfig {
                language: entry.language.clone(),
                body_query: entry.body_query,
            };
            if !entry.admits(crate::text::analysis::AnalysisOperation::SignatureOutline) {
                return Err(OutlineUnavailable::UnsupportedLanguage);
            }
            let tree = tree.as_ref().ok_or(OutlineUnavailable::ParseFailed)?;
            let kept = extractor::outline_from_tree(content, &cfg, tree, deadline())
                .ok_or(OutlineUnavailable::ParseFailed)?;
            renderer::render_skeleton(&kept, entry.comment_style)
                .ok_or(OutlineUnavailable::NoDeclarations)
        });
        (skeleton, declarations)
    });
    match shared {
        Ok((skeleton, declarations)) => (
            skeleton.and_then(|skeleton| usable_outline(content, skeleton)),
            declarations,
        ),
        Err(_) => (Err(OutlineUnavailable::ParseFailed), None),
    }
}

/// [`tree_sitter_graph_facts`] without call facts (see
/// [`extract_declarations_document_with_extension`]).
fn tree_sitter_declarations(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    extract_declarations_document_with_extension(content, file_path, grammar)
}

#[must_use]
pub fn extract_graph_facts(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    extract_graph_facts_with_metadata_inner(content, file_path).map(|extraction| extraction.facts)
}

pub mod languages;
pub(crate) mod renderer;

/// Every declaration kind the extractors emit: tree-sitter graph facts
/// (`nodes::declaration_kind` and `nodes::declaration`) and the JS/TS oxc
/// extractor (`js_oxc::symbol_kind_name`).
pub const DECLARATION_KINDS: &[&str] = &[
    "class",
    "constant",
    "constructor",
    "enum",
    "enumMember",
    "function",
    "impl",
    "interface",
    "label",
    "macro",
    "method",
    "module",
    "namespace",
    "property",
    "struct",
    "symbol",
    "trait",
    "type",
    "variable",
];

use crate::text::extension_of;
use extractor::LangExtractConfig;
use languages::CommentStyle;

mod outline;
pub(crate) use outline::extract_signatures_inner;
use outline::usable_outline;
pub use outline::{OutlineUnavailable, is_comment_line};

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
