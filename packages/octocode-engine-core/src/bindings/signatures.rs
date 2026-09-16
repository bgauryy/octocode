use crate::bindings::tasks::{
    SemanticBoundaryOffsetsTask, StructuralSearchFilesTask, StructuralSearchTask,
    SyntaxTreeInspectTask,
};
use napi::bindgen_prelude::AsyncTask;
use napi::Result;
use napi_derive::napi;

/// Native exports.
#[napi]
pub const SIGNATURES_ONLY_HINT: &str = crate::signatures::SIGNATURES_ONLY_HINT;

/// Structural skeleton with an `NNN| ` line-number gutter, produced purely by
/// tree-sitter parsing (no regex heuristics). Returns `null` for data/config
/// formats, any language without a wired grammar, content above the 1MB guard,
/// and any skeleton that would not be smaller than the source.
#[napi(js_name = "extractSignatures")]
pub fn extract_signatures(content: String, file_path: String) -> Option<String> {
    crate::portable::extract_signatures(&content, &file_path)
}

/// Native JS/TS document symbols (server-free) as a JSON `DocumentSymbol[]`.
///
/// Parses ECMAScript/TypeScript *syntax* with `oxc_parser` and walks
/// declarations into the LSP `DocumentSymbol` shape (nested, numeric
/// `SymbolKind`, 0-based UTF-16 ranges). **No type inference** — in-file
/// scope/binding accuracy only; type-aware outlines still require a language
/// server. Only `ts/tsx/js/jsx/mjs/cjs/mts/cts` are handled.
///
/// Returns `null` for non-JS/TS files, oversized content, a hard parse failure
/// (caller should fall back to `extractSignatures`/tree-sitter), or a file with
/// no extractable top-level symbols.
#[napi(js_name = "extractJsSymbols")]
pub fn extract_js_symbols(content: String, file_path: String) -> Option<String> {
    crate::portable::extract_js_symbols(&content, &file_path)
}

/// Canonical list of file extensions (lowercase, no leading dot) handled by the
/// native oxc JS/TS path (`extractJsSymbols` / `findInFileReferences`). Callers
/// should gate native dispatch on this list rather than hardcoding it, so the
/// Rust and JS sides never drift.
#[napi(js_name = "getSupportedJsTsExtensions")]
pub fn get_supported_js_ts_extensions() -> Vec<String> {
    crate::portable::supported_js_ts_extensions()
}

/// Native in-file references (server-free) for the JS/TS symbol under
/// `(line, character)` (0-based, UTF-16), as a JSON `Range[]` covering the
/// declaration and every resolved in-file reference (declaration first).
///
/// **Same-file only.** oxc resolves bindings within one module; cross-file
/// references require a language server. No type inference. Returns `null` for
/// non-JS/TS files, oversized content, a parse failure, or when the cursor is
/// not on a resolvable binding/reference.
#[napi(js_name = "findInFileReferences")]
pub fn find_in_file_references(
    content: String,
    file_path: String,
    line: u32,
    character: u32,
) -> Option<String> {
    crate::portable::find_in_file_references(&content, &file_path, line, character)
}

/// Native graph facts as JSON.
///
/// Uses the richer OXC JS/TS graph extractor when available, then falls back to
/// generic tree-sitter graph facts for other source languages. Syntax-level
/// only: declarations, imports, exports, containment, and direct call edges.
/// Cross-file identity and type-aware proof remain LSP work.
#[napi(js_name = "extractGraphFacts")]
pub fn extract_graph_facts(content: String, file_path: String) -> Option<String> {
    crate::portable::extract_graph_facts(&content, &file_path)
}

/// Canonical lowercase extensions (no leading dot) that can produce native
/// `GraphFacts`. JS/TS use the OXC lane; other entries use tree-sitter syntax
/// inventory. LSP proof is still required for semantic reference certainty.
#[napi(js_name = "getSupportedGraphFactExtensions")]
pub fn get_supported_graph_fact_extensions() -> Vec<String> {
    crate::portable::supported_graph_fact_extensions()
}

