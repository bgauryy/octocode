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
use crate::tools::leads::skipped::{SkippedEntry, skipped_cause, skipped_paths};
use octocode_contract_types::SkippedPaths;
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SpanValues, TextSearchOptions,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// What the re-run of an empty search over its skipped paths found: the
/// files, each tagged with the causes that hid it, and what the path policy
/// withheld from that walk (symlinks it does not follow, credential files).
pub(super) struct Lifted {
    tagged: Tagged,
    withheld: crate::policy::discovery::Withheld,
}

/// Re-run an empty search over what the defaults leave out (ignored and
/// hidden entries, the default-excluded directories), each file found
/// tagged with the causes that hid it ([`probe_lifted`]).
pub(super) fn probe_unsearched(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    search_options: &TextSearchOptions,
    cancel: &impl CancellationCheck,
) -> Result<Lifted, ()> {
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
    lifted: Result<Lifted, ()>,
    skipped: Skipped<'_>,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Unsearched, ToolError> {
    let walk = query_walk(query);
    let root = std::path::PathBuf::from(&search_options.path);
    let verdict = diagnose(&root, walk, |relaxed| {
        Ok::<_, std::convert::Infallible>(lifted.as_ref().map_or_else(
            |()| Hits {
                cut: true,
                ..Hits::default()
            },
            |lifted| lifted.tagged.find(relaxed, None),
        ))
    });
    let Ok(verdict) = verdict;
    if let Verdict::Absent = verdict {
        if let Ok(lifted) = &lifted {
            absent(query, &skipped, &lifted.withheld, hints, warnings);
        }
        return Ok(Unsearched::default());
    }
    let subject = if query.result_view == LocalSearchQueryResultView::FilesWithout {
        ["file that lacks a match", "files that lack a match"]
    } else {
        ["file that matches", "files that match"]
    };
    let named = skipped.walk.counts();
    let Some(disclosure) = crate::tools::leads::skipped::disclose(
        &verdict,
        subject,
        crate::tools::leads::skipped::Unchecked {
            causes: &skipped.causes,
            named: &named,
        },
    ) else {
        return Ok(Unsearched::default());
    };
    insert_lead(
        next,
        crate::tools::leads::skipped::lead(
            crate::id::ToolId::LocalSearch,
            query,
            &disclosure.causes,
        ),
    );
    // A check that ran out of time names the ignored directories a fresh
    // bounded walk finds, with their rules; a finished one names the rules
    // behind the files it found.
    let (named, unchecked) = match &verdict {
        Verdict::Skipped(diagnosis) => (diagnosis.skips().to_vec(), false),
        _ => (unsearched_ignored(search_options, walk), true),
    };
    if unchecked {
        warnings.push(disclosure.text);
    } else {
        hints.insert(0, disclosure.text);
    }
    Ok(Unsearched {
        may_hold_matches: true,
        unchecked,
        observed: observed_paths(query, &named)?,
    })
}

/// The check of the skipped paths searched them and found nothing, so no
/// lead lifts them: the hint says so, and what the path policy withheld
/// from that walk (a skipped directory of symlinks) is counted in a warning,
/// since absence there is unproven.
fn absent(
    query: &LocalSearchQuery,
    skipped: &Skipped<'_>,
    withheld: &crate::policy::discovery::Withheld,
    hints: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let counts = skipped.walk.counts();
    if counts.is_empty() {
        return;
    }
    let what = if query.result_view == LocalSearchQueryResultView::FilesWithout {
        "file lacking a match"
    } else {
        "match"
    };
    let hint = match withheld.notice() {
        Some(notice) => {
            warnings.push(format!("Skipped paths: {notice}"));
            format!(
                "Skipped paths ({counts}) hold no {what} outside what their check withheld (warnings)."
            )
        }
        None => format!("Skipped paths ({counts}) hold no {what} either; the check searched them."),
    };
    hints.insert(0, hint);
}

/// What a page's walk skipped, by cause, for the empty page's check of it.
pub(super) struct Skipped<'a> {
    pub(super) walk: &'a crate::policy::discovery::WalkSkips,
    /// The causes it skipped paths by: what a lead over them lifts.
    pub(super) causes: Vec<Cause>,
}

/// What the empty page's check of the skipped paths found.
#[derive(Default)]
pub(super) struct Unsearched {
    /// Skipped paths hold a match, or may: the check did not finish.
    pub(super) may_hold_matches: bool,
    /// The check did not finish: the skipped paths are unsearched scope
    /// ([`crate::tools::leads::skipped::SKIPPED_UNCHECKED`]).
    pub(super) unchecked: bool,
    /// The skipped paths the check named, each with its rule.
    pub(super) observed: Option<SkippedPaths>,
}

fn insert_lead(next: &mut Option<Value>, lead: Value) {
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("includeSkipped".into(), lead);
    }
}

/// The skipped paths a check named, one entry per cause and rule (its first
/// path, with `count` when the rule hides more), or every path with
/// `extras:["details"]`; the `includeSkipped` lead lifts them all either way.
/// A check's findings are samples (`sampled`).
fn observed_paths(
    query: &LocalSearchQuery,
    skipped: &[crate::tools::empty_walk::Skip],
) -> Result<Option<SkippedPaths>, ToolError> {
    skipped_paths(
        skipped.iter().map(|skip| {
            SkippedEntry::path(
                octocode_engine::text::slash_path(&skip.entry),
                skipped_cause(skip.cause),
            )
            .because(skip.reason.clone())
        }),
        lists_every_skipped_path(query),
        true,
    )
}

