//! Match rows, scope gaps and the repairs a page offers.
use super::*;
use crate::response::result::Confidence;

/// One match row, an object: `line`, `column` (1-based), `endLine` for a
/// multi-line match, and `value` (source text or a marked, bounded prefix). With
/// `captureText` the row also carries `endColumn` for a multi-line span, and each capture once in `metavarRanges` (text plus
/// 1-based position; the engine `metavars` map is a fallback for a capture
/// without a range).
pub(super) struct MatchRow {
    pub(super) value: Value,
    /// The lean row withheld something `captureText:true` returns: a
    /// multi-line match cut to its header (signature) followed by `…`, or
    /// capture text the value does not show.
    pub(super) withheld: bool,
    /// The shown text was cut at `matchContentLength`.
    pub(super) shown_cut: bool,
    /// The match's 1-based start and end lines.
    pub(super) lines: (u64, u64),
    /// The whole value's characters when they exceed `matchContentLength`,
    /// so the shown or the `captureText` value is cut.
    pub(super) cut: Option<usize>,
}

pub(super) fn match_value(
    m: &StructuralDetailedMatch,
    capture_text: bool,
    content_length: usize,
) -> MatchRow {
    let lines = (u64::from(m.start_line), u64::from(m.end_line));
    let whole_chars = m.text.chars().count();
    let cut = (whole_chars > content_length).then_some(whole_chars);
    let header = m.header.as_deref().filter(|_| !capture_text);
    let (text, shown_cut) = match header {
        Some(header) => bounded_match(&format!("{header} …"), content_length),
        None => bounded_match(&m.text, content_length),
    };
    if !capture_text {
        let hidden = |capture: &str| !capture.is_empty() && !text.contains(capture);
        let withheld = header.is_some()
            || m.metavar_ranges
                .values()
                .flatten()
                .any(|range| hidden(&range.text))
            || m.metavars.values().flatten().any(|value| hidden(value));
        // A row is an object named like the inputs that take it; a
        // multi-line match states its last line.
        let mut value = json!({
            "line": m.start_line,
            "column": crate::num::one_based_column(m.start_col),
        });
        if m.end_line != m.start_line {
            value["endLine"] = json!(m.end_line);
        }
        value["value"] = json!(text);
        if header.is_some() || shown_cut {
            value["clipped"] = json!(true);
        }
        return MatchRow {
            value,
            withheld,
            shown_cut,
            lines,
            cut,
        };
    }
    use crate::num::one_based_column;
    let mut value =
        json!({"line":m.start_line,"value":text,"column":one_based_column(m.start_col)});
    if shown_cut {
        value["clipped"] = json!(true);
    }
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
        value["endColumn"] = json!(one_based_column(m.end_col));
    }
    let mut ranges = serde_json::Map::new();
    for (name, values) in &m.metavar_ranges {
        let rows = values
            .iter()
            .map(|range| {
                let mut row = json!({
                    "text":range.text,
                    "line":range.line,
                    "column":one_based_column(range.column),
                    "endColumn":one_based_column(range.end_column)
                });
                if range.end_line != range.line {
                    row["endLine"] = json!(range.end_line);
                }
                row
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            ranges.insert(name.clone(), Value::Array(rows));
        }
    }
    // Capture maps are unordered; emit names sorted so output is deterministic.
    let mut captures: Vec<_> = m.metavars.iter().collect();
    captures.sort_by(|a, b| a.0.cmp(b.0));
    let mut metavars = serde_json::Map::new();
    for (name, values) in captures {
        if !ranges.contains_key(name) && !values.is_empty() {
            metavars.insert(name.clone(), json!(values));
        }
    }
    if !metavars.is_empty() {
        value["metavars"] = Value::Object(metavars);
    }
    if !ranges.is_empty() {
        value["metavarRanges"] = Value::Object(ranges);
    }
    MatchRow {
        value,
        withheld: false,
        shown_cut,
        lines,
        cut,
    }
}

/// Exact source text bounded to `limit` Unicode scalars
/// (`matchContentLength`), including the trailing ellipsis when cut, and
/// whether it was cut.
pub(super) fn bounded_match(text: &str, limit: usize) -> (String, bool) {
    if text.chars().count() > limit {
        let mut output = text
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        output.push('…');
        (output, true)
    } else {
        (text.to_owned(), false)
    }
}