/// JSON array describing graph fact coverage by extension/language. This is an
/// agent-facing capability matrix, not proof that every fact family is complete.
#[napi(js_name = "getGraphFactCapabilities")]
pub fn get_graph_fact_capabilities() -> String {
    crate::portable::graph_fact_capabilities()
}

/// Canonical parser-family inventory for runtime selectors and agent context.
/// Names, aliases, extensions, and capability flags all come from the same
/// registry used by structural search, signatures, graph facts, and LSP grammar
/// detection; consumers must not maintain a parallel language table.
#[napi(js_name = "getGrammarCapabilities")]
pub fn get_grammar_capabilities() -> Vec<crate::types::GrammarCapability> {
    crate::portable::grammar_capabilities()
}

/// Structural (AST) search — octocode's L2 layer. Resolves the grammar from
/// `file_path`'s extension and matches a code-shaped `pattern` OR a YAML `rule`
/// (exactly one). Returns node ranges (1-based lines, ready as `lineHint`s)
/// plus captured metavariables. Throws on unsupported extension, invalid
/// pattern/rule, or both/neither query supplied.
///
/// Runs on libuv's worker pool (parsing is tree-sitter, CPU-bound) so a large
/// file never blocks the event loop. Returns a Promise from JavaScript.
#[napi(js_name = "structuralSearch")]
pub fn structural_search(
    content: String,
    file_path: String,
    pattern: Option<String>,
    rule: Option<String>,
) -> AsyncTask<StructuralSearchTask> {
    AsyncTask::new(StructuralSearchTask {
        content,
        file_path,
        pattern,
        rule,
    })
}

/// Detailed structural search. Unlike `structuralSearch`, unsupported
/// extensions and invalid queries are represented as status + diagnostics so
/// callers can distinguish true empty results from weak evidence.
#[napi(js_name = "structuralSearchDetailed")]
pub fn structural_search_detailed(
    content: String,
    file_path: String,
    pattern: Option<String>,
    rule: Option<String>,
) -> Result<crate::structural::StructuralSearchDetailedResult> {
    Ok(crate::portable::structural_search_detailed(
        &content,
        &file_path,
        pattern.as_deref(),
        rule.as_deref(),
    )?)
}

/// Runs on libuv's worker pool — the directory walk + per-file parse is
/// CPU/IO-bound and can span thousands of files. Returns a Promise from
/// JavaScript.
#[napi(js_name = "structuralSearchFiles")]
pub fn structural_search_files(
    options: crate::structural::StructuralSearchFilesOptions,
) -> AsyncTask<StructuralSearchFilesTask> {
    AsyncTask::new(StructuralSearchFilesTask {
        options: Some(options),
    })
}

#[napi(js_name = "structuralSearchFilesDetailed")]
pub fn structural_search_files_detailed(
    options: crate::structural::StructuralSearchFilesOptions,
) -> Result<crate::structural::StructuralSearchFilesDetailedResult> {
    Ok(crate::portable::structural_search_files_detailed(options)?)
}

#[napi(js_name = "getSupportedStructuralExtensions")]
pub fn get_supported_structural_extensions() -> Vec<String> {
    crate::portable::supported_structural_extensions()
}

/// Return a bounded, paginated syntax-tree view using the structural grammar
/// registry. Parsing and traversal run on libuv's worker pool.
#[napi(js_name = "inspectSyntaxTree")]
pub fn inspect_syntax_tree(
    content: String,
    file_path: String,
    options: Option<crate::structural::SyntaxTreeInspectOptions>,
) -> AsyncTask<SyntaxTreeInspectTask> {
    AsyncTask::new(SyntaxTreeInspectTask {
        content,
        file_path,
        options,
    })
}

