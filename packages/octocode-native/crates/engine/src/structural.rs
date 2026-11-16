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
mod parse_cost;
mod query;
mod syntax_errors;
mod syntax_tree;
mod types;

pub use files::{Rebuilt, file_matches, search_files};
#[cfg(test)]
use files::{search_files_detailed, search_files_unfiltered};
pub use octo::{
    COMPILE_FAILED, EXAMPLE_RULE, LiteralSigil, MATCH_DEADLINE, PARSE_INTERRUPTED,
    PatternDeclaration, QualifiedPattern, RULE_INVALID, SigilForm, SigilRepair, WrittenPattern,
};
pub use parse_cost::{BYTES_PER_TOKEN, parse_budget, parse_cost};
pub use query::QUERY_INVALID;
pub use syntax_errors::count_syntax_errors;
pub use syntax_tree::{
    SyntaxTreeInspectOptions, SyntaxTreeInspectResult, SyntaxTreeNode, inspect_with_extension,
};
#[cfg(test)]
use types::StructuralMatch;
pub use types::{
    FileMatches, JudgeText, KeepLine, RecoveredBody, StructuralDemand, StructuralDetailedMatch,
    StructuralDiagnostic, StructuralSearchDetailedResult, StructuralSearchFilesDetailedResult,
    StructuralSearchFilesOptions, TextJudge, WalkErrors,
};

use crate::security::MAX_CONTENT_SIZE;
use crate::signatures::languages;
use language::AgLanguage;
#[cfg(test)]
use octo::all_matches;
use octo::{compile_matcher, matches_with_recovery};
use query::{StructuralQuery, invalid_query_explanation};
use serde_json::Value;

