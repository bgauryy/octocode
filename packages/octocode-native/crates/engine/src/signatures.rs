mod arguments;
mod calls;
pub(crate) mod extractor;
pub mod graph_facts;
pub(crate) mod js_oxc;
pub mod markup;
pub use js_oxc::call_sites::{EnclosingCallable, ReferenceSite, reference_sites};
#[doc(hidden)]
pub use js_oxc::nesting::nesting_sweep_check;
pub(crate) mod node_kinds;
mod nodes;
pub(crate) mod spelled;
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
/// manufacturing references to an empty target. v14: JS/TS private-method
/// calls (`this.#m()`, `this?.#m()`) are call sites. v15: each file is
/// extracted to completion; no per-file clock cuts facts. v16: Rust
/// declared return types, receiver-call references, and block-level and
/// file-level `static`/`const` receiver types. v17: TS/JS receivers
/// destructured from a call to the file's own function take the types of
/// the object it returns (`const { s } = make()`). v18: the JS/TS tree-sitter
/// lane says it is the fallback, reads `import type`, re-exports, written
/// `export`s and module-level variables, and leaves other load unknown.
/// v20: CommonJS `require` bindings in a function or block, and inline
/// `require(x).k()` receivers, carry the scope extent they hold over.
/// v21: an inline receiver's extent is its callee `require(x).k`, not the
/// whole call, so a call in its arguments is never under it.
/// v22: a module `cfg_attr(not(test), path = ..)` records `plain_path`, so its
/// own-name file is known to compile only under test.
/// v23: a declaration's range and source range start at the outer annotations
/// written before it (Rust `#[…]` attributes), and its doc line is read above
/// or between them.
/// v24: `module.exports = require('./impl')` exports `*` from `./impl`, as
/// `export * from` does.
/// v25: a Go function declares the type of its first result (`return_type`),
/// and a Go member call carries the call whose value its receiver is
/// (`receiver_call`), as Rust's already do.
/// v26: a Go result named by a type parameter declares no `return_type`, and
/// a Go receiver bound to a call keeps that call (`receiver_call`) beside a
/// constructor-name `receiver_type`, which the callee's declared result
/// outranks.
/// v27: JS/TS facts list the declared types of the file's classes,
/// interfaces and type aliases (`declared_types`), and a member call whose
/// receiver only such a type reaches (`x.f.m()` with `x: T`, a parameter of
/// `const f: Alias = (p) => …`, `T["k"]`) carries it (`receiver_declared`).
pub const GRAPH_FACTS_SCHEMA_VERSION: u32 = 27;

/// The declaration kinds an `extends`/`implements` clause can name or
/// declare: the nodes of a type hierarchy. One list for the graph's linking
/// and the hierarchy walks.
pub const TYPE_DECLARATION_KINDS: &[&str] =
    &["class", "interface", "struct", "trait", "type", "enum"];

/// An empty native syntax facts document for `file`, parsed as `language`
/// by oxc.
#[expect(
    clippy::expect_used,
    reason = "source templates address u32 offsets; oxc spans are u32, so its admitted input fits them"
)]
fn native_graph_facts(
    language: String,
    file: &str,
    content: &str,
) -> crate::graph::GraphFactsDocument {
    let source_templates = crate::text::analysis::SourceTemplates::new(
        content,
        crate::text::analysis::Producer::new(format!("oxc-facts:{language}"), "6"),
        crate::text::analysis::AnalysisOperation::GraphFacts,
    )
    .expect("oxc input fits u32 offsets");
    graph_facts_document(language, file, source_templates)
}

/// An empty facts document for `file` whose source correspondence is
/// `source_templates`: the source digest is the one those templates hashed,
/// so each extraction hashes its source once.
fn graph_facts_document(
    language: String,
    file: &str,
    source_templates: crate::text::analysis::SourceTemplates,
) -> crate::graph::GraphFactsDocument {
    crate::graph::GraphFactsDocument {
        kind: "graphFacts".to_owned(),
        schema_version: GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast".to_owned(),
        source_digest: source_templates
            .root_digest()
            .unwrap_or_default()
            .to_owned(),
        source_templates,
        language,
        file: file.to_owned(),
        ..Default::default()
    }
}

pub(crate) struct GraphFactsExtraction {
    pub facts: crate::graph::GraphFactsDocument,
    /// One syntax-aware value-reference count per declaration id, so the
    /// dead-code analysis can tell value escapes from declarations and calls.
    pub reference_counts: Vec<crate::graph::GraphReferenceCount>,
}

pub(crate) fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
) -> Option<GraphFactsExtraction> {
    js_oxc::extract_graph_facts_with_metadata(content, file_path)
        .or_else(|| unless_stopped(|| tree_sitter_graph_facts(content, file_path)))
}

/// `fallback`, unless the caller's work was stopped: an oxc job it cancelled
/// returns nothing, which is no reason to read the file another way.
fn unless_stopped<T>(fallback: impl FnOnce() -> Option<T>) -> Option<T> {
    if crate::deep_stack::caller_stopped() {
        return None;
    }
    fallback()
}

