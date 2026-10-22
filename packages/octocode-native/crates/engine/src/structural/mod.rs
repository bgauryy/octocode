//! Structural (AST) search over the tree-sitter grammars we already link.
//!
//! This is octocode's L2 search layer: it answers shape questions text search
//! can't (a call shaped `foo($X)`, an `eval()` call site that is NOT inside a
//! comment/string) and that LSP is too heavy for. The default matcher is
//! Octocode-owned; the grammars are the exact `tree_sitter::Language` values in
//! [`crate::signatures::languages`] — no second grammar set, no link collision.

mod files;
pub(crate) mod kinds;
mod language;
mod octo;
mod query;
mod syntax_errors;
mod syntax_tree;
mod types;

#[cfg(test)]
use files::search_files_detailed;
pub use files::{Rebuilt, file_matches, search_files};
pub use octo::{
    COMPILE_FAILED, EXAMPLE_RULE, MATCH_DEADLINE, PARSE_INTERRUPTED, PatternDeclaration,
    RULE_INVALID, validate_rule_shape,
};
pub use query::QUERY_INVALID;
pub use syntax_errors::count_syntax_errors;
pub use syntax_tree::{
    SyntaxTreeInspectOptions, SyntaxTreeInspectResult, SyntaxTreeNode, inspect_with_extension,
};
#[cfg(test)]
use types::StructuralMatch;
pub use types::{
    FileMatches, StructuralDemand, StructuralDetailedMatch, StructuralDiagnostic,
    StructuralSearchDetailedResult, StructuralSearchFilesDetailedResult,
    StructuralSearchFilesOptions,
};

use crate::signatures::languages;
use language::AgLanguage;
#[cfg(test)]
use octo::all_matches;
use octo::{compile_matcher, matches_with_recovery};
use query::{StructuralQuery, invalid_query_explanation};

/// Defense-in-depth cap on content handed to the single-content structural
/// entry points (`search`, `search_detailed`). The file walker already bounds
/// per-file bytes via `max_file_bytes`; this mirrors that backstop on the
/// single-content path, so a multi-MB blob can't hang tree-sitter parsing or `match_multi_capture` backtracking with no
/// timeoutMs escape. At-or-below passes; over returns an error / `truncated`.
const MAX_STRUCTURAL_CONTENT_BYTES: usize = crate::signatures::MAX_PARSE_SIZE;

/// Test helper: a structural search over `content`, parsed with the grammar
/// resolved from `ext`, returning bare matches. Exactly one of `pattern` /
/// `rule` must be `Some`. Production runs [`search_detailed`].
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule YAML, or both/neither query supplied.
#[cfg(test)]
pub(crate) fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<StructuralMatch>, String> {
    if content.len() > MAX_STRUCTURAL_CONTENT_BYTES {
        return Err(format!(
            "[structural.content.tooLarge] structural search content exceeds {MAX_STRUCTURAL_CONTENT_BYTES} byte limit"
        ));
    }
    let query = StructuralQuery::new(pattern, rule)
        .map_err(|message| format!("[structural.query.invalid] {message}"))?;
    let entry = languages::find_entry(ext).ok_or_else(|| {
        format!("[structural.language.unsupported] structural search does not support .{ext} files")
    })?;
    let lang = AgLanguage::new(entry);
    let run = compile_matcher(&lang, &query).map_err(|error| error.to_string())?;
    // The non-detailed API returns bare StructuralMatch; node_kind is only
    // surfaced by the detailed shape.
    Ok(all_matches(&run, content)
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(|m| m.matched)
        .collect())
}

/// The declaration a structural `pattern` spells in the grammar of `ext`
/// (a node the grammar names by a `name` field: a function, a type, a
/// method), with its name unless that is a metavariable; `None` for any other
/// pattern, an unknown grammar, or a pattern that does not compile.
pub fn pattern_declaration(pattern: &str, ext: &str) -> Option<PatternDeclaration> {
    let entry = languages::find_entry(ext)?;
    octo::pattern_declaration(&AgLanguage::new(entry), pattern)
}

/// A single-content result that ends before any match: `status` with one
/// `diagnostic` saying why.
fn no_matches(
    file_path: &str,
    status: &str,
    language_id: Option<String>,
    query: types::StructuralQueryExplanation,
    diagnostic: StructuralDiagnostic,
) -> StructuralSearchDetailedResult {
    StructuralSearchDetailedResult {
        path: file_path.to_owned(),
        status: status.to_owned(),
        language_id,
        query,
        matches: Vec::new(),
        recovered: false,
        diagnostics: vec![diagnostic],
    }
}

/// Match one source using the selected grammar, or the path's extension.
pub fn search_detailed(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> StructuralSearchDetailedResult {
    let extension = extension.map_or_else(
        || crate::text::file_extension::extension_of(file_path, true, "txt"),
        str::to_owned,
    );
    evaluate_source(content, file_path, &extension, pattern, rule)
}

pub(crate) fn evaluate_source(
    content: &str,
    file_path: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> StructuralSearchDetailedResult {
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
        .with_path(file_path);
        return no_matches(
            file_path,
            "truncated",
            None,
            invalid_query_explanation(pattern, rule, "content exceeds single-content byte limit"),
            diagnostic,
        );
    }
    let query = match StructuralQuery::new(pattern, rule) {
        Ok(query) => query,
        Err(message) => {
            let diagnostic = query::invalid_query_diagnostic(message.clone()).with_path(file_path);
            return no_matches(
                file_path,
                "parserFailed",
                None,
                invalid_query_explanation(pattern, rule, &message),
                diagnostic,
            );
        }
    };

    let query_explanation = query.explanation();
    let Some(entry) = languages::find_entry(ext) else {
        let diagnostic = files::unsupported_language(file_path);
        return no_matches(
            file_path,
            "unsupported",
            None,
            query_explanation,
            diagnostic,
        );
    };

    let lang = AgLanguage::new(entry);
    let run = match compile_matcher(&lang, &query) {
        Ok(run) => run,
        Err(error) => {
            if let Some(interruption) = error.interruption() {
                return no_matches(
                    file_path,
                    "truncated",
                    entry.language_id.map(str::to_owned),
                    query_explanation,
                    interruption.diagnostic(file_path),
                );
            }
            let diagnostic = error.diagnostic().with_path(file_path);
            return no_matches(
                file_path,
                "parserFailed",
                entry.language_id.map(str::to_owned),
                query_explanation,
                diagnostic,
            );
        }
    };

    let (matches, recovered) = match matches_with_recovery(&run, content) {
        Ok(found) => found,
        Err(error) => {
            return no_matches(
                file_path,
                "truncated",
                entry.language_id.map(str::to_owned),
                query_explanation,
                error.diagnostic(file_path),
            );
        }
    };
    let matches = matches
        .into_iter()
        .map(|m| StructuralDetailedMatch::from_match(m.matched, m.node_kind))
        .collect();

    StructuralSearchDetailedResult {
        path: file_path.to_owned(),
        status: "ok".to_owned(),
        language_id: entry.language_id.map(str::to_owned),
        query: query_explanation,
        matches,
        recovered,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
