//! Structural rule plumbing shared by the AST tools: which grammars a scope
//! holds, the compile probe, grammar inference, one invalid-pattern error,
//! and the language globs a graph-facts scan takes.

use octocode_contract_types::ErrorCode;
use std::collections::{BTreeMap, BTreeSet};

use octocode_engine::signatures::languages;

/// `languageGlobs` (language → globs) as the engine scan's flat glob list.
pub(crate) fn language_globs(
    map: &BTreeMap<String, Vec<String>>,
) -> Vec<octocode_engine::types::GraphLanguageGlob> {
    map.iter()
        .flat_map(|(language, globs)| {
            globs
                .iter()
                .map(|glob| octocode_engine::types::GraphLanguageGlob {
                    language: language.clone(),
                    glob: glob.clone(),
                })
        })
        .collect()
}

/// `languageGlobs` compiled against a scan root, by the rules the graph-facts
/// scan applies: each rule's parser extension and matcher.
#[derive(Default)]
pub(crate) struct GlobParsers(Vec<octocode_engine::fact_scan::ParserOverride>);

impl GlobParsers {
    /// No map compiles to no rules. A malformed glob is `invalidGlob`; a key
    /// that selects no single grammar is `languageUnsupported`.
    pub(crate) fn compile(
        root: &std::path::Path,
        map: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Result<Self, octocode_engine::fact_scan::GraphScanError> {
        let rules = map.map(language_globs);
        octocode_engine::fact_scan::parser_overrides(root, rules.as_deref()).map(Self)
    }

    /// Whether any rule names `path`: a file only `languageGlobs` map to a
    /// grammar is a candidate of a graph-facts walk, even when two rules of
    /// different parsers name it (the scan then skips it as ambiguous).
    pub(crate) fn names(&self, path: &std::path::Path) -> bool {
        self.0
            .iter()
            .any(|(_, matcher)| octocode_engine::search::glob::matches(matcher, path, false))
    }

    /// The parser extension the rules select for `path`; none when no rule
    /// matches it, or when rules of different parsers do (the graph scan
    /// skips such a file as ambiguous).
    pub(crate) fn extension(&self, path: &std::path::Path) -> Option<&str> {
        let mut selected = self
            .0
            .iter()
            .filter(|(_, matcher)| octocode_engine::search::glob::matches(matcher, path, false))
            .map(|(extension, _)| extension.as_str());
        let first = selected.next()?;
        selected.all(|other| other == first).then_some(first)
    }
}

/// A diagnostic that says the query does not compile in the file's grammar.
pub(crate) fn is_compile_failure(
    diagnostic: &octocode_engine::structural::StructuralDiagnostic,
) -> bool {
    matches!(
        diagnostic.code.as_str(),
        octocode_engine::structural::COMPILE_FAILED | octocode_engine::structural::RULE_INVALID
    )
}

/// Recovery for `invalidPattern`, in both tools: the code covers a
/// pattern that parses as no node and a rule object that does not decode.
pub(crate) const INVALID_PATTERN_HINT: &str = "Make each pattern one parseable node and each rule a rule object; astSearch operation:\"syntaxTree\" shows the shape.";

/// Recovery of an engine structural diagnostic. The engine names no tool or
/// contract field, so a diagnostic whose repair is another tool or a field
/// is worded here, by its code; any other keeps the engine's own.
pub(crate) fn diagnostic_recovery(code: &str, engine: Option<&str>) -> Option<String> {
    let text = match code {
        "structural.language.unsupported" => {
            "Set language to the grammar the file is written in, or search its text with localSearch."
        }
        "structural.file.tooLarge" => {
            "Search its text with localSearch, or read line ranges with localFetch."
        }
        "structural.content.tooLarge" => {
            "Target a smaller file; for this one, read bounded ranges with localFetch or search text with localSearch."
        }
        "syntaxTree.budget.deadline" | "syntaxTree.budget.nodeLimit" => {
            "Nodes past this stop are not listed on any page; query them with a match pattern or read their lines with localFetch."
        }
        _ => return engine.map(str::to_owned),
    };
    Some(text.to_owned())
}

/// One engine diagnostic as a row entry: `path` only when it names a file
/// other than the row's own, and `recovery` only when there is one.
pub(crate) fn diagnostic_row(
    d: &octocode_engine::structural::StructuralDiagnostic,
    path: Option<&str>,
) -> serde_json::Value {
    let mut row = serde_json::json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message});
    if let Some(path) = path {
        row["path"] = path.into();
    }
    if let Some(recovery) = diagnostic_recovery(&d.code, d.recovery.as_deref()) {
        row["recovery"] = recovery.into();
    }
    row
}