/// The ignored directories a walk skipped that could hold a path the
/// search's include globs match, named, within a fresh
/// [`PROBE_BUDGET`](crate::tools::empty_walk::PROBE_BUDGET) of their own
/// (the probe that ran out of time left none of the diagnosis' budget): the
/// rules the lead lifts.
fn unsearched_ignored(
    options: &TextSearchOptions,
    walk: Walk,
) -> Vec<crate::tools::empty_walk::Skip> {
    let root = std::path::Path::new(&options.path);
    let scope = octocode_engine::search::glob::IncludeScope::new(
        root,
        options.include.as_deref().unwrap_or_default(),
    );
    let started = std::time::Instant::now();
    let allowance = crate::tools::empty_walk::PROBE_BUDGET;
    let mut skips = crate::tools::empty_walk::ignored_directories(root, walk, &|| {
        started.elapsed() > allowance
    });
    skips.retain(|skip| {
        scope.reaches(
            &skip.entry,
            skip.entry.strip_prefix(root).unwrap_or(&skip.entry),
            true,
        )
    });
    skips
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
    probe_walked(options, paths, defaults, cancel).map(|(found, cut, _)| (found, cut))
}

/// [`probe_search`] with what the path policy withheld from its walk.
fn probe_walked(
    options: TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> Option<(
    octocode_engine::types::TextSearchResult,
    bool,
    crate::policy::discovery::Withheld,
)> {
    let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
        paths.clone(),
        std::path::PathBuf::from(&options.path),
        defaults,
        options.include.as_deref().unwrap_or_default(),
    ));
    let started = std::time::Instant::now();
    let deadline = crate::tools::empty_walk::probe_budget();
    let cut = std::sync::atomic::AtomicBool::new(false);
    let found = crate::tools::leads::best_effort(|| {
        search_cancellable(options, walk.clone(), &|| {
            let late = started.elapsed() > deadline;
            if late {
                cut.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            cancel.check().is_err() || late
        })
    })?
    .ok()?;
    Some((
        found,
        cut.load(std::sync::atomic::Ordering::Relaxed),
        walk.skipped().withheld,
    ))
}

/// One bounded walk over the query's own scope and walk (its default
/// excludes included, as the `findSimilar` lead runs it) for `folded` (the
/// searched text) in any case, `variants` anywhere in a line and `near`
/// spellings as whole words, naming which occur in which files. The row's
/// own `wholeWord` and `multiline` apply, as the leads keep them.
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
    options.invert_match = Some(false);
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
    let whole_word = query.whole_word == Some(true);
    let spellings = folded.is_some() || !variants.is_empty() || !near.is_empty();
    if !spellings && !whole_word {
        return None;
    }
    let defaults = query.default_excludes.defaults();
    let found = if spellings {
        probe_spellings(options, folded, &variants, &near, paths, defaults, cancel)
    } else {
        Spellings::default()
    };
    // No whole word in any case: the text may occur inside longer words.
    let word_parts = (whole_word && found.folded_files == 0)
        .then(|| probe_word_parts(options, paths, defaults, cancel));
    Some(Variants {
        variants,
        found,
        word_parts,
    })
}

/// The files holding the row's text inside a longer word: its own search
/// with `wholeWord` off, and whether the deadline cut that count.
fn probe_word_parts(
    options: &TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> (usize, bool) {
    let mut options = options.probe(SearchOutput::FilesWithMatch);
    options.whole_word = Some(false);
    options.invert_match = Some(false);
    probe_search(options, paths, defaults, cancel)
        .map(|(found, cut)| (found.files.len(), cut))
        .unwrap_or_default()
}

/// The other spellings [`probe_variants`] tried, and where they occur.
pub(super) struct Variants {
    variants: Vec<String>,
    found: Spellings,
    /// A whole-word row's text inside longer words: files, and whether the
    /// deadline cut the count.
    word_parts: Option<(usize, bool)>,
}

/// Say when an empty literal search's text matches ignoring case, else
/// (a `wholeWord` row) inside longer words, else when an identifier's other
/// case styles match, else its spellings one edit away, so a spelling or
/// option miss does not read as absence: the first that matches leads
/// (`ignoreCase`, `broadenSearch` without `wholeWord`, else `findSimilar`).
pub(super) fn disclose_variants(
    query: &LocalSearchQuery,
    Variants {
        variants,
        found,
        word_parts,
    }: Variants,
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
    } else if let Some((files, cut)) = word_parts.filter(|(files, _)| *files > 0) {
        (
            "broadenSearch",
            format!(
                "No match for {} as a whole word; it matches inside longer words in {}{files} file(s); wholeWord excludes them.",
                local_search_query::pattern(query),
                if cut { "at least " } else { "" },
            ),
            restart_lead(query, |fields| {
                fields.remove("wholeWord");
            }),
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
        map.entry(name).or_insert(lead);
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
) -> Result<Lifted, ()> {
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
    let (searched, _, withheld) =
        probe_walked(lifted, paths, walk.default_excludes, cancel).ok_or(())?;
    let cut = !searched.stats.cap_reasons.is_empty();
    let tagged = Tagged::found(
        &root,
        globs,
        searched.files.iter().map(|file| root.join(&file.path)),
        cut,
    )
    .map_err(|_| ())?;
    Ok(Lifted { tagged, withheld })
}