/// Returns a sorted list of JS char offsets (UTF-16 code units) where
/// top-level semantic blocks begin in `content`.
///
/// Uses registered Tree-sitter body queries; see `getSupportedSignatureExtensions`
/// for the compiled language set. Unsupported and structural-only languages
/// return `[]`, as do plain text and files above the 1 MB guard.
///
/// Char offsets match JavaScript `string.substring()` — pass them directly to
/// JavaScript string slicing without conversion.
///
/// Runs on libuv's worker pool (tree-sitter parse is CPU-bound) so a large file
/// never blocks the event loop. Returns a Promise from JavaScript.
#[napi(js_name = "getSemanticBoundaryOffsets")]
pub fn get_semantic_boundary_offsets(
    content: String,
    file_path: String,
) -> AsyncTask<SemanticBoundaryOffsetsTask> {
    AsyncTask::new(SemanticBoundaryOffsetsTask { content, file_path })
}

/// Returns all extensions that have signature-outline support. This is exactly
/// the set of tree-sitter grammars with a function-body query (no regex
/// heuristics): structural-only grammars (for example HTML/CSS/JSON/YAML) are
/// excluded because they produce no outline.
#[napi(js_name = "getSupportedSignatureExtensions")]
pub fn get_supported_signature_extensions() -> Vec<String> {
    crate::portable::supported_signature_extensions()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_signature_extensions_are_tree_sitter_only_and_sorted() {
        let exts = get_supported_signature_extensions();
        // Languages that must have signature extraction (body_query set)
        for required in ["ts", "py", "rs", "go", "java", "rb", "php", "kt"] {
            assert!(
                exts.iter().any(|e| e == required),
                "missing {required} from signature list"
            );
        }
        for (enabled, optional) in [
            (cfg!(feature = "tree-sitter-cpp"), &["cpp", "hpp"][..]),
            (cfg!(feature = "tree-sitter-c-sharp"), &["cs"][..]),
            (cfg!(feature = "tree-sitter-swift"), &["swift"][..]),
            (
                cfg!(feature = "tree-sitter-extended"),
                &["scala", "sc", "sbt"][..],
            ),
        ] {
            for extension in optional {
                assert_eq!(
                    exts.iter().any(|ext| ext == extension),
                    enabled,
                    ".{extension} must reflect its compiled grammar feature"
                );
            }
        }
        // Languages that must NOT have signature extraction (no body_query)
        for absent in [
            "vue", "svelte", "md", "markdown", "sql", "html", "jl", "ml", "ex", "exs", "tf", "hcl",
            "tfvars", "proto",
        ] {
            assert!(
                !exts.iter().any(|e| e == absent),
                "{absent} must not have a signature outline (no grammar / structural-only)"
            );
        }
        let mut sorted = exts.clone();
        sorted.sort();
        assert_eq!(exts, sorted, "extension list must be sorted");
    }

    #[test]
    fn grammar_capabilities_are_complete_and_preserve_family_aliases() {
        let capabilities = get_grammar_capabilities();
        let extensions: Vec<&str> = capabilities
            .iter()
            .flat_map(|capability| capability.extensions.iter().map(String::as_str))
            .collect();
        assert_eq!(
            extensions.len(),
            crate::signatures::languages::supported_extensions().len()
        );
        assert_eq!(
            extensions
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            extensions.len(),
            "every extension must belong to exactly one parser entry"
        );

        let kotlin = capabilities
            .iter()
            .find(|capability| capability.language == "Kotlin")
            .expect("Kotlin capability");
        assert_eq!(kotlin.extensions, ["kt", "kts"]);
        assert!(kotlin.structural_search && kotlin.signature_outline && kotlin.graph_facts);

        let ruby = capabilities
            .iter()
            .find(|capability| capability.language == "Ruby")
            .expect("Ruby capability");
        assert_eq!(ruby.extensions, ["rb", "rake", "gemspec", "ru"]);

        let tsx = capabilities
            .iter()
            .find(|capability| capability.language == "TSX")
            .expect("TSX capability");
        assert!(tsx
            .selector_aliases
            .iter()
            .any(|alias| alias == "typescript"));
    }
}
