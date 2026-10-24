//! Scope resolution and grammar choice: the validated scope, the grammar a
//! query compiles in, and why a pattern cannot apply.
use super::query::MatchQuery;
use super::types::{Scan, Scope};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use crate::tools::ast_rule::{
    GlobParsers, GrammarChoice, RuleError, choose_grammar, compile_check, count_grammars,
};
use octocode_contract_types::ErrorCode;
use octocode_engine::signatures::languages::family_extensions;
use serde_json::{Value, json};

/// The parser extension a scope's files are read with: a single file's own
/// (or its override), else the named or inferred grammar's.
pub(super) fn scope_extension(scope: &Scope) -> Option<String> {
    if scope.is_file {
        return Some(
            super::super::language::parser_override(&scope.canonical, scope.language.as_deref())
                .unwrap_or_else(|| {
                    octocode_engine::text::extension_of(
                        &scope.canonical.to_string_lossy(),
                        true,
                        "",
                    )
                }),
        );
    }
    scope
        .language
        .as_deref()
        .or(scope.inferred.as_deref())
        .and_then(crate::tools::ast_rule::primary_extension)
}

/// How the query fares in one grammar of a directory scan's walk.
pub(super) struct GrammarCompile {
    pub(super) label: String,
    pub(super) extensions: std::collections::BTreeSet<String>,
    /// The walk's files of this grammar.
    pub(super) files: usize,
    /// Whether the query compiles in it, else why not.
    pub(super) outcome: Result<(), RuleError>,
}

/// The query compiled in each grammar the scan's own walk met (its census),
/// once per call. Empty for a single file or an explicit `parser`, whose
/// compile checks are the scope's own.
pub(super) fn compile_in_grammars(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
) -> Vec<GrammarCompile> {
    if scope.is_file || scope.language.is_some() {
        return Vec::new();
    }
    let rule = q.rule();
    count_grammars(scan.census.clone())
        .into_iter()
        .map(|(label, extensions, files)| GrammarCompile {
            outcome: compile_check(&extensions, None, rule.as_ref()),
            label,
            extensions,
            files,
        })
        .collect()
}

/// The grammars of a scope that the query does not compile in, while it
/// compiles in another grammar of the same scope. A query that does not
/// parse as language X cannot match X code: those files are outside what the
/// query applies to, not files left unread, so they make the result neither
/// partial nor failed. They are still named, with the compiler's reason, in a
/// warning. A query that compiles in no grammar of the scope is an error, not
/// this.
#[derive(Default)]
pub(super) struct Inapplicable {
    /// Grammar label → its files in the walk.
    pub(super) languages: std::collections::BTreeMap<String, usize>,
}

impl Inapplicable {
    pub(super) fn of(grammars: &[GrammarCompile]) -> Self {
        if !grammars.iter().any(|grammar| grammar.outcome.is_ok()) {
            return Self::default();
        }
        Self {
            languages: grammars
                .iter()
                .filter(|grammar| grammar.outcome.is_err())
                .map(|grammar| (grammar.label.clone(), grammar.files))
                .collect(),
        }
    }

    /// Whether a file read with `parser` is of a grammar the query does not
    /// apply to.
    pub(super) fn covers(&self, parser: &str) -> bool {
        octocode_engine::signatures::languages::find_entry(parser).is_some_and(|entry| {
            self.languages
                .contains_key(&octocode_engine::signatures::languages::label(entry))
        })
    }

    /// The files of each grammar the query does not compile in, by
    /// grammar: a quiet count, not a problem (a pattern is written in one
    /// grammar); setting `parser` to one of them shows its compile error.
    pub(super) fn counts(&self) -> Option<Value> {
        (!self.languages.is_empty()).then(|| {
            self.languages
                .iter()
                .map(|(language, files)| (language.clone(), json!(files)))
                .collect::<serde_json::Map<_, _>>()
                .into()
        })
    }
}

/// The one grammar a directory scan without `parser` read, from the
/// census of its own walk (no second walk): `None` when it read several (or
/// one the query does not compile in). A first window whose files are all
/// of no supported grammar is `languageRequired`.
pub(super) fn infer_grammar(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    grammars: &[GrammarCompile],
) -> Result<Option<String>, ToolError> {
    if scope.is_file || scope.language.is_some() {
        return Ok(None);
    }
    match choose_grammar(
        grammars
            .iter()
            .map(|grammar| (grammar.label.clone(), grammar.extensions.clone())),
        |language, _| {
            grammars
                .iter()
                .find(|grammar| grammar.label == language)
                .map_or(Ok(()), |grammar| grammar.outcome.clone())
        },
    ) {
        GrammarChoice::One(language) => Ok(Some(language)),
        GrammarChoice::Several(_) | GrammarChoice::Invalid => Ok(None),
        // A walk that admitted no file at all, or withheld some, names no
        // grammar to set: its empty result and disclosures are the answer.
        GrammarChoice::Absent
            if q.scan_offset() == 0 && scan.unparsed.count > 0 && scan.withheld.total() == 0 =>
        {
            Err(ToolError::new(
                ErrorCode::LanguageRequired,
                "No source file of a supported structural grammar was found under path; set parser.",
            ))
        }
        GrammarChoice::Absent => Ok(None),
    }
}

/// Validate the selector and query without narrowing an inferred directory
/// scan to the grammars visible before its file limit.
pub(super) fn resolve_scope(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Scope, ToolError> {
    let requested = q
        .language()
        .as_deref()
        .map(|language| {
            family_extensions(language).ok_or_else(|| {
                ToolError::new(
                    ErrorCode::LanguageUnsupported,
                    crate::tools::ast_rule::unsupported_language_message(language),
                )
            })
        })
        .transpose()?;
    let p = paths
        .validate(q.path())
        .map_err(super::super::errors::policy_error)?;
    let meta = std::fs::metadata(&p.canonical)
        .map_err(|error| super::super::errors::io_error(&q.path(), error))?;
    if q.scope().is_some() && !meta.is_file() {
        return Err(ToolError::new(
            ErrorCode::InvalidInput,
            "scope requires a single source file; use one query per file.",
        ));
    }
    if meta.is_file() && q.language_globs().is_some() {
        return Err(ToolError::new(
            ErrorCode::LanguageDirectoryRequired,
            "parserGlobs is for directory matches. Use parser for a single file.",
        ));
    }
    let parsers = GlobParsers::compile(&p.canonical, q.language_globs().as_ref())
        .map_err(|error| ToolError::graph_scan(&error, cancel))?;
    let extensions = requested.or_else(|| meta.is_dir().then(structural_extensions));
    // Validate against grammar capabilities, not the capped discovery prefix.
    // The literal prefilter may otherwise hide even a globally invalid query.
    if meta.is_dir()
        && let Some(extensions) = &extensions
    {
        compile_check(extensions, None, q.rule().as_ref())
            .map_err(|error| ToolError::new(error.code, error.message))?;
    }
    let language = q.language();
    Ok(Scope {
        canonical: p.canonical,
        is_file: meta.is_file(),
        language,
        // Read from the scan's own walk (`infer_grammar`).
        inferred: None,
        extensions,
        parsers,
    })
}

/// Every extension a structural grammar parses.
pub(super) fn structural_extensions() -> std::collections::BTreeSet<String> {
    octocode_engine::signatures::languages::grammar_capabilities()
        .iter()
        .filter(|grammar| grammar.structural_search)
        .flat_map(|grammar| grammar.extensions.iter().cloned())
        .collect()
}
