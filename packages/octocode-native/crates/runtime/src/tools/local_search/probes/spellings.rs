//! Near-miss probes of an empty search: one-edit spellings, identifier
//! variants and word parts, and ignore-case.

use super::super::local_search_query;
use super::*;

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
pub(in crate::tools::local_search) fn identifier_variants(term: &str) -> Option<Vec<String>> {
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
pub(in crate::tools::local_search) fn one_edit_spellings(term: &str) -> Option<Vec<String>> {
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
pub(in crate::tools::local_search) fn probe_variants(
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
pub(in crate::tools::local_search) struct Variants {
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
pub(in crate::tools::local_search) fn disclose_variants(
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