/// Test helper: a structural search over `content`, parsed with the grammar
/// resolved from `ext`, returning bare matches. Exactly one of `pattern` /
/// `rule` must be `Some`. Production runs [`search_detailed`].
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule object, or both/neither query supplied.
#[cfg(test)]
pub(crate) fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&Value>,
) -> Result<Vec<StructuralMatch>, String> {
    if content.len() > MAX_CONTENT_SIZE {
        return Err(format!(
            "[structural.content.tooLarge] structural search content exceeds {MAX_CONTENT_SIZE} byte limit"
        ));
    }
    let query = StructuralQuery::new(pattern, rule)
        .map_err(|message| format!("[structural.query.invalid] {message}"))?;
    let entry = languages::find_entry(ext)
        .filter(|entry| entry.admits(crate::text::analysis::AnalysisOperation::StructuralSearch))
        .ok_or_else(|| {
            format!(
                "[structural.language.unsupported] structural search does not support .{ext} files"
            )
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
/// (a function, a type, a method, a binding), with its name unless that is a
/// metavariable; `None` for any other
/// pattern, an unknown grammar, or a pattern that does not compile.
pub fn pattern_declaration(pattern: &str, ext: &str) -> Option<PatternDeclaration> {
    let entry = languages::find_entry(ext)?;
    octo::pattern_declaration(&AgLanguage::new(entry), pattern)
}

/// The fields (`type_parameters`, `return_type`) in which the declaration of
/// `pattern`'s kind that starts on 1-based `line` of `content`, in the
/// grammar of `ext`, differs from the pattern: the part there is absent or
/// is not what the pattern spells. Empty when the pattern spells no
/// declaration, no node of its kind starts there, or no field-held part
/// differs.
pub fn pattern_differences(pattern: &str, ext: &str, content: &str, line: u32) -> Vec<String> {
    languages::find_entry(ext)
        .map(|entry| octo::pattern_differences(&AgLanguage::new(entry), pattern, content, line))
        .unwrap_or_default()
}

/// The kind of node a structural `pattern` parses as in the grammar of
/// `ext`; `None` when it does not compile or is a special pattern.
pub fn pattern_kind(pattern: &str, ext: &str) -> Option<String> {
    let entry = languages::find_entry(ext)?;
    octo::pattern_kind(&AgLanguage::new(entry), pattern)
}

/// The literal tokens of a structural `pattern` in the grammar of `ext` that
/// spell a metavariable sigil but are no metavariable: one glued to other
/// text (`search_$X`, one identifier) or a lower-case name (`$x`,
/// `$$$args`). Read from the tree the matcher compiles: a `{context,
/// selector}` pattern's whole context, the selected node included. None for
/// an unknown grammar or a pattern that does not compile.
pub fn literal_sigils(pattern: WrittenPattern<'_>, ext: &str) -> Vec<LiteralSigil> {
    languages::find_entry(ext)
        .map(|entry| octo::literal_sigils(&AgLanguage::new(entry), pattern))
        .unwrap_or_default()
}

/// The rewrite of what the caller wrote of `pattern` (the text, or the
/// context) that matches what its [`literal_sigils`] meant in the grammar of
/// `ext`, when it is exact: each lower-case name upper-cased, and a glued
/// head token (`search_$X`) replaced by a capture whose text the regex tests
/// (`^search_`).
pub fn sigil_repair(pattern: WrittenPattern<'_>, ext: &str) -> Option<SigilRepair> {
    let entry = languages::find_entry(ext)?;
    octo::sigil_repair(&AgLanguage::new(entry), pattern)
}

/// How a structural `pattern` parsed in the grammar of `ext` (a `{context,
/// selector}` pattern: its selected node), as a one-line S-expression of
/// named kinds, field names and metavariables: `$X.unwrap($Y)` in Rust is
/// `(call_expression function:(field_expression value:$X
/// field:(field_identifier "unwrap")) arguments:(arguments $Y))`. `None`
/// when it does not compile or is a special pattern.
pub fn pattern_shape(pattern: WrittenPattern<'_>, ext: &str) -> Option<String> {
    let entry = languages::find_entry(ext)?;
    octo::pattern_shape(&AgLanguage::new(entry), pattern)
}

/// Grammars whose paths qualify with `::`: there a longer path before a
/// pattern's leftmost segment is a namespace qualification, not a member
/// receiver as `.` can be. Every other grammar qualifies with `.`.
const QUALIFIED_PATH_GRAMMARS: [&str; 3] = ["Rust", "C++", "CUDA"];

/// The longer spellings of a structural `pattern` in the grammar of `ext`,
/// which match the sites the pattern as written leaves out: its literal
/// names with a fresh capture and a separator before them, where
///
/// - a name starts a path (the leftmost segment of a `::` path, or a bare
///   name inside the pattern, such as an `impl`'s trait), with `::` (a
///   grammar with `::` paths only);
/// - a bare name is called, with the grammar's separator (`::` or `.`):
///   the calls that qualify it;
/// - a type name in a type position (an annotation), with `.` (a grammar
///   with `.` paths): the types a namespace or module qualifies.
///
/// `None` for a pattern without such a name, or one that does not compile.
pub fn pattern_qualifier(pattern: &str, ext: &str) -> Option<QualifiedPattern> {
    let entry = languages::find_entry(ext)?;
    let separator = if QUALIFIED_PATH_GRAMMARS.contains(&entry.name) {
        octo::Separator::Path
    } else {
        octo::Separator::Member
    };
    octo::pattern_qualifier(&AgLanguage::new(entry), pattern, separator)
}

/// Every longer spelling of `pattern` in the grammar of `ext`:
/// [`pattern_qualifier`]'s, and in a grammar with `::` paths also the member
/// spelling of a bare callee (`$Q.name(..)`), since a call on a receiver
/// (`x.name(..)`) is a call of that name the bare pattern leaves out too.
/// Empty for a pattern without such a name.
pub fn pattern_qualifiers(pattern: &str, ext: &str) -> Vec<QualifiedPattern> {
    let Some(entry) = languages::find_entry(ext) else {
        return Vec::new();
    };
    let lang = AgLanguage::new(entry);
    let separators: &[octo::Separator] = if QUALIFIED_PATH_GRAMMARS.contains(&entry.name) {
        &[octo::Separator::Path, octo::Separator::Member]
    } else {
        &[octo::Separator::Member]
    };
    separators
        .iter()
        .filter_map(|separator| octo::pattern_qualifier(&lang, pattern, *separator))
        .collect()
}

/// The literals every match of `pattern` (or `rule`) spells, any one of
/// which a reader can look for a match by; `None` when no literal is
/// required or the query is malformed.
pub fn query_anchors(pattern: Option<&str>, rule: Option<&Value>) -> Option<Vec<String>> {
    query::StructuralQuery::new(pattern, rule)
        .ok()?
        .recovery_anchors()
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
        recovered_bodies: Vec::new(),
        diagnostics: vec![diagnostic],
    }
}

/// Match one source using the selected grammar, or the path's extension.
pub fn search_detailed(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    pattern: Option<&str>,
    rule: Option<&Value>,
) -> StructuralSearchDetailedResult {
    let extension = extension.map_or_else(
        || crate::text::extension_of(file_path, true, "txt"),
        str::to_owned,
    );
    evaluate_source(content, file_path, &extension, pattern, rule)
}

/// Restate each text of `matches`, found by matching `masked` (the shown
/// text with each placeholder written over in place, [`JudgeText`]), from
/// `raw`, the text a read shows, at the same bytes: a row shows its
/// source's text as a read does, and is judged by its place there. A text
/// that is not `masked`'s bytes at its span (a node of another unit) stays.
pub fn restore_texts(matches: &mut [StructuralDetailedMatch], masked: &str, raw: &str) {
    let restate = |text: &mut String, range: &crate::text::analysis::ByteRange| {
        let at = range.start_byte as usize..range.end_byte as usize;
        if masked.get(at.clone()) == Some(text.as_str())
            && let Some(own) = raw.get(at)
        {
            own.clone_into(text);
        }
    };
    for matched in matches {
        restate(&mut matched.text, &matched.source_span.range);
        if let (Some(header), Some(span)) = (&mut matched.header, &matched.header_span) {
            restate(header, &span.range);
        }
        for (name, ranges) in &mut matched.metavar_ranges {
            let values = matched.metavars.get_mut(name);
            let mut values = values.map(|values| values.iter_mut());
            for range in ranges {
                let before = range.text.clone();
                restate(&mut range.text, &range.source_span.range);
                if let Some(value) = values.as_mut().and_then(Iterator::next)
                    && *value == before
                {
                    value.clone_from(&range.text);
                }
            }
        }
    }
}

pub(crate) fn evaluate_source(
    content: &str,
    file_path: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&Value>,
) -> StructuralSearchDetailedResult {
    // The single-content path holds a source whole, as every read does: a
    // larger blob is refused here, as the file walker refuses it per file.
    if content.len() > MAX_CONTENT_SIZE {
        let diagnostic = StructuralDiagnostic::new(
            "structural.content.tooLarge",
            "warning",
            "parse",
            format!(
                "Structural search content is {} bytes, above the single-content limit of {MAX_CONTENT_SIZE} bytes.",
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
    let Some(entry) = languages::find_entry(ext)
        .filter(|entry| entry.admits(crate::text::analysis::AnalysisOperation::StructuralSearch))
    else {
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
        recovered: recovered.file,
        recovered_bodies: recovered.bodies,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests;

mod source_binding;
pub use source_binding::SourceBinder;
