/// Largest source the parsers accept (declarations, graph facts, oxc,
/// syntax trees). Separate from the minifier's 1 MiB guard: parsing a real
/// monolithic source (TypeScript's 3 MB checker.ts) is cheap and bounded,
/// and the runtime tools share this bound.
pub const MAX_PARSE_SIZE: usize = 8 * 1024 * 1024;

pub(crate) mod extractor;
pub mod graph_facts;
pub(crate) mod js_oxc;
mod nodes;

mod js_oxc_calls;
mod js_oxc_commonjs;
mod js_oxc_receiver;
mod js_oxc_references;
mod js_oxc_shared;
pub(crate) mod receiver_fields;

/// v2: diagnostics are typed records (`code`, `message`, `line`), not text.
pub const GRAPH_FACTS_SCHEMA_VERSION: u32 = 2;

/// The declaration kinds an `extends`/`implements` clause can name or
/// declare: the nodes of a type hierarchy. One list for the graph's linking
/// and the hierarchy walks.
pub const TYPE_DECLARATION_KINDS: &[&str] =
    &["class", "interface", "struct", "trait", "type", "enum"];

/// An empty native syntax facts document for `file`, parsed as `language`.
fn native_graph_facts(language: String, file: &str) -> crate::graph::GraphFactsDocument {
    crate::graph::GraphFactsDocument {
        kind: "graphFacts".to_owned(),
        schema_version: GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast".to_owned(),
        language,
        file: file.to_owned(),
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
}

/// [`extract_declarations_document`] parsed with the grammar of `extension`
/// (a `language` override) instead of the path's own: declarations only, no
/// call or path facts.
#[must_use]
pub fn extract_declarations_document_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    graph_facts::extract_declaration_facts_with_extension(content, file_path, extension)
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
    if let Some(declarations) = js_oxc::extract_declarations(content, file_path) {
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
            if entry.body_query.is_empty() {
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
/// `graph_facts::extract_declaration_facts_with_extension`).
fn tree_sitter_declarations(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_declaration_facts_with_extension(content, file_path, grammar)
}

#[must_use]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata_inner(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
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

use crate::text::file_extension::extension_of;
use extractor::LangExtractConfig;
use languages::CommentStyle;

/// Why a file has no symbols outline: the typed reason a reader falls back
/// to another view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutlineUnavailable {
    /// Larger than [`MAX_PARSE_SIZE`]: not parsed.
    TooLarge,
    /// No grammar with an outline for this file type.
    UnsupportedLanguage,
    /// The parser gave no tree in time.
    ParseFailed,
    /// The parse kept no declaration line.
    NoDeclarations,
    /// The outline would be no smaller than the source.
    NotSmaller,
    /// Over-long lines (minified or bundled source) make up most of the
    /// outline, so it is not a scannable list of signatures.
    Minified,
}

impl OutlineUnavailable {
    /// The reason as the readers report it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too-large",
            Self::UnsupportedLanguage => "unsupported-language",
            Self::ParseFailed => "parse-failed",
            Self::NoDeclarations => "no-declarations",
            Self::NotSmaller => "not-smaller",
            Self::Minified => "minified-lines",
        }
    }
}

/// Extract a structural skeleton from `content`: `N\ttext` lines, or the
/// typed reason there is none.
pub(crate) fn extract_signatures_inner(
    content: &str,
    file_path: &str,
) -> Result<String, OutlineUnavailable> {
    if content.len() > MAX_PARSE_SIZE {
        return Err(OutlineUnavailable::TooLarge);
    }
    let skeleton = std::panic::catch_unwind(|| {
        let ext = extension_of(file_path, true, "txt");
        extract_by_ext(
            content,
            crate::text::file_extension::grammar_extension(content, &ext),
        )
    })
    .unwrap_or(Err(OutlineUnavailable::ParseFailed))?;
    usable_outline(content, skeleton)
}

/// `skeleton` as the outline of `content`, unless it is no smaller than the
/// source or mostly over-long lines.
fn usable_outline(content: &str, skeleton: String) -> Result<String, OutlineUnavailable> {
    // Return the source view when the rendered outline is no smaller. A file
    // with few body lines can grow once the outline includes line gutters.
    //
    // The comparison is on the rendered output the agent actually receives
    // (gutter included) — that is the byte count we promise never to inflate.
    // Tiny files where the per-line gutter alone tips the balance are suppressed
    // too, which is correct: a symbol outline of a handful of lines carries no
    // navigational value over just showing the lines.
    if skeleton.len() >= content.len() {
        return Err(OutlineUnavailable::NotSmaller);
    }

    // A usable outline is scannable: mostly one short signature per line.
    // Minified or bundled sources pack whole declarations onto a few enormous
    // physical lines, so per-line extraction keeps those lines verbatim and
    // the "outline" is really raw minified bytes. That holds when over-long
    // lines make up most of the outline; a few long declarations (generated
    // one-line structs) in an otherwise normal outline stay, verbatim, and
    // their range gutter makes each body one `ranges` read away.
    let long_chars: usize = skeleton
        .lines()
        .map(|line| line.chars().count())
        .filter(|chars| *chars > OUTLINE_MAX_LINE_CHARS)
        .sum();
    if long_chars.saturating_mul(2) > skeleton.chars().count() {
        return Err(OutlineUnavailable::Minified);
    }
    Ok(skeleton)
}

/// Whether `line` is a whole-line comment in the comment syntax of the
/// grammar that reads `file_path`; `None` when no grammar reads it.
#[must_use]
pub fn is_comment_line(file_path: &str, line: &str) -> Option<bool> {
    let ext = extension_of(file_path, true, "txt");
    let entry = languages::find_entry(&ext)?;
    Some(renderer::is_pure_comment(line, entry.comment_style))
}

/// Longest rendered outline line (gutter included) still treated as a scannable
/// signature. Hand-written declaration lines effectively never reach this;
/// an outline made mostly of longer lines is minified/bundled source.
const OUTLINE_MAX_LINE_CHARS: usize = 1000;

fn extract_by_ext(content: &str, ext: &str) -> Result<String, OutlineUnavailable> {
    // Tree-sitter is the only signature path — real AST parsing, no regex
    // heuristics. Languages outside the canonical registry have no outline
    // and the caller falls back to the standard/none view.
    let entry = languages::find_entry(ext)
        .filter(|entry| !entry.body_query.is_empty())
        .ok_or(OutlineUnavailable::UnsupportedLanguage)?;
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    let kept = extractor::extract_outline(content, &cfg).ok_or(OutlineUnavailable::ParseFailed)?;
    renderer::render_skeleton(&kept, entry.comment_style).ok_or(OutlineUnavailable::NoDeclarations)
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
