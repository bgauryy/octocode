//! Bounded probe searches of an empty page: other spellings of a literal
//! miss, and matches in the paths the walk skipped.

use super::leads::*;
use super::types::*;
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::tools::empty_walk::{Cause, Globs, Hits, Tagged, Verdict, Walk, diagnose};
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{
    LocalSearchSkippedPath, LocalSearchSkippedPathCause, LocalSearchSkippedPaths,
};
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SpanValues, TextSearchOptions,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Re-run an empty search over what the defaults leave out (ignored and
/// hidden entries, the default-excluded directories), each file found
/// tagged with the causes that hid it ([`probe_lifted`]).
pub(super) fn probe_unsearched(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    search_options: &TextSearchOptions,
    cancel: &impl CancellationCheck,
) -> Result<Tagged, ()> {
    let walk = query_walk(query);
    probe_lifted(
        search_options,
        walk.lifting(&walk.skipping()),
        paths,
        cancel,
    )
}

/// The walk the query ran: the causes it skipped paths by.
fn query_walk(query: &LocalSearchQuery) -> Walk {
    Walk {
        ignored: query.include_ignored == Some(true),
        hidden: query.include_hidden == Some(true),
        default_excludes: query.default_excludes.defaults(),
    }
}

/// Say which skipped paths the re-run of an empty search
/// ([`probe_unsearched`]) found a match in: the hint names the rule, entry
/// or directory, and the lead searches again with only those lifted.
/// Default-excluded directories the re-run could not finish are named with
/// the lead that searches them. Returns whether skipped paths were shown to
/// hold a match.
pub(super) fn disclose_unsearched(
    query: &LocalSearchQuery,
    search_options: &TextSearchOptions,
    tagged: Result<Tagged, ()>,
    excluded: Option<String>,
    hidden: usize,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
) -> Result<(bool, Option<LocalSearchSkippedPaths>), ToolError> {
    let walk = query_walk(query);
    let root = std::path::PathBuf::from(&search_options.path);
    let verdict = diagnose(&root, walk, |relaxed| {
        Ok::<_, std::convert::Infallible>(tagged.as_ref().map_or_else(
            |()| Hits {
                cut: true,
                ..Hits::default()
            },
            |tagged| tagged.find(relaxed, None),
        ))
    });
    let (found, causes, may_hold_matches, entries) = match verdict {
        Ok(Verdict::Skipped(diagnosis)) => {
            let at_least = if diagnosis.found().cut {
                "at least "
            } else {
                ""
            };
            let listed = if query.result_view == LocalSearchQueryResultView::FilesWithout {
                "lack a match"
            } else {
                "match"
            };
            (
                vec![format!(
                    "{at_least}{} file(s) {listed} in skipped paths ({}).",
                    diagnosis.found().count,
                    diagnosis.labels()
                )],
                diagnosis.causes().to_vec(),
                true,
                observed_paths(diagnosis.skips())?,
            )
        }
        Ok(Verdict::Unproven) => {
            let ignored = unsearched_ignored(&root, walk);
            let mut found = vec![];
            let mut causes = vec![];
            let hidden = if walk.hidden { 0 } else { hidden };
            let may_hold_matches = !ignored.is_empty() || hidden > 0;
            if !ignored.is_empty() {
                found.push(format!(
                    "{} ignored directory(s) were not searched; the check of them timed out.",
                    ignored.len()
                ));
                causes.push(Cause::Ignored);
            }
            if hidden > 0 {
                found.push(format!(
                    "{hidden} hidden path(s) were not searched; the check of them timed out."
                ));
                causes.push(Cause::Hidden);
            }
            if let Some(counts) = excluded.filter(|_| walk.default_excludes) {
                found.push(format!(
                    "Default-excluded paths were not searched ({counts})."
                ));
                causes.push(Cause::DefaultExcluded);
            }
            if causes.is_empty() {
                return Ok((false, None));
            }
            (found, causes, may_hold_matches, observed_paths(&ignored)?)
        }
        _ => return Ok((false, None)),
    };
    for hint in found.into_iter().rev() {
        hints.insert(0, hint);
    }
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("includeSkipped".into(), lifting_lead(query, &causes));
    }
    let observed = (!entries.is_empty()).then_some(LocalSearchSkippedPaths {
        entries,
        sampled: true,
    });
    Ok((may_hold_matches, observed))
}

