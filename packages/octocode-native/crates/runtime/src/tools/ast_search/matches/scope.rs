//! Scope resolution and grammar choice: the validated scope, the grammar a
//! query compiles in, why a pattern cannot apply, and the exact-miss rule.
use super::*;
use crate::response::result::Confidence;
use octocode_contract_types::ErrorCode;

/// A declaration pattern that matched nothing in a scope that holds
/// declarations of its kind (named as the pattern names it): a pattern
/// matches every part it spells, so an item that adds or leaves out a part
/// (a modifier, a return type) is not a match. The rule that lists them.
pub(super) struct ExactMiss {
    pub(super) kind: String,
    pub(super) count: usize,
    pub(super) files: usize,
    pub(super) query: Value,
}

/// The parser extension a scope's files are read with: a single file's own
/// (or its override), else the named or inferred grammar's.
pub(super) fn scope_extension(scope: &Scope) -> Option<String> {
    if scope.is_file {
        return Some(
            super::super::parser_override(&scope.canonical, scope.language.as_deref())
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

/// [`ExactMiss`] of an empty `pattern` scan, counted by one scan of its
/// rule over the same scope; `None` when the pattern spells no declaration
/// or the scope holds none of its kind.
pub(super) fn exact_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Option<ExactMiss> {
    let MatchQuery::Pattern(_) = q else {
        return None;
    };
    let extension = scope_extension(scope)?;
    let declaration = octocode_engine::structural::pattern_declaration(&q.pattern()?, &extension)?;
    let mut rule = json!({"kind": declaration.kind});
    if let Some((kind, name)) = &declaration.name {
        rule["has"] = json!({"kind": kind, "regex": format!("^{}$", regex::escape(name))});
    }
    let mut query = q.to_value();
    let fields = query.as_object_mut()?;
    for field in ["pattern", "snapshot", "page", "matchPage", "scanOffset"] {
        fields.shift_remove(field);
    }
    fields.insert("rule".into(), rule);
    if !scope.is_file
        && !fields.contains_key("language")
        && let Some(inferred) = &scope.inferred
    {
        fields.insert("language".into(), json!(inferred));
    }
    let rule_query: AstSearchQueryMatchRule = serde_json::from_value(query.clone()).ok()?;
    let rule_query = MatchQuery::Rule(&rule_query);
    let rule_scope = resolve_scope(rule_query, paths, cancel).ok()?;
    let scan = if rule_scope.is_file {
        scan_file(rule_query, &rule_scope, security)
    } else {
        scan_directory(rule_query, &rule_scope, paths, cancel)
    }
    .ok()?;
    let count = scan
        .files
        .iter()
        .map(|file| file.matches.len())
        .sum::<usize>();
    let files = scan
        .files
        .iter()
        .filter(|file| !file.matches.is_empty())
        .count();
    (count > 0).then_some(ExactMiss {
        kind: declaration.kind,
        count,
        files,
        query,
    })
}

impl ExactMiss {
    /// The diagnostic that counts the declarations the pattern missed, and
    /// the rule query that lists them.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            kind,
            count,
            files,
            query,
        } = self;
        let diagnostic = json!({
            "code": "structural.pattern.exact",
            "severity": "warning",
            "stage": "match",
            "message": format!(
                "The pattern matches as written: {count} {kind} node(s) in {files} file(s) differ from it in a part it spells or leaves out (a modifier, a return type)."
            ),
            "recovery": "Spell those parts in the pattern, or list every node of the kind with a rule.",
        });
        match out["diagnostics"].as_array_mut() {
            Some(diagnostics) => diagnostics.push(diagnostic),
            None => out["diagnostics"] = json!([diagnostic]),
        }
        out["next"]["broadenSearch"] =
            crate::response::result::Continuation::new(ToolId::AstSearch, query)
                .confidence(Confidence::Exact)
                .build();
    }
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
/// once per call. Empty for a single file or an explicit `language`, whose
/// compile checks are the scope's own.
pub(super) fn compile_in_grammars(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
) -> Vec<GrammarCompile> {
    if scope.is_file || scope.language.is_some() {
        return Vec::new();
    }
    let (pattern, rule) = (q.pattern(), q.rule());
    count_grammars(scan.census.clone())
        .into_iter()
        .map(|(label, extensions, files)| GrammarCompile {
            outcome: compile_check(&extensions, pattern.as_deref(), rule.as_deref()),
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
    /// Grammar label → (its files in the walk, why the query does not compile).
    pub(super) languages: std::collections::BTreeMap<String, (usize, String)>,
}

impl Inapplicable {
    pub(super) fn of(grammars: &[GrammarCompile]) -> Self {
        if !grammars.iter().any(|grammar| grammar.outcome.is_ok()) {
            return Self::default();
        }
        Self {
            languages: grammars
                .iter()
                .filter_map(|grammar| {
                    let error = grammar.outcome.as_ref().err()?;
                    Some((
                        grammar.label.clone(),
                        (grammar.files, error.message.clone()),
                    ))
                })
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

    /// The warning that names each language with its file count and reason.
    pub(super) fn notice(&self) -> Option<String> {
        if self.languages.is_empty() {
            return None;
        }
        let names = self
            .languages
            .iter()
            .map(|(language, (files, reason))| {
                let plural = if *files == 1 { "" } else { "s" };
                format!("{language} ({files} file{plural}: {reason})")
            })
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!(
            "The query does not apply to {names}: it does not compile in that grammar, so those files cannot match it. The files of the other grammars were searched; set language to search one grammar."
        ))
    }
}

/// The one grammar a directory scan without `language` read, from the
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
                "No source file of a supported structural grammar was found under path; set language.",
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
            language_extensions(language).ok_or_else(|| {
                ToolError::new(
                    ErrorCode::LanguageUnsupported,
                    crate::tools::ast_rule::unsupported_language_message(language),
                )
            })
        })
        .transpose()?;
    let p = paths
        .validate(q.path())
        .map_err(super::super::policy_error)?;
    let meta = std::fs::metadata(&p.canonical)
        .map_err(|error| super::super::io_error(&q.path(), error))?;
    if q.scope().is_some() && !meta.is_file() {
        return Err(ToolError::new(
            ErrorCode::InvalidInput,
            "scope requires a single source file; use one query per file.",
        ));
    }
    if meta.is_file() && q.language_globs().is_some() {
        return Err(ToolError::new(
            ErrorCode::LanguageDirectoryRequired,
            "languageGlobs is for directory matches. Use language for a single file.",
        ));
    }
    let parsers = GlobParsers::compile(&p.canonical, q.language_globs())
        .map_err(|error| ToolError::graph_scan(&error, cancel))?;
    let extensions = requested.or_else(|| meta.is_dir().then(structural_extensions));
    // Validate against grammar capabilities, not the capped discovery prefix.
    // The literal prefilter may otherwise hide even a globally invalid query.
    if meta.is_dir()
        && let Some(extensions) = &extensions
    {
        compile_check(extensions, q.pattern().as_deref(), q.rule().as_deref())
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
