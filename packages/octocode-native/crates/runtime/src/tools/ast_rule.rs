//! Structural rule plumbing shared by the AST tools: which grammars a scope
//! holds, the compile probe, grammar inference, one invalid-pattern error,
//! and the language globs a graph-facts scan takes.

use std::collections::{BTreeMap, BTreeSet};

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

/// Error code for a pattern or rule that compiles for no grammar.
pub(crate) const INVALID_PATTERN: &str = "invalidPattern";

/// Recovery for [`INVALID_PATTERN`], in both tools.
pub(crate) const INVALID_PATTERN_HINT: &str = "Make the pattern one complete, parseable node, or inspect its shape with astSearch operation:\"syntaxTree\".";

/// A rule that does not compile, as both tools report it.
#[derive(Debug)]
pub(crate) struct RuleError {
    pub code: &'static str,
    pub message: String,
}

/// Engine text without its leading `[code] ` tag.
pub(crate) fn untagged(message: &str) -> &str {
    message
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .map_or(message, |(_, text)| text)
}

/// Compiles the pattern or YAML rule as a search for each extension of one
/// grammar; it passes when any of them parses it (`.tsx` accepts JSX that
/// `.ts` rejects).
pub(crate) fn compile_check(
    extensions: &BTreeSet<String>,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<(), RuleError> {
    let mut first_error = None;
    for extension in extensions {
        let result = octocode_engine::portable::structural_search_detailed(
            "",
            &format!("pattern.{extension}"),
            pattern,
            rule,
        )
        .map_err(|error| RuleError {
            code: INVALID_PATTERN,
            message: untagged(&error.to_string()).to_owned(),
        })?;
        match result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "structural.query.compileFailed")
        {
            Some(diagnostic) => {
                first_error.get_or_insert_with(|| RuleError {
                    code: INVALID_PATTERN,
                    message: untagged(&diagnostic.message).to_owned(),
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
    /// Candidates exist, and none compiles it: the first compile error.
    Invalid(RuleError),
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
        (0, Some(error)) => GrammarChoice::Invalid(error),
        (0, None) => GrammarChoice::Absent,
        _ => GrammarChoice::Several(parsing),
    }
}

/// The grammar and extensions a file's own extension selects.
pub(crate) fn file_grammar(path: &std::path::Path) -> Option<(String, BTreeSet<String>)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    octocode_engine::portable::grammar_capabilities()
        .into_iter()
        .filter(|capability| capability.structural_search)
        .find(|capability| capability.extensions.contains(&extension))
        .map(|capability| {
            let extensions = language_extensions(&capability.language)
                .unwrap_or_else(|| BTreeSet::from([extension]));
            (grammar_selector(&capability), extensions)
        })
}

/// Grammars whose extensions occur under `root`, from a bounded walk that
/// honors the same discovery policy, ignore files and excluded directories
/// as the match scan. Each maps to its full extension set.
pub(crate) fn present_grammars(
    root: &std::path::Path,
    prune: &[String],
    hidden: bool,
    no_ignore: bool,
    max_files: usize,
    permits: &dyn Fn(&std::path::Path) -> Result<bool, String>,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let capabilities = octocode_engine::portable::grammar_capabilities()
        .into_iter()
        .filter(|capability| capability.structural_search)
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut files = 0_usize;
    let pruned = prune.to_vec();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(!hidden)
        .git_ignore(!no_ignore)
        .git_exclude(!no_ignore)
        .ignore(!no_ignore)
        .parents(!no_ignore)
        .filter_entry(move |entry| {
            entry.depth() == 0
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !pruned
                    .iter()
                    .any(|name| entry.file_name().to_string_lossy() == name.as_str())
        })
        .build();
    for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if !permits(entry.path())? {
            continue;
        }
        files += 1;
        if files > max_files {
            break;
        }
        if let Some(extension) = entry.path().extension().and_then(|ext| ext.to_str()) {
            seen.insert(extension.to_ascii_lowercase());
        }
    }
    let mut grammars = BTreeMap::new();
    for extension in seen {
        if let Some(capability) = capabilities
            .iter()
            .find(|capability| capability.extensions.contains(&extension))
            && let Some(extensions) = language_extensions(&capability.language)
        {
            grammars.insert(grammar_selector(capability), extensions);
        }
    }
    Ok(grammars)
}

/// The language a caller would write for a grammar: its language id
/// (`rust`, `typescript`), else its lowercased name.
pub(crate) fn grammar_selector(capability: &octocode_engine::types::GrammarCapability) -> String {
    capability
        .language_id
        .clone()
        .unwrap_or_else(|| capability.language.to_ascii_lowercase())
}

pub(crate) fn language_extensions(language: &str) -> Option<BTreeSet<String>> {
    let selector = language.trim().to_ascii_lowercase();
    let capabilities = octocode_engine::portable::grammar_capabilities();
    if let Some(extension) = selector.strip_prefix('.') {
        return capabilities
            .iter()
            .any(|capability| capability.extensions.iter().any(|item| *item == extension))
            .then(|| BTreeSet::from([extension.to_owned()]));
    }
    let mut extensions: BTreeSet<String> = capabilities
        .into_iter()
        .filter(|capability| {
            capability.language.eq_ignore_ascii_case(&selector)
                || capability
                    .language_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&selector))
                || capability
                    .selector_aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&selector))
                || capability
                    .extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(&selector))
        })
        .flat_map(|capability| capability.extensions)
        .map(|extension| extension.to_ascii_lowercase())
        .collect();
    if !extensions.is_empty() && (selector == "cpp" || selector == "c++") {
        extensions.insert("h".to_owned());
    }
    (!extensions.is_empty()).then_some(extensions)
}

pub(crate) fn has_extension_in(path: &std::path::Path, extensions: &BTreeSet<String>) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.contains(&ext.to_ascii_lowercase()))
}