pub(super) fn match_display_path(root: &std::path::Path, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    let relative = candidate.strip_prefix(root).unwrap_or(candidate);
    super::super::rooted_display(root, relative)
}

pub(super) fn continuation_with(q: MatchQuery<'_>, changes: Value, snapshot: &str) -> Value {
    let mut query = q.to_value();
    query["snapshot"] = json!(snapshot);
    let query = crate::response::result::with_changes(query, &changes);
    crate::response::result::Continuation::new(ToolId::AstSearch, query)
        .confidence(Confidence::Exact)
        .build()
}

/// What a directory scan can still offer beside its answer: the other
/// grammars the scope holds (an empty scan only), and the walk flags that
/// were off while files of the requested grammar sat behind them.
#[derive(Default)]
pub(super) struct ScopeGaps {
    pub(super) repair: Option<Value>,
    pub(super) skipped: Option<SkippedWalk>,
}

/// What the skipped walks hold of the requested grammar, as every local
/// tool discloses it ([`crate::tools::leads::skipped`]), with its lead.
pub(super) struct SkippedWalk {
    pub(super) disclosure: crate::tools::leads::skipped::Disclosure,
    pub(super) lead: Value,
}

impl SkippedWalk {
    /// Place the disclosure and its lead on `out`.
    pub(super) fn attach(self, out: &mut Value) {
        crate::tools::leads::skipped::attach(out, &self.disclosure, self.lead);
    }
}

