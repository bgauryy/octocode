//! Native JS/TS symbol outline via `oxc_parser`.
//!
//! Builds an LSP-shaped `DocumentSymbol` tree (nested, numeric `SymbolKind`,
//! 0-based UTF-16 ranges) and flattens it into graph-facts declarations.
//!
//! **No type inference.** oxc parses ECMAScript/TypeScript *syntax*; it resolves
//! in-file scopes/bindings but not types. Callers stamp `source: "native"` so
//! the fidelity tier is explicit. Type-aware outlines still require a server.
//!
//! oxc is less error-tolerant than tree-sitter, so on a hard parse failure we
//! return `None` and the caller falls back to the tree-sitter signature path.

use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use crate::deep_stack::{job_cancelled, run_on_deep_stack_with_timeout, with_thread_allocator};
use crate::text::file_extension::JS_TS_EXTENSIONS;

use super::{
    extractor::AST_EXECUTION_TIMEOUT,
    js_oxc_calls::collect_program_calls,
    js_oxc_commonjs as commonjs,
    js_oxc_references::{CountTarget, value_reference_counts},
    js_oxc_shared::SpanPositions,
};
use crate::graph::{GraphFactDeclaration, GraphFactEdge, GraphFactsDocument, GraphRange};

mod module_facts;
mod outline;
mod symbols;
mod walk;

use outline::{module_outline, push_heritage_edges};
use walk::{collect_member_functions, collect_program};

// LSP SymbolKind numeric codes (subset we emit); keep them in sync with the
// LSP spec. `TYPE_ALIAS` is module-private: LSP has no type-alias kind, and
// the tree never leaves this module, so declarations name it `type`.
mod kind {
    pub const NAMESPACE: u8 = 3;
    pub const CLASS: u8 = 5;
    pub const METHOD: u8 = 6;
    pub const PROPERTY: u8 = 7;
    pub const CONSTRUCTOR: u8 = 9;
    pub const ENUM: u8 = 10;
    pub const INTERFACE: u8 = 11;
    pub const FUNCTION: u8 = 12;
    pub const VARIABLE: u8 = 13;
    pub const CONSTANT: u8 = 14;
    pub const ENUM_MEMBER: u8 = 22;
    /// Outside the LSP range (1..=26): never confused with a spec kind.
    pub const TYPE_ALIAS: u8 = 0;
}

#[derive(Serialize)]
struct DocumentSymbol {
    name: String,
    kind: u8,
    range: GraphRange,
    #[serde(rename = "selectionRange")]
    selection_range: GraphRange,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<Self>,
}

/// Module exports indexed by local binding, with sorted distinct public names.
/// Re-exports (`export … from`) bind no local and are not listed.
#[derive(Default)]
struct LocalExports {
    bindings: BTreeMap<String, BTreeSet<String>>,
}

impl LocalExports {
    fn push(&mut self, local: String, public: String) {
        self.bindings.entry(local).or_default().insert(public);
    }

    fn public_names(&self, local: &str) -> Option<Vec<String>> {
        self.bindings
            .get(local)
            .map(|names| names.iter().cloned().collect())
    }
}

fn source_type_for(ext: &str, file_path: &str, content: &str) -> SourceType {
    // oxc has no Flow parser. Flow annotations are close enough to TypeScript
    // that a TSX parse usually succeeds; when it cannot, the hard failure
    // routes the caller to the tree-sitter TSX grammar (see `grammar_extension`).
    if crate::text::file_extension::is_flow_source(content, ext) {
        return SourceType::tsx();
    }
    // `from_path` owns the extension policy: `.cjs`/`.cts` are CommonJS (top-level
    // `return` allowed), `.mjs`/`.mts` are ES modules, `.js`/`.ts` are
    // unambiguous (module only with import/export), and `.d.ts`/`.d.mts`/`.d.cts`
    // are ambient declarations. Paths without a JS/TS extension fall back to the
    // caller's defaulted extension (`extension_of(.., "ts")`).
    let source_type = SourceType::from_path(file_path)
        .or_else(|_| SourceType::from_extension(ext))
        .unwrap_or_else(|_| SourceType::ts());
    // React components commonly live in plain `.js`/`.mjs`/`.cjs`; JSX there must
    // not be a parse error. TypeScript stays JSX-free so `<T>x` assertions parse.
    if source_type.is_javascript() {
        source_type.with_jsx(true)
    } else {
        source_type
    }
}