/// Diagnostic rows with one entry per distinct diagnostic: rows equal in
/// everything but `path` fold into one row that counts its files in the
/// message and lists every one (`files[].path`, shaped by the response stage
/// like any row path); a diagnostic one file raised keeps its own `path`.
/// Only the repeated text is removed, never a path.
pub(crate) fn group_diagnostics(rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    // Per distinct diagnostic: its first row as written, the row without
    // `path` it is compared by, and every path that raised it.
    let mut grouped: Vec<(serde_json::Value, serde_json::Value, Vec<serde_json::Value>)> =
        Vec::new();
    for row in rows {
        let mut key = row.clone();
        let path = key
            .as_object_mut()
            .and_then(|fields| fields.shift_remove("path"));
        match grouped.iter_mut().find(|(_, known, _)| *known == key) {
            Some((_, _, paths)) => paths.extend(path),
            None => grouped.push((row, key, path.into_iter().collect())),
        }
    }
    grouped
        .into_iter()
        .map(|(first, mut row, mut paths)| {
            match paths.len() {
                0 => return first,
                1 if first.get("path").is_some() => return first,
                1 => {
                    row["path"] = paths.remove(0);
                    return row;
                }
                _ => {}
            }
            if let Some(message) = row["message"].as_str() {
                row["message"] = format!("{message} ({} files)", paths.len()).into();
            }
            row["files"] = paths
                .into_iter()
                .map(|path| serde_json::json!({"path": path}))
                .collect();
            row
        })
        .collect()
}

/// A rule that does not compile, as both tools report it.
#[derive(Clone, Debug)]
pub(crate) struct RuleError {
    pub code: ErrorCode,
    pub message: String,
}