/// A skip cause as `skippedPaths` names it.
pub(super) fn skipped_cause(cause: Cause) -> LocalSearchSkippedPathCause {
    match cause {
        Cause::Ignored => LocalSearchSkippedPathCause::Ignored,
        Cause::Hidden => LocalSearchSkippedPathCause::Hidden,
        Cause::DefaultExcluded => LocalSearchSkippedPathCause::DefaultExcluded,
    }
}

fn observed_paths(
    skipped: &[crate::tools::empty_walk::Skip],
) -> Result<Vec<LocalSearchSkippedPath>, ToolError> {
    skipped
        .iter()
        .map(|skip| {
            Ok(LocalSearchSkippedPath {
                path: octocode_engine::text::slash_path(&skip.entry)
                    .try_into()
                    .map_err(skipped_path_error)?,
                cause: skipped_cause(skip.cause),
                reason: Some(skip.reason.clone()),
            })
        })
        .collect()
}

pub(super) fn skipped_path_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::new(
        ErrorCode::OutputContractViolation,
        format!("Invalid skipped-path evidence: {error}"),
    )
}

/// The ignored directories a walk skipped, named, within a fresh
/// [`PROBE_BUDGET`](crate::tools::empty_walk::PROBE_BUDGET) of their own
/// (the probe that ran out of time left none of the diagnosis' budget): the
/// rules the lead lifts.
fn unsearched_ignored(root: &std::path::Path, walk: Walk) -> Vec<crate::tools::empty_walk::Skip> {
    let started = std::time::Instant::now();
    let allowance = crate::tools::empty_walk::PROBE_BUDGET;
    crate::tools::empty_walk::ignored_directories(root, walk, &|| started.elapsed() > allowance)
}

/// Counts of the paths the original walk observed but default exclusions skipped.
pub(super) fn excluded_counts(skipped: &crate::policy::discovery::WalkSkips) -> Option<String> {
    let dirs = skipped.pruned.len();
    let files = skipped.generated.len();
    let mut parts = Vec::new();
    if dirs > 0 {
        parts.push(crate::response::pages::counted(dirs as u64, "dir", "dirs"));
    }
    if files > 0 {
        parts.push(crate::response::pages::counted(
            files as u64,
            "file",
            "files",
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" and "))
}

/// Words of a compound identifier (`parseConfig`, `parse_config`,
/// `ParseConfig`, `parse-config`, `HTTPServer`): split at `_`, `-` and case
/// boundaries; `None` for one word or a non-identifier.
fn identifier_words(term: &str) -> Option<Vec<String>> {
    let valid = term.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && term
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return None;
    }
    let chars: Vec<char> = term.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = index.checked_sub(1).map(|i| chars[i]);
        let next = chars.get(index + 1).copied();
        // `aB` starts a word; so does the `B` of `HTTPServer` before `e`.
        let boundary = c.is_ascii_uppercase()
            && previous.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
            });
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c.to_ascii_lowercase());
    }
    if !word.is_empty() {
        words.push(word);
    }
    (words.len() >= 2).then_some(words)
}