impl ScopeGaps {
    pub(super) fn of(
        q: MatchQuery<'_>,
        scope: &Scope,
        paths: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Self {
        Self {
            repair: other_language_repair(q, scope, paths, cancel),
            skipped: skipped_walk(q, scope, paths, cancel),
        }
    }

    /// A scan with no other language to offer (it found matches, it parsed
    /// files, or no `language` was set): only the skipped walks, no repair.
    pub(super) fn beside_parsed(
        q: MatchQuery<'_>,
        scope: &Scope,
        paths: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Self {
        Self {
            repair: None,
            skipped: skipped_walk(q, scope, paths, cancel),
        }
    }
}

/// Asks which of the walks the request left off (ignore files, hidden
/// entries, default-excluded directories) hold files of the requested
/// grammar; the lead lifts exactly those.
pub(super) fn skipped_walk(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<SkippedWalk> {
    let extensions = scope.extensions.as_ref()?;
    let walk = Walk {
        ignored: q.include_ignored().unwrap_or(false),
        hidden: q.include_hidden().unwrap_or(false),
        default_excludes: q.default_excludes(),
    };
    let budget = q
        .max_files()
        .map_or_else(octocode_engine::scan_budget::walk_ceiling, |files| {
            files as usize
        });
    let (include, exclude) = (
        q.include().unwrap_or_default(),
        q.exclude().unwrap_or_default(),
    );
    let discovery = paths.discovery_walk();
    let permits = |path: &std::path::Path| discovery.permits(path);
    let stop = || cancel.check().is_err();
    let keep = |path: &std::path::Path| {
        has_extension_in(path, extensions) || scope.parsers.extension(path).is_some()
    };
    let probe = FileProbe::new(&scope.canonical, budget, &permits, &keep)
        .within(Globs {
            include: &include,
            exclude: &exclude,
        })
        .stopping(&stop)
        .beyond(walk);
    let verdict = diagnose(&scope.canonical, walk, |relaxed| probe.find(relaxed)).ok()?;
    let language = scope.language.as_deref().or(scope.inferred.as_deref());
    let subject = language.map_or_else(
        || ["file".to_owned(), "files".to_owned()],
        |name| [format!("{name} file"), format!("{name} files")],
    );
    // A check cut before it found anything proved nothing: every cause the
    // walk skipped by is unproven.
    let skipping = walk.skipping();
    let named = skipping
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(", ");
    let disclosure = crate::tools::leads::skipped::disclose(
        &verdict,
        [subject[0].as_str(), subject[1].as_str()],
        crate::tools::leads::skipped::Unchecked {
            causes: &skipping,
            named: &named,
        },
    )?;
    let lead =
        crate::tools::leads::skipped::lead(ToolId::AstSearch, &q.to_value(), &disclosure.causes);
    Some(SkippedWalk { disclosure, lead })
}

/// The grammars of a directory scope that occur in it and compile the query,
/// without `language` in play.
pub(super) fn directory_choice(
    q: MatchQuery<'_>,
    root: &std::path::Path,
    parsers: &GlobParsers,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<GrammarChoice, ToolError> {
    let (include, exclude) = (
        q.include().unwrap_or_default(),
        q.exclude().unwrap_or_default(),
    );
    let prune = prune::walk_names(q.default_excludes(), &include);
    let discovery = paths.discovery_walk();
    let present = present_grammars(
        root,
        &prune,
        Walk {
            ignored: q.include_ignored().unwrap_or(false),
            hidden: q.include_hidden().unwrap_or(false),
            default_excludes: q.default_excludes(),
        },
        q.max_files()
            .map_or_else(octocode_engine::scan_budget::walk_ceiling, |files| {
                files as usize
            }),
        Globs {
            include: &include,
            exclude: &exclude,
        },
        parsers,
        &|path| discovery.permits(path),
        &|| cancel.check().is_err(),
    )
    .map_err(super::super::native_error)?;
    let (pattern, rule) = (q.pattern(), q.rule());
    Ok(choose_grammar(present.into_iter(), |_, extensions| {
        compile_check(extensions, pattern.as_deref(), rule.as_ref())
    }))
}

/// One repair lead that runs the query once per candidate grammar, as a
/// batch within the input limit (the most frequent grammars lead).
pub(super) fn language_repair(q: MatchQuery<'_>, languages: &[String]) -> Value {
    let base = q.to_value();
    let queries = languages
        .iter()
        .take(crate::response::result::rows_per_call(ToolId::AstSearch))
        .map(|language| {
            let mut row = base.clone();
            row["language"] = json!(language);
            if let Some(fields) = row.as_object_mut() {
                fields.remove("snapshot");
            }
            row
        })
        .collect::<Vec<_>>();
    crate::response::result::Continuation::input(ToolId::AstSearch, json!({ "queries": queries }))
        .confidence(Confidence::Exact)
        .build()
}

/// The repair of a `language` that parsed no file in a directory: the
/// grammars the scope does hold, when the query compiles in any of them.
pub(super) fn other_language_repair(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Value> {
    let requested = q.language()?;
    let languages =
        match directory_choice(q, &scope.canonical, &scope.parsers, paths, cancel).ok()? {
            GrammarChoice::One(language) => vec![language],
            GrammarChoice::Several(languages) => languages,
            GrammarChoice::Invalid | GrammarChoice::Absent => return None,
        }
        .into_iter()
        .filter(|language| *language != requested)
        .collect::<Vec<_>>();
    (!languages.is_empty()).then(|| language_repair(q, &languages))
}

pub(super) fn diag(d: &StructuralDiagnostic) -> Value {
    crate::tools::ast_rule::diagnostic_row(d, d.path.as_deref())
}

pub(super) fn error_diagnostic(
    diagnostics: &[StructuralDiagnostic],
) -> Option<&StructuralDiagnostic> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == "error")
}

/// The engine diagnostic as a tool error carrying its declared wire code.
pub(super) fn diagnostic_error(diagnostic: &StructuralDiagnostic) -> ToolError {
    ToolError::new(
        super::super::engine_error_code(&diagnostic.code),
        diagnostic.message.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: &str) -> StructuralDiagnostic {
        StructuralDiagnostic {
            code: code.to_owned(),
            severity: "error".to_owned(),
            stage: "search".to_owned(),
            message: "failed".to_owned(),
            path: None,
            recovery: None,
        }
    }

    #[test]
    fn diagnostic_errors_carry_only_declared_error_codes() {
        for (engine, wire) in [
            (octocode_engine::structural::RULE_INVALID, "invalidPattern"),
            ("structural.file.unreadable", "fileReadFailed"),
            (octocode_engine::structural::MATCH_DEADLINE, "timeout"),
            ("structural.unknown.failure", "executionFailed"),
        ] {
            assert_eq!(
                crate::id::error_codes::as_str(diagnostic_error(&diagnostic(engine)).code),
                wire,
                "{engine}"
            );
        }
    }
}