/// [`extract_graph_facts_with_metadata_inner`] parsed as `extension` (a
/// `language` override) instead of the path's own: the override selects the
/// grammar, and the extraction stays with that grammar's owner.
pub(crate) fn extract_graph_facts_parsed_as(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<GraphFactsExtraction> {
    js_oxc::extract_graph_facts_as(content, file_path, Some(extension)).or_else(|| {
        unless_stopped(|| {
            graph_facts::extract_graph_facts_with_metadata_with_extension(
                content, file_path, extension,
            )
        })
    })
}

/// Tree-sitter graph facts with the grammar chosen from the path, except that
/// Flow-typed JavaScript is read with the TSX grammar: the JS grammar recovers
/// Flow annotations as garbage declarations (`if` "functions").
fn tree_sitter_graph_facts(content: &str, file_path: &str) -> Option<GraphFactsExtraction> {
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, grammar)
}

/// The doc line of `declaration`: the doc comment above its header (the
/// first row of its range: decorators, attributes), else, for a header that
/// comments can sit inside (Rust `#[allow] /// doc`), the one above its item.
pub(crate) fn declaration_doc_line(
    lines: &[&str],
    declaration: &crate::graph::GraphFactDeclaration,
    ext: &str,
) -> Option<u32> {
    leading_doc_line(lines, declaration.range.start.line as usize, ext).or_else(|| {
        let item = (declaration.line as usize).saturating_sub(1);
        (item > declaration.range.start.line as usize)
            .then(|| leading_doc_line(lines, item, ext))
            .flatten()
    })
}

/// 0-based line where the doc comment above a declaration starting at
/// `start` (0-based) begins, if any: the topmost comment of the run of
/// comment lines and, for Rust, `#[...]` attribute lines directly above it,
/// in any order (`/// doc` above or between attributes). Blank lines end the
/// run. Comments follow the grammar's [`CommentStyle`] (so C/C++ `#include`
/// and `#define` are code), and a `*` continuation run must reach its `/*`
/// opener.
pub(crate) fn leading_doc_line(lines: &[&str], start: usize, ext: &str) -> Option<u32> {
    let style = languages::find_entry(ext).map_or(CommentStyle::C, |entry| entry.comment_style);
    let is_comment = |line: &str| renderer::is_pure_comment(line, style);
    let mut top = start.min(lines.len());
    let mut doc = None;
    while top > 0 {
        let line = lines[top - 1];
        if is_comment(line) {
            doc = Some(top - 1);
        } else if !(ext == "rs" && line.trim_start().starts_with("#[")) {
            break;
        }
        top -= 1;
    }
    // A block's continuation line alone (` * …`) is not where it starts.
    let doc = doc
        .filter(|&row| style == CommentStyle::Hash || !lines[row].trim_start().starts_with('*'))?;
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
        .or_else(|| unless_stopped(|| tree_sitter_declarations(content, file_path)))
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
    if let Some(facts) = js_oxc::extract_declarations_as(content, file_path, Some(extension)) {
        return Some(facts);
    }
    if crate::deep_stack::caller_stopped() {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        graph_facts::extract_facts_before(
            content,
            file_path,
            extension,
            extractor::whole_file_deadline(),
            true,
        )
    }))
    .unwrap_or(None)
    .map(|extraction| extraction.facts)
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

/// The grammar [`extract_graph_facts`] reads a file with when tree-sitter
/// reads it, with the extension that grammar goes by; `None` when oxc owns
/// the file or no grammar does.
/// The declarations of `content` whose `extends`/`implements` clause names
/// `name`, with those edges, as [`extract_graph_facts`] records them, reading
/// only the parts of the file that spell `name`; `None` when only the whole
/// facts can tell (oxc owns the file, or the narrow walk cannot decide).
#[must_use]
pub fn extract_named_heritage(
    content: &str,
    file_path: &str,
    name: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    let (entry, grammar) = graph_facts_grammar(content, file_path)?;
    let tree = extractor::parse_before(content, &entry.language, extractor::whole_file_deadline())?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        graph_facts::named_heritage_from_tree(content, file_path, &grammar, &tree, name)
    }))
    .unwrap_or(None)
}

/// Whether `content`, the text of `file_path`, can declare a type whose
/// `extends` / `implements` clause the facts record: `false` only for a
/// grammar whose heritage clauses never name a supertype (Go embeds), so a
/// subtype scan reads nothing there without parsing it. Every file oxc
/// owns, and one no grammar reads, may.
#[must_use]
pub fn may_name_supertypes(content: &str, file_path: &str) -> bool {
    graph_facts_grammar(content, file_path)
        .is_none_or(|(_, grammar)| graph_facts::names_supertypes(&grammar))
}

pub(crate) fn graph_facts_grammar(
    content: &str,
    file_path: &str,
) -> Option<(&'static languages::LanguageEntry, String)> {
    if js_oxc::reads(file_path) {
        return None;
    }
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext).to_owned();
    Some((languages::find_entry(&grammar)?, grammar))
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
use languages::CommentStyle;

mod outline;
#[cfg(test)]
pub(crate) use outline::extract_signatures_inner;
pub use outline::{OutlineUnavailable, declarations_outline, is_comment_line};

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