/// The other case styles of a compound identifier: snake, camel, Pascal,
/// SCREAMING and kebab, without the term itself.
pub(super) fn identifier_variants(term: &str) -> Option<Vec<String>> {
    let words = identifier_words(term)?;
    let capitalized = |word: &String| {
        let mut chars = word.chars();
        chars
            .next()
            .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
            .unwrap_or_default()
    };
    let camel = words[0].clone() + &words[1..].iter().map(capitalized).collect::<String>();
    let mut variants = vec![
        words.join("_"),
        camel,
        words.iter().map(capitalized).collect::<String>(),
        words.join("_").to_ascii_uppercase(),
        words.join("-"),
    ];
    variants.retain(|variant| variant != term);
    variants.sort_unstable();
    variants.dedup();
    (!variants.is_empty()).then_some(variants)
}

/// Shortest identifier probed for one-edit spellings: a shorter one is one
/// edit from too many unrelated names to name.
const MIN_SPELLING_CHARS: usize = 4;

/// Every identifier one edit from `term`: a char deleted, substituted or
/// inserted, or two adjacent chars swapped. Substituted and inserted chars
/// are lowercase letters, digits and `_`, and uppercase letters when `term`
/// has one. Sorted, without `term`; `None` when `term` is not an ASCII
/// identifier (`[A-Za-z_][A-Za-z0-9_]*`) of at least [`MIN_SPELLING_CHARS`].
pub(super) fn one_edit_spellings(term: &str) -> Option<Vec<String>> {
    let identifier = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if term.len() < MIN_SPELLING_CHARS || !identifier(term) {
        return None;
    }
    let chars = term.chars().collect::<Vec<_>>();
    let alphabet = ('a'..='z')
        .chain('0'..='9')
        .chain(['_'])
        .chain(('A'..='Z').filter(|_| chars.iter().any(char::is_ascii_uppercase)))
        .collect::<Vec<_>>();
    let mut spellings = std::collections::BTreeSet::new();
    let mut edited = |edit: &dyn Fn(&mut Vec<char>)| {
        let mut spelling = chars.clone();
        edit(&mut spelling);
        spellings.insert(spelling.into_iter().collect::<String>());
    };
    for at in 0..chars.len() {
        edited(&|s: &mut Vec<char>| {
            s.remove(at);
        });
        if at + 1 < chars.len() {
            edited(&|s: &mut Vec<char>| s.swap(at, at + 1));
        }
        for &c in &alphabet {
            edited(&|s: &mut Vec<char>| s[at] = c);
        }
    }
    for at in 0..=chars.len() {
        for &c in &alphabet {
            edited(&|s: &mut Vec<char>| s.insert(at, c));
        }
    }
    spellings.remove(term);
    spellings.retain(|spelling| identifier(spelling));
    Some(spellings.into_iter().collect())
}

/// What one spelling probe found: files holding a case-style variant, and
/// each one-edit spelling found with the files holding it.
#[derive(Default)]
struct Spellings {
    /// Files holding the searched text in another case.
    folded_files: usize,
    variant_files: usize,
    near: std::collections::BTreeMap<String, usize>,
    near_files: usize,
    /// The deadline cut the walk: every count is a floor.
    cut: bool,
}

