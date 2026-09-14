//! Stable match ranking with Unicode collation for repository tie-breaking.
use crate::providers::github::{ProviderError, ProviderErrorKind};
use serde_json::Value;
use std::{cmp::Reverse, sync::OnceLock};

pub(super) struct Match {
    pub path: String,
    pub value: Value,
    pub score: u8,
}
pub(super) struct Group {
    pub id: String,
    pub matches: Vec<Match>,
}
pub(super) struct Term {
    lower: String,
    word: Option<regex::Regex>,
}

pub(super) fn terms(keywords: &[String]) -> Result<Vec<Term>, ProviderError> {
    keywords
        .iter()
        .map(|value| value.trim())
        .filter(|v| !v.is_empty())
        .map(|value| {
            let word = if value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                Some(
                    regex::Regex::new(&format!(r"(?i-u:\b{}\b)", regex::escape(value))).map_err(
                        |error| {
                            ProviderError::new(ProviderErrorKind::Configuration, error.to_string())
                        },
                    )?,
                )
            } else {
                None
            };
            Ok(Term {
                lower: value.to_lowercase(),
                word,
            })
        })
        .collect()
}

pub(super) fn score(path: &str, value: &str, terms: &[Term]) -> u8 {
    let base = path.rsplit('/').next().unwrap_or_default().to_lowercase();
    let stem = base
        .rsplit_once('.')
        .map_or(base.as_str(), |(stem, _)| stem);
    let mut score = 0;
    for term in terms {
        if base == term.lower || stem == term.lower {
            return 2;
        }
        if term.word.as_ref().map_or_else(
            || value.to_lowercase().contains(&term.lower),
            |pattern| pattern.is_match(value),
        ) {
            score = 1;
        }
    }
    score
}

pub(super) fn sort(groups: &mut [Group]) -> Result<(), ProviderError> {
    static COLLATOR: OnceLock<Result<icu_collator::CollatorBorrowed<'static>, String>> =
        OnceLock::new();
    let collator = COLLATOR
        .get_or_init(|| {
            icu_collator::Collator::try_new(Default::default(), Default::default())
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| ProviderError::new(ProviderErrorKind::Configuration, error.clone()))?;
    for group in groups.iter_mut() {
        group.matches.sort_by_key(|value| Reverse(value.score));
    }
    groups.sort_by(|left, right| {
        let left_exact = left.matches.first().is_some_and(|value| value.score > 0);
        let right_exact = right.matches.first().is_some_and(|value| value.score > 0);
        right_exact
            .cmp(&left_exact)
            .then_with(|| right.matches.len().cmp(&left.matches.len()))
            .then_with(|| collator.compare(&left.id, &right.id))
    });
    Ok(())
}
