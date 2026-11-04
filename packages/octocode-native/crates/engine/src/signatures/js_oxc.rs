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
#[cfg(test)]
use oxc_span::SourceType;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use crate::deep_stack::{job_cancelled, with_thread_allocator};

use super::{
    js_oxc_calls::collect_program_calls,
    js_oxc_commonjs,
    js_oxc_references::{CountTarget, value_reference_counts},
    js_oxc_shared::SpanPositions,
};
use crate::graph::{GraphFactEdge, GraphFactsDocument, GraphRange};

pub(super) mod call_sites;
mod callers;
mod dialect;
mod module_facts;
mod outline;
mod symbols;
mod validity;
mod walk;

use dialect::{is_oxc_dialect, on_oxc_worker, source_type_for, with_js_program};
use outline::{module_outline, push_heritage_edges};
use validity::assess_recovery;
use walk::{collect_member_functions, collect_program, push_defined_exports};

/// The kind of a TypeScript type alias, module-private: LSP has no type-alias
/// kind, and the tree never leaves this module, so declarations name it
/// `type`. Outside the LSP range (1..=26): never confused with a spec kind.
const TYPE_ALIAS_KIND: u8 = 0;

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

/// Native JS/TS graph facts: a syntax-level AST inventory of declarations,
/// imports, exports, function/class containment, and direct call expressions.
/// It deliberately avoids type inference and cross-file resolution; callers
/// combine it with LSP proof when they need semantic identity.
pub(crate) fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<super::GraphFactsExtraction> {
    extract_graph_facts_as(content, file_path, None)
}

/// [`extract_graph_facts_with_metadata`], read as `parser`'s dialect.
pub(crate) fn extract_graph_facts_as(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
) -> Option<super::GraphFactsExtraction> {
    let dialect = parser.map(str::to_owned);
    on_oxc_worker(content, file_path, parser, move |content, file_path| {
        extract_graph_facts_with_metadata_inner(content, file_path, dialect.as_deref())
    })
}

fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
) -> Option<super::GraphFactsExtraction> {
    with_js_program::<true, _>(content, file_path, parser, |ext, parser_ret| {
        let line_index = SpanPositions::new(content);
        let mut symbols = Vec::new();
        collect_program(&parser_ret.program, &line_index, &mut symbols);
        // One semantic build serves CommonJS shapes, arguments and references.
        let semantic = SemanticBuilder::new()
            .with_build_nodes(true)
            .build(&parser_ret.program)
            .semantic;
        let program = &parser_ret.program;
        let (common_js_exports, common_js_note) =
            if js_oxc_commonjs::may_contain_exports(&parser_ret.tokens, content) {
                js_oxc_commonjs::collect_common_js_exports(&semantic, program, &line_index)
            } else {
                Default::default()
            };
        let mut common_js_names = common_js_exports.names;
        common_js_names.extend(push_defined_exports(
            common_js_exports.defined,
            &line_index,
            &mut symbols,
        ));
        let (mut declarations, mut edges, imports, exports) = module_outline(
            &parser_ret.program,
            &line_index,
            file_path,
            content,
            &symbols,
            common_js_names,
        );
        if job_cancelled() {
            return None;
        }

        let mut calls = Vec::new();
        let mut heritage = Vec::new();
        collect_program_calls(&parser_ret.program, &line_index, &mut calls, &mut heritage);
        assess_recovery(&parser_ret, &line_index, &mut declarations, &mut calls);
        if job_cancelled() {
            return None;
        }
        // OXC tokens make this an absence check, not a binding claim. Candidate
        // loaders still require the full native scope inventory, including escapes.
        let common_js = if js_oxc_commonjs::may_contain_loader(&parser_ret.tokens, content) {
            js_oxc_commonjs::collect_common_js_loads(
                &semantic,
                &parser_ret.program,
                &line_index,
                &declarations,
            )
        } else {
            Vec::new()
        };
        let callers = callers::Callers::new(&declarations);
        for (index, call) in calls.iter_mut().enumerate() {
            // Names are display labels. Source ranges identify the enclosing occurrence;
            // callee binding resolution requires semantics and remains explicitly unknown.
            call.id = format!(
                "call:{}@{}:{}:{index}",
                file_path, call.range.start.line, call.range.start.character
            );
            if let Some(caller) = callers.of(call) {
                call.caller.clone_from(&caller.name);
                call.caller_id = Some(caller.id.clone());
            }
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
        let base = super::native_graph_facts(ext, file_path, content);
        let argument_sites = super::js_oxc_arguments::collect(
            &semantic,
            &parser_ret,
            &line_index,
            (content, &base.source_digest),
            file_path,
            &declarations,
            &imports,
            &calls,
        );

        let facts = GraphFactsDocument {
            declarations,
            imports,
            exports,
            calls,
            argument_sites,
            common_js,
            edges,
            diagnostics: parser_ret
                .diagnostics
                .into_iter()
                .map(|error| parse_error(error.message.to_string()))
                .chain(common_js_note)
                .collect(),
            ..base
        };
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
    extract_declarations_as(content, file_path, None)
}

/// [`extract_declarations`], read as `parser`'s dialect.
pub(crate) fn extract_declarations_as(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
) -> Option<GraphFactsDocument> {
    let dialect = parser.map(str::to_owned);
    on_oxc_worker(content, file_path, parser, move |content, file_path| {
        extract_declarations_inner(content, file_path, dialect.as_deref())
    })
}

/// [`extract_declarations`] for a caller already on the deep-stack pool:
/// parsed in place, with no copy of `content` for another job.
pub(crate) fn extract_declarations_in_place(
    content: &str,
    path: &str,
) -> Option<GraphFactsDocument> {
    let parse = || extract_declarations_inner(content, path, None);
    (content.len() <= crate::signatures::MAX_PARSE_SIZE && is_oxc_dialect(path, None))
        .then(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(parse)).unwrap_or(None))?
}

fn extract_declarations_inner(
    content: &str,
    file_path: &str,
    parser: Option<&str>,
) -> Option<GraphFactsDocument> {
    with_js_program::<false, _>(content, file_path, parser, |ext, parser_ret| {
        let line_index = SpanPositions::new(content);
        let mut symbols = Vec::new();
        collect_program(&parser_ret.program, &line_index, &mut symbols);
        collect_member_functions(&parser_ret.program.body, &line_index, &mut symbols);
        symbols.sort_by_key(|symbol| (symbol.range.start.line, symbol.range.start.character));
        let (mut declarations, edges, imports, exports) = module_outline(
            &parser_ret.program,
            &line_index,
            file_path,
            content,
            &symbols,
            Vec::new(),
        );
        assess_recovery(&parser_ret, &line_index, &mut declarations, &mut []);
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
            ..super::native_graph_facts(ext, file_path, content)
        })
    })
}

/// Why oxc gives no facts for `content` read as `parser`'s dialect: its first
/// error, with the 1-based line it names. `None` when it reports none (it
/// could not run, or the caller stopped).
pub(super) fn parse_failure(
    content: &str,
    file_path: &str,
    parser: &str,
) -> Option<(String, Option<u32>)> {
    let dialect = parser.to_owned();
    on_oxc_worker(
        content,
        file_path,
        Some(parser),
        move |content, file_path| {
            with_thread_allocator(|allocator| {
                let source_type = source_type_for(&dialect, file_path, content);
                let errors = Parser::new(allocator, content, source_type)
                    .parse()
                    .diagnostics;
                let error = errors.first()?;
                let line = error
                    .labels
                    .first()
                    .and_then(|label| content.get(..usize::try_from(label.offset()).ok()?))
                    .map(|before| crate::num::saturating_u32(before.matches('\n').count()) + 1);
                Some((error.message.to_string(), line))
            })
        },
    )
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
