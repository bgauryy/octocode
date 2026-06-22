use napi::{Error, Result, Status};
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
    crate::signatures::extract_signatures_inner(&content, &file_path)
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
    crate::signatures::js_oxc::extract_js_symbols(&content, &file_path)
}

/// Canonical list of file extensions (lowercase, no leading dot) handled by the
/// native oxc JS/TS path (`extractJsSymbols` / `findInFileReferences`). Callers
/// should gate native dispatch on this list rather than hardcoding it, so the
/// Rust and JS sides never drift.
#[napi(js_name = "getSupportedJsTsExtensions")]
pub fn get_supported_js_ts_extensions() -> Vec<String> {
    crate::file_extension::JS_TS_EXTENSIONS
        .iter()
        .map(|ext| (*ext).to_owned())
        .collect()
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
    crate::signatures::js_oxc::find_in_file_references(&content, &file_path, line, character)
}

/// Native graph facts as JSON.
///
/// Uses the richer OXC JS/TS graph extractor when available, then falls back to
/// generic tree-sitter graph facts for other source languages. Syntax-level
/// only: declarations, imports, exports, containment, and direct call edges.
/// Cross-file identity and type-aware proof remain LSP work.
#[napi(js_name = "extractGraphFacts")]
pub fn extract_graph_facts(content: String, file_path: String) -> Option<String> {
    crate::signatures::js_oxc::extract_graph_facts(&content, &file_path)
        .or_else(|| crate::signatures::graph_facts::extract_graph_facts(&content, &file_path))
}

/// Canonical lowercase extensions (no leading dot) that can produce native
/// `GraphFacts`. JS/TS use the OXC lane; other entries use tree-sitter syntax
/// inventory. LSP proof is still required for semantic reference certainty.
#[napi(js_name = "getSupportedGraphFactExtensions")]
pub fn get_supported_graph_fact_extensions() -> Vec<String> {
    crate::signatures::graph_facts::graph_fact_extensions()
}

/// JSON array describing graph fact coverage by extension/language. This is an
/// agent-facing capability matrix, not proof that every fact family is complete.
#[napi(js_name = "getGraphFactCapabilities")]
pub fn get_graph_fact_capabilities() -> String {
    crate::signatures::graph_facts::graph_fact_capabilities_json()
}

/// Structural (AST) search — octocode's L2 layer. Resolves the grammar from
/// `file_path`'s extension and matches a code-shaped `pattern` OR a YAML `rule`
/// (exactly one). Returns node ranges (1-based lines, ready as `lineHint`s)
/// plus captured metavariables. Throws on unsupported extension, invalid
/// pattern/rule, or both/neither query supplied.
#[napi(js_name = "structuralSearch")]
pub fn structural_search(
    content: String,
    file_path: String,
    pattern: Option<String>,
    rule: Option<String>,
) -> Result<Vec<crate::structural::StructuralMatch>> {
    let ext = crate::file_extension::get_extension_internal(&file_path, true, "txt");
    // Contain tree-sitter matcher panics on pathological input: an unwind
    // across the napi FFI boundary would abort the Node process. Mirror the
    // panic guards used in `apply.rs` / signature extraction.
    let outcome = std::panic::catch_unwind(|| {
        crate::structural::search(&content, &ext, pattern.as_deref(), rule.as_deref())
    })
    .unwrap_or_else(|_| Err("structural search failed on pathological input".to_string()));
    outcome.map_err(|message| Error::new(Status::InvalidArg, message))
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
    let ext = crate::file_extension::get_extension_internal(&file_path, true, "txt");
    std::panic::catch_unwind(|| {
        crate::structural::search_detailed(
            &content,
            &file_path,
            &ext,
            pattern.as_deref(),
            rule.as_deref(),
        )
    })
    .map_err(|_| {
        Error::new(
            Status::GenericFailure,
            "structural detailed search failed on pathological input",
        )
    })
}

#[napi(js_name = "structuralSearchFiles")]
pub fn structural_search_files(
    options: crate::structural::StructuralSearchFilesOptions,
) -> Result<crate::structural::StructuralSearchFilesResult> {
    std::panic::catch_unwind(|| crate::structural::search_files(options))
        .unwrap_or_else(|_| Err("structural file search failed on pathological input".to_string()))
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

#[napi(js_name = "structuralSearchFilesDetailed")]
pub fn structural_search_files_detailed(
    options: crate::structural::StructuralSearchFilesOptions,
) -> Result<crate::structural::StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(|| crate::structural::search_files_detailed(options))
        .unwrap_or_else(|_| {
            Err("structural detailed file search failed on pathological input".to_string())
        })
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

#[napi(js_name = "getSupportedStructuralExtensions")]
pub fn get_supported_structural_extensions() -> Vec<String> {
    crate::structural::supported_extensions()
}

/// Returns a sorted list of JS char offsets (UTF-16 code units) where
/// top-level semantic blocks begin in `content`.
///
/// **Tree-sitter only** (exact AST): `ts tsx js jsx mjs cjs mts cts py pyi go rs
///   java c h cpp cc cxx hpp hh hxx cs sh bash zsh`. Languages without a wired
/// grammar and structural-only grammars (HTML/CSS/Scala/JSON/YAML/TOML) return
/// `[]` — there is no regex/heuristic fallback. Also `[]` for data/config files,
/// plain text, and files above the 1 MB guard.
///
/// Char offsets match JavaScript `string.substring()` — pass them directly to
/// JavaScript string slicing without conversion.
#[napi(js_name = "getSemanticBoundaryOffsets")]
pub fn get_semantic_boundary_offsets(content: String, file_path: String) -> Vec<u32> {
    crate::signatures::get_semantic_boundary_offsets_inner(&content, &file_path)
}

/// Returns all extensions that have signature-outline support. This is exactly
/// the set of tree-sitter grammars with a function-body query (no regex
/// heuristics): structural-only grammars (HTML/CSS/Scala/JSON/YAML/TOML) are
/// excluded because they produce no outline.
#[napi(js_name = "getSupportedSignatureExtensions")]
pub fn get_supported_signature_extensions() -> Vec<String> {
    let mut exts: Vec<String> = crate::signatures::languages::signature_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    exts.sort();
    exts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_signature_extensions_are_tree_sitter_only_and_sorted() {
        let exts = get_supported_signature_extensions();
        // Languages that must have signature extraction (body_query set)
        for required in [
            "ts", "py", "rs", "go", "java", "rb", "php", "kt", "ex", "lua", "erl", "zig", "r",
            "swift", "scala", "sc", "sbt", "tf", "hcl", "tfvars", "proto",
        ] {
            assert!(
                exts.iter().any(|e| e == required),
                "missing {required} from signature list"
            );
        }
        // Languages that must NOT have signature extraction (no body_query)
        for absent in ["vue", "svelte", "md", "markdown", "sql", "html", "jl", "ml"] {
            assert!(
                !exts.iter().any(|e| e == absent),
                "{absent} must not have a signature outline (no grammar / structural-only)"
            );
        }
        let mut sorted = exts.clone();
        sorted.sort();
        assert_eq!(exts, sorted, "extension list must be sorted");
    }
}