/// Compiles the pattern or rule object as a search for each extension of one
/// grammar; it passes when any of them parses it (`.tsx` accepts JSX that
/// `.ts` rejects).
pub(crate) fn compile_check(
    extensions: &BTreeSet<String>,
    pattern: Option<&str>,
    rule: Option<&serde_json::Value>,
) -> Result<(), RuleError> {
    let mut first_error = None;
    for extension in extensions {
        let result = octocode_engine::structural::search_detailed(
            "",
            &format!("pattern.{extension}"),
            None,
            pattern,
            rule,
        );
        match result
            .diagnostics
            .iter()
            .find(|diagnostic| is_compile_failure(diagnostic))
        {
            Some(diagnostic) => {
                first_error.get_or_insert_with(|| RuleError {
                    code: ErrorCode::InvalidPattern,
                    message: diagnostic.message.clone(),
                });
            }
            None => return Ok(()),
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// The grammar a query without `language` runs with.
pub(crate) enum GrammarChoice {
    /// Exactly one candidate compiles the rule.
    One(String),
    /// Candidates exist, and none compiles the rule.
    Invalid,
    /// The scope holds no candidate grammar.
    Absent,
    /// Several candidates compile it.
    Several(Vec<String>),
}

/// Picks the one candidate grammar (`selector → extensions`) that compiles.
pub(crate) fn choose_grammar(
    candidates: impl Iterator<Item = (String, BTreeSet<String>)>,
    compiles: impl Fn(&str, &BTreeSet<String>) -> Result<(), RuleError>,
) -> GrammarChoice {
    let mut first_error = None;
    let mut compiled = Vec::new();
    for (language, extensions) in candidates {
        match compiles(&language, &extensions) {
            Ok(()) => compiled.push((language, extensions)),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    // A grammar whose files another candidate's family already scans (each
    // file with its own parser: `typescript` covers `.tsx`) is no choice.
    let mut parsing: Vec<String> = compiled
        .iter()
        .enumerate()
        .filter(|(index, (_, extensions))| {
            !compiled.iter().enumerate().any(|(other, (_, family))| {
                other != *index
                    && extensions.is_subset(family)
                    && (extensions.len() < family.len() || other < *index)
            })
        })
        .map(|(_, (language, _))| language.clone())
        .collect();
    match (parsing.len(), first_error) {
        (1, _) => GrammarChoice::One(parsing.remove(0)),
        (0, Some(_)) => GrammarChoice::Invalid,
        (0, None) => GrammarChoice::Absent,
        _ => GrammarChoice::Several(parsing),
    }
}

#[cfg(test)]
thread_local! {
    static PRESENT_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Grammar-census walks this thread made (tests).
#[cfg(test)]
pub(crate) fn present_walks() -> usize {
    PRESENT_WALKS.with(std::cell::Cell::get)
}

/// Grammars whose files occur under `root`, by their extension or the
/// `languageGlobs` rule that names their parser, from a bounded walk that
/// honors the same discovery policy, ignore rules and excluded directories
/// as the match scan. Each maps to its full extension set, most files
/// first (ties by name), so a list of candidates leads with the grammar the
/// scope is mostly written in. `stop` cancels the walk
/// (`ErrorKind::Cancelled`); an invalid glob is `ErrorKind::InvalidGlob`.
pub(crate) fn present_grammars(
    root: &std::path::Path,
    prune: &[String],
    walk: crate::tools::empty_walk::Walk,
    max_files: usize,
    globs: crate::tools::empty_walk::Globs<'_>,
    parsers: &GlobParsers,
    permits: &dyn Fn(&std::path::Path) -> bool,
    stop: &octocode_engine::search::walk::Stop<'_>,
) -> Result<Vec<(String, BTreeSet<String>)>, octocode_engine::error::Error> {
    #[cfg(test)]
    PRESENT_WALKS.with(|count| count.set(count.get() + 1));
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut files = 0_usize;
    for entry in walk.walker(root, prune, globs)? {
        if stop() {
            return Err(octocode_engine::error::Error::cancelled());
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) || !permits(entry.path()) {
            continue;
        }
        files += 1;
        if files > max_files {
            break;
        }
        let extension = parsers.extension(entry.path()).map_or_else(
            || octocode_engine::text::extension_of(&entry.path().to_string_lossy(), true, ""),
            str::to_owned,
        );
        if !extension.is_empty() {
            *seen.entry(extension).or_default() += 1;
        }
    }
    Ok(rank_grammars(seen))
}

/// The grammars of a file census (`extension → files`), each with its full
/// extension set and file count, most files first (ties by name).
pub(crate) fn count_grammars(
    seen: BTreeMap<String, usize>,
) -> Vec<(String, BTreeSet<String>, usize)> {
    let mut grammars: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    for (extension, count) in seen {
        if let Some(entry) = languages::find_entry(&extension)
            && let Some(extensions) = languages::family_extensions(entry.name)
        {
            grammars
                .entry(languages::label(entry))
                .or_insert_with(|| (extensions, 0))
                .1 += count;
        }
    }
    let mut ranked = grammars.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(a, (_, a_files)), (b, (_, b_files))| b_files.cmp(a_files).then(a.cmp(b)));
    ranked
        .into_iter()
        .map(|(language, (extensions, files))| (language, extensions, files))
        .collect()
}

/// [`count_grammars`] without the counts.
pub(crate) fn rank_grammars(seen: BTreeMap<String, usize>) -> Vec<(String, BTreeSet<String>)> {
    count_grammars(seen)
        .into_iter()
        .map(|(language, extensions, _)| (language, extensions))
        .collect()
}

/// The one message for a `language` no structural grammar answers to
/// (astSearch, error code `languageUnsupported`).
pub(crate) fn unsupported_language_message(language: &str) -> String {
    format!(
        "language \"{language}\" is not a supported structural grammar. Use a language name, id, or extension such as \"typescript\", \"rust\", or \"py\"."
    )
}

/// The extension that parses a source as `language`: the one a `.ext`
/// selector names, else the grammar's first registered extension.
pub(crate) fn primary_extension(language: &str) -> Option<String> {
    let selector = language.trim().to_ascii_lowercase();
    if let Some(extension) = selector.strip_prefix('.') {
        return languages::family_extensions(&selector).map(|_| extension.to_owned());
    }
    languages::select(&selector)
        .next()
        .and_then(|entry| entry.extensions.first())
        .map(|extension| (*extension).to_owned())
}

pub(crate) fn has_extension_in(path: &std::path::Path, extensions: &BTreeSet<String>) -> bool {
    extensions.contains(&octocode_engine::text::extension_of(
        &path.to_string_lossy(),
        true,
        "",
    ))
}

#[cfg(test)]
mod tests;