/// Cheap pre-check so non-JS/TS files never pay for the content copy and the
/// big-stack worker thread; graph scans call this for every file in a repo.
fn is_oxc_path(file_path: &str) -> bool {
    JS_TS_EXTENSIONS
        .contains(&crate::text::file_extension::extension_of(file_path, true, "ts").as_str())
}

/// Run an oxc `job` on the deep-stack pool under the AST timeout. `None` for
/// oversized input or a non-JS/TS path. oxc can ICE on pathological input;
/// the unwind is contained so it never aborts the process.
fn on_oxc_worker<T: Send + 'static>(
    content: &str,
    file_path: &str,
    job: impl FnOnce(&str, &str) -> Option<T> + Send + 'static,
) -> Option<T> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE || !is_oxc_path(file_path) {
        return None;
    }
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    run_on_deep_stack_with_timeout(AST_EXECUTION_TIMEOUT, move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&content, &file_path)))
            .unwrap_or(None)
    })
}

/// Parse JS/TS `content` on the thread arena (`TOKENS` keeps the token
/// stream) and hand `f` the extension and the parse. `None` for a non-JS/TS
/// extension, a cancelled job, or a hard parse failure with nothing
/// recovered: the caller then falls back to the more error-tolerant
/// tree-sitter path rather than emit a stub.
fn with_js_program<const TOKENS: bool, R>(
    content: &str,
    file_path: &str,
    f: impl for<'a> FnOnce(String, oxc_parser::ParserReturn<'a>) -> Option<R>,
) -> Option<R> {
    let ext = crate::text::file_extension::extension_of(file_path, true, "ts");
    if !JS_TS_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    with_thread_allocator(|allocator| {
        let parser = Parser::new(
            allocator,
            content,
            source_type_for(&ext, file_path, content),
        );
        let parser_ret = if TOKENS {
            parser
                .with_config(oxc_parser::config::TokensParserConfig)
                .parse()
        } else {
            parser.parse()
        };
        if job_cancelled()
            || (parser_ret.program.body.is_empty() && !parser_ret.diagnostics.is_empty())
        {
            return None;
        }
        f(ext, parser_ret)
    })
}

/// Native JS/TS graph facts: a syntax-level AST inventory of declarations,
/// imports, exports, function/class containment, and direct call expressions.
/// It deliberately avoids type inference and cross-file resolution; callers
/// combine it with LSP proof when they need semantic identity.
pub(crate) fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<super::GraphFactsExtraction> {
    on_oxc_worker(content, file_path, extract_graph_facts_with_metadata_inner)
}

fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
) -> Option<super::GraphFactsExtraction> {
    with_js_program::<true, _>(content, file_path, |ext, parser_ret| {
        let line_index = SpanPositions::new(content);
        let mut symbols = Vec::new();
        collect_program(&parser_ret.program, &line_index, &mut symbols);
        let (declarations, mut edges, imports, exports) = module_outline(
            &parser_ret.program,
            &line_index,
            file_path,
            content,
            &symbols,
        );
        if job_cancelled() {
            return None;
        }

        let mut calls = Vec::new();
        let mut heritage = Vec::new();
        collect_program_calls(&parser_ret.program, &line_index, &mut calls, &mut heritage);
        if job_cancelled() {
            return None;
        }
        // OXC tokens make this an absence check, not a binding claim. Candidate
        // loaders still require the full native scope inventory, including escapes.
        let common_js = if commonjs::may_contain_loader(&parser_ret.tokens, content) {
            commonjs::collect_common_js_loads(&parser_ret.program, &line_index)
        } else {
            Vec::new()
        };
        let mut declarations_by_name: std::collections::HashMap<&str, Vec<&GraphFactDeclaration>> =
            std::collections::HashMap::new();
        for declaration in &declarations {
            declarations_by_name
                .entry(&declaration.name)
                .or_default()
                .push(declaration);
        }
        for (index, call) in calls.iter_mut().enumerate() {
            // Names are display labels. Source ranges identify the enclosing occurrence;
            // callee binding resolution requires semantics and remains explicitly unknown.
            call.id = format!(
                "call:{}@{}:{}:{index}",
                file_path, call.range.start.line, call.range.start.character
            );
            let caller = declarations_by_name
                .get(call.caller.as_str())
                .into_iter()
                .flat_map(|items| items.iter().copied())
                .filter(|declaration| {
                    (
                        declaration.range.start.line,
                        declaration.range.start.character,
                    ) <= (call.range.start.line, call.range.start.character)
                        && (declaration.range.end.line, declaration.range.end.character)
                            >= (call.range.end.line, call.range.end.character)
                })
                .min_by_key(|declaration| {
                    (
                        declaration.range.end.line - declaration.range.start.line,
                        declaration
                            .range
                            .end
                            .character
                            .saturating_sub(declaration.range.start.character),
                    )
                });
            call.caller_id = caller.map(|declaration| declaration.id.clone());
            edges.push(GraphFactEdge {
                id: format!("edge:{}", call.id),
                from: call
                    .caller_id
                    .clone()
                    .unwrap_or_else(|| format!("file:{file_path}")),
                to: format!(
                    "reference:{}@{}:{}:{index}",
                    file_path, call.range.start.line, call.range.start.character
                ),
                relation: call.kind.clone(),
                source: "ast".to_owned(),
                line: call.line,
                resolution: "unresolved".to_owned(),
            });
        }
        push_heritage_edges(&declarations, heritage, &mut edges);

        let facts = GraphFactsDocument {
            declarations,
            imports,
            exports,
            calls,
            common_js,
            edges,
            diagnostics: parser_ret
                .diagnostics
                .into_iter()
                .map(|error| parse_error(error.message.to_string()))
                .collect(),
            ..super::native_graph_facts(ext, file_path)
        };
        let semantic = SemanticBuilder::new()
            .with_build_nodes(true)
            .build(&parser_ret.program)
            .semantic;
        if job_cancelled() {
            return None;
        }
        let targets: Vec<_> = facts
            .declarations
            .iter()
            .map(|declaration| CountTarget {
                id: &declaration.id,
                name: &declaration.name,
                line: declaration.selection_range.start.line,
                character: declaration.selection_range.start.character,
            })
            .collect();
        let reference_counts = value_reference_counts(&semantic, &line_index, &targets);
        Some(super::GraphFactsExtraction {
            facts,
            reference_counts,
        })
    })
}

/// Declarations (with exports and containment) only: the calls, CommonJS
/// loads, and value-reference counts a dependency graph needs are skipped. An
/// outline of a large file otherwise pays for a full semantic pass and a
/// multi-megabyte facts document it never reads.
pub(crate) fn extract_declarations(content: &str, file_path: &str) -> Option<GraphFactsDocument> {
    on_oxc_worker(content, file_path, extract_declarations_inner)
}

fn extract_declarations_inner(content: &str, file_path: &str) -> Option<GraphFactsDocument> {
    with_js_program::<false, _>(content, file_path, |ext, parser_ret| {
        let line_index = SpanPositions::new(content);
        let mut symbols = Vec::new();
        collect_program(&parser_ret.program, &line_index, &mut symbols);
        collect_member_functions(&parser_ret.program.body, &line_index, &mut symbols);
        symbols.sort_by_key(|symbol| (symbol.range.start.line, symbol.range.start.character));
        let (declarations, edges, imports, exports) = module_outline(
            &parser_ret.program,
            &line_index,
            file_path,
            content,
            &symbols,
        );
        if job_cancelled() {
            return None;
        }
        Some(GraphFactsDocument {
            declarations,
            imports,
            exports,
            edges,
            diagnostics: parser_ret
                .diagnostics
                .into_iter()
                .map(|error| parse_error(error.message.to_string()))
                .collect(),
            ..super::native_graph_facts(ext, file_path)
        })
    })
}

/// A parser error as a fact diagnostic.
fn parse_error(message: String) -> crate::graph::GraphFactDiagnostic {
    crate::graph::GraphFactDiagnostic {
        code: crate::graph::GraphFactCode::ParseError,
        message,
        line: None,
    }
}

#[cfg(test)]
pub(super) mod tests;
