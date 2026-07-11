//! Structural (AST) search over the tree-sitter grammars we already link.
//!
//! This is octocode's L2 search layer: it answers shape questions ripgrep
//! can't (a call shaped `foo($X)`, an `eval()` call site that is NOT inside a
//! comment/string) and that LSP is too heavy for. The default matcher is
//! Octocode-owned; the grammars are the exact `tree_sitter::Language` values in
//! [`crate::signatures::languages`] — no second grammar set, no link collision.

mod files;
mod language;
mod octo;
mod query;
mod types;

pub use files::{search_files, search_files_detailed};
pub use types::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralMatch, StructuralSearchDetailedResult,
    StructuralSearchFilesDetailedResult, StructuralSearchFilesOptions, StructuralSearchFilesResult,
};

use crate::signatures::languages;
use language::AgLanguage;
use octo::compile_matcher;
use query::{invalid_query_explanation, StructuralQuery};
use types::{structural_query_fingerprint, STRUCTURAL_ANALYZER, STRUCTURAL_ANALYZER_VERSION};

/// Defense-in-depth cap on content handed to the single-content structural
/// entry points (`search`, `search_detailed`). The file walker already bounds
/// per-file bytes via `max_file_bytes`; this mirrors that backstop on the path
/// OQL continuations and the public napi export hand off to, so a multi-MB blob
/// can't hang tree-sitter parsing or `match_multi_capture` backtracking with no
/// timeoutMs escape. At-or-below passes; over returns an error / `truncated`.
const MAX_STRUCTURAL_CONTENT_BYTES: usize = 1_000_000;

/// Run a structural search over `content`, parsed with the grammar resolved
/// from `ext`. Exactly one of `pattern` / `rule` must be `Some`.
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule YAML, or both/neither query supplied — the napi layer maps these to a
/// JS error so the caller can surface guidance instead of a silent empty set.
pub fn supported_extensions() -> Vec<String> {
    languages::supported_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<StructuralMatch>, String> {
    if content.len() > MAX_STRUCTURAL_CONTENT_BYTES {
        return Err(format!(
            "structural search content exceeds {MAX_STRUCTURAL_CONTENT_BYTES} byte limit"
        ));
    }
    let query = StructuralQuery::new(pattern, rule)?;
    let entry = languages::find_entry(ext)
        .ok_or_else(|| format!("structural search does not support .{ext} files"))?;
    let lang = AgLanguage::new(ext, entry);
    let run = compile_matcher(&lang, query)?;
    // The non-detailed API returns bare StructuralMatch; node_kind is only
    // surfaced by the detailed shape.
    Ok(run(content).into_iter().map(|m| m.matched).collect())
}

pub fn search_detailed(
    content: &str,
    file_path: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> StructuralSearchDetailedResult {
    let query_fingerprint = structural_query_fingerprint(pattern, rule);
    if content.len() > MAX_STRUCTURAL_CONTENT_BYTES {
        let diagnostic = StructuralDiagnostic::new(
            "structural.content.tooLarge",
            "warning",
            "parse",
            format!(
                "Structural search content is {} bytes, above the single-content limit of {MAX_STRUCTURAL_CONTENT_BYTES} bytes.",
                content.len()
            ),
        )
        .with_path(file_path)
        .with_recovery("Scope the content with fetch.content charLength/charOffset or run a file search with maxFileBytes instead.");
        return StructuralSearchDetailedResult {
            path: file_path.to_owned(),
            analyzer: STRUCTURAL_ANALYZER.to_owned(),
            analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
            status: "truncated".to_owned(),
            language_id: None,
            query: invalid_query_explanation(
                pattern,
                rule,
                "content exceeds single-content byte limit",
            ),
            matches: Vec::new(),
            diagnostics: vec![diagnostic],
        };
    }
    let query = match StructuralQuery::new(pattern, rule) {
        Ok(query) => query,
        Err(message) => {
            let diagnostic = StructuralDiagnostic::new(
                "structural.query.invalid",
                "error",
                "match",
                message.clone(),
            )
            .with_path(file_path)
            .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.");
            return StructuralSearchDetailedResult {
                path: file_path.to_owned(),
                analyzer: STRUCTURAL_ANALYZER.to_owned(),
                analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
                status: "parserFailed".to_owned(),
                language_id: None,
                query: invalid_query_explanation(pattern, rule, &message),
                matches: Vec::new(),
                diagnostics: vec![diagnostic],
            };
        }
    };

    let query_explanation = query.explanation();
    let Some(entry) = languages::find_entry(ext) else {
        let diagnostic = StructuralDiagnostic::new(
            "structural.language.unsupported",
            "warning",
            "parse",
            format!("Structural search does not support .{ext} files."),
        )
        .with_path(file_path)
        .with_recovery("Use text search for this extension or add a tree-sitter grammar mapping.");
        return StructuralSearchDetailedResult {
            path: file_path.to_owned(),
            analyzer: STRUCTURAL_ANALYZER.to_owned(),
            analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
            status: "unsupported".to_owned(),
            language_id: None,
            query: query_explanation,
            matches: Vec::new(),
            diagnostics: vec![diagnostic],
        };
    };

    let lang = AgLanguage::new(ext, entry);
    let run = match compile_matcher(&lang, query) {
        Ok(run) => run,
        Err(message) => {
            let diagnostic = StructuralDiagnostic::new(
                "structural.query.compileFailed",
                "error",
                "match",
                message.clone(),
            )
            .with_path(file_path)
            .with_recovery(
                "Check the structural pattern or YAML rule against this file's language grammar.",
            );
            return StructuralSearchDetailedResult {
                path: file_path.to_owned(),
                analyzer: STRUCTURAL_ANALYZER.to_owned(),
                analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
                status: "parserFailed".to_owned(),
                language_id: entry.language_id.map(str::to_owned),
                query: query_explanation,
                matches: Vec::new(),
                diagnostics: vec![diagnostic],
            };
        }
    };

    let matches = run(content)
        .into_iter()
        .map(|m| {
            StructuralDetailedMatch::from_match(
                file_path,
                &query_fingerprint,
                m.matched,
                m.node_kind,
            )
        })
        .collect();

    StructuralSearchDetailedResult {
        path: file_path.to_owned(),
        analyzer: STRUCTURAL_ANALYZER.to_owned(),
        analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
        status: "ok".to_owned(),
        language_id: entry.language_id.map(str::to_owned),
        query: query_explanation,
        matches,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