/// One bounded probe: `options` over the query's own walk (`paths`, the
/// default excludes when `defaults`, the include globs), stopped by `cancel`
/// or the probe budget. `None` when it cannot run; the flag says the budget
/// cut it, so every count it makes is a floor.
pub(super) fn probe_search(
    options: TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> Option<(octocode_engine::types::TextSearchResult, bool)> {
    let walk = Arc::new(crate::policy::discovery::SearchWalk::new(
        paths.clone(),
        std::path::PathBuf::from(&options.path),
        defaults,
        options.include.as_deref().unwrap_or_default(),
    ));
    let started = std::time::Instant::now();
    let deadline = crate::tools::empty_walk::probe_budget();
    let cut = std::sync::atomic::AtomicBool::new(false);
    let found = crate::tools::leads::best_effort(|| {
        search_cancellable(options, walk, &|| {
            let late = started.elapsed() > deadline;
            if late {
                cut.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            cancel.check().is_err() || late
        })
    })?
    .ok()?;
    Some((found, cut.load(std::sync::atomic::Ordering::Relaxed)))
}

/// One bounded walk over the query's own scope and walk (its default
/// excludes included, as the `findSimilar` lead runs it) for `folded` (the
/// searched text) in any case, `variants` anywhere in a line and `near`
/// spellings as whole words, naming which occur in which files.
fn probe_spellings(
    options: &TextSearchOptions,
    folded: Option<&str>,
    variants: &[String],
    near: &[String],
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> Spellings {
    let alternation = |terms: &[String]| {
        terms
            .iter()
            .map(|term| regex::escape(term))
            .collect::<Vec<_>>()
            .join("|")
    };
    let mut options = options.probe(SearchOutput::Spans(SpanValues::Unique));
    options.pattern = [
        folded.map(|text| format!("(?i:{})", regex::escape(text))),
        (!variants.is_empty()).then(|| alternation(variants)),
        (!near.is_empty()).then(|| format!(r"\b(?:{})\b", alternation(near))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("|");
    // Each file's distinct spans name what it holds.
    options.syntax = PatternSyntax::Regex;
    options.case = CaseRule::Sensitive;
    options.whole_word = Some(false);
    options.invert_match = Some(false);
    options.multiline = Some(false);
    options.max_snippet_chars = variants
        .iter()
        .chain(near)
        .map(String::as_str)
        .chain(folded)
        .map(|term| u32_of_usize(term.len()))
        .max();
    let (found, cut) = probe_search(options, paths, defaults, cancel)
        .map(|(found, cut)| (found.files, cut))
        .unwrap_or_default();
    let mut spellings = Spellings {
        cut,
        ..Spellings::default()
    };
    for file in found {
        let holds = |terms: &[String]| {
            file.matches
                .iter()
                .filter(|span| terms.binary_search(&span.value).is_ok())
                .map(|span| span.value.as_str())
                .collect::<std::collections::BTreeSet<_>>()
        };
        if folded.is_some_and(|text| {
            file.matches
                .iter()
                .any(|span| span.value != text && span.value.to_lowercase() == text.to_lowercase())
        }) {
            spellings.folded_files += 1;
        }
        if !holds(variants).is_empty() {
            spellings.variant_files += 1;
        }
        let held = holds(near);
        if !held.is_empty() {
            spellings.near_files += 1;
        }
        for spelling in held {
            *spellings.near.entry(spelling.to_owned()).or_default() += 1;
        }
    }
    spellings
}

/// The same search over other spellings of the identifier: its case-style
/// variants anywhere, or the near spellings found as whole words.
fn variants_lead(query: &LocalSearchQuery, variants: &[String], whole_word: bool) -> Value {
    restart_lead(query, |fields| {
        let pattern = variants
            .iter()
            .map(|variant| regex::escape(variant))
            .collect::<Vec<_>>()
            .join("|");
        fields.insert("matchString".into(), json!(pattern));
        fields.insert("regex".into(), json!("rust"));
        fields.remove("caseMode");
        if whole_word {
            fields.insert("wholeWord".into(), json!(true));
        }
    })
}

/// An empty literal search's other spellings: its text in any case (a
/// case-sensitive run), an identifier's other case styles and its spellings
/// one edit away (a typo), probed in one walk. `None` when it has none.
pub(super) fn probe_variants(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: &TextSearchOptions,
    cancel: &impl CancellationCheck,
) -> Option<Variants> {
    if query
        .regex
        .is_some_and(|regex| regex != LocalSearchQueryRegex::Literal)
    {
        return None;
    }
    // A search that ran case-sensitively (`sensitive`, or smart case on
    // text with an uppercase letter) also probes the text in any case.
    let case_sensitive = match query.case_mode {
        LocalSearchQueryCaseMode::Sensitive => true,
        LocalSearchQueryCaseMode::Smart => local_search_query::pattern(query)
            .chars()
            .any(char::is_uppercase),
        LocalSearchQueryCaseMode::Insensitive => false,
    };
    let folded = case_sensitive.then_some(local_search_query::pattern(query));
    let variants = identifier_variants(local_search_query::pattern(query)).unwrap_or_default();
    let mut near = one_edit_spellings(local_search_query::pattern(query)).unwrap_or_default();
    // A case style one edit away (`parse_config` → `parse-config`) is
    // named as a variant.
    near.retain(|spelling| variants.binary_search(spelling).is_err());
    if folded.is_none() && variants.is_empty() && near.is_empty() {
        return None;
    }
    let found = probe_spellings(
        options,
        folded,
        &variants,
        &near,
        paths,
        query.default_excludes.defaults(),
        cancel,
    );
    Some(Variants { variants, found })
}

/// The other spellings [`probe_variants`] tried, and where they occur.
pub(super) struct Variants {
    variants: Vec<String>,
    found: Spellings,
}

/// Say when an empty literal search's text matches ignoring case, else when
/// an identifier's other case styles match, else its spellings one edit away,
/// so a spelling miss does not read as absence: the first that matches leads
/// (`ignoreCase`, else `findSimilar`).
pub(super) fn disclose_variants(
    query: &LocalSearchQuery,
    Variants { variants, found }: Variants,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
) {
    let floor = if found.cut { "at least " } else { "" };
    let (name, hint, lead) = if found.folded_files > 0 {
        (
            "ignoreCase",
            format!(
                "No match for {}; it matches ignoring case in {floor}{} file(s).",
                local_search_query::pattern(query),
                found.folded_files
            ),
            ignore_case_lead(query),
        )
    } else if found.variant_files > 0 {
        (
            "findSimilar",
            format!(
                "No match for {}; its other spellings match in {floor}{} file(s).",
                local_search_query::pattern(query),
                found.variant_files
            ),
            variants_lead(query, &variants, false),
        )
    } else if found.near_files > 0 {
        let names = found.near.keys().cloned().collect::<Vec<_>>();
        (
            "findSimilar",
            format!(
                "No match for {}; near spellings match in {floor}{} file(s): {}.",
                local_search_query::pattern(query),
                found.near_files,
                names.join(", ")
            ),
            variants_lead(query, &names, true),
        )
    } else {
        return;
    };
    hints.insert(0, hint);
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert(name.into(), lead);
    }
}

/// The same search with `caseMode:"insensitive"`, from page 1.
fn ignore_case_lead(query: &LocalSearchQuery) -> Value {
    restart_lead(query, |fields| {
        fields.insert("caseMode".into(), json!("insensitive"));
    })
}

/// The files `options`' search finds under the fully relaxed `walk` (files
/// only), tagged with the causes that hide each ([`Tagged::found`]). A search
/// that cannot run is `Err`; one cut by its deadline keeps what it found and
/// answers every relaxed walk as cut.
fn probe_lifted(
    options: &TextSearchOptions,
    walk: Walk,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
) -> Result<Tagged, ()> {
    // The files the row's view lists: those without a match for a
    // `filesWithout` row, else those with one.
    let mut lifted = options.probe(if options.output == SearchOutput::FilesWithoutMatch {
        SearchOutput::FilesWithoutMatch
    } else {
        SearchOutput::FilesWithMatch
    });
    lifted.ignore = octocode_engine::search::walk::IgnorePolicy {
        include_ignored: walk.ignored,
        include_hidden: walk.hidden,
    };
    let root = std::path::PathBuf::from(&options.path);
    let globs = Globs {
        include: options.include.as_deref().unwrap_or_default(),
        exclude: options.exclude.as_deref().unwrap_or_default(),
    };
    let (searched, _) = probe_search(lifted, paths, walk.default_excludes, cancel).ok_or(())?;
    let cut = !searched.stats.cap_reasons.is_empty();
    Tagged::found(
        &root,
        globs,
        searched.files.iter().map(|file| root.join(&file.path)),
        cut,
    )
    .map_err(|_| ())
}
