//! The one GitHub search-term builder ghSearchRepo, ghSearchCode and
//! ghSearchHistory share: keywords, the `repo:`/`user:` scope and
//! qualifiers under one rule each, so a value can never start a new term
//! and a relative date window means the same date in every tool.
use octocode_github::{ProviderError, ProviderErrorKind, qualifier_value, quote_search_keyword};

/// Qualifiers whose value is a date (or a date range): a bare relative
/// window (`30d`, `2w`, `6m`, `1y`, `12h`) on one resolves to an absolute
/// `>=YYYY-MM-DD` lower bound, which GitHub understands.
const DATE_KEYS: &[&str] = &[
    "created",
    "pushed",
    "updated",
    "merged",
    "closed",
    "committer-date",
    "author-date",
];

fn is_date_key(key: &str) -> bool {
    DATE_KEYS.contains(&key)
}

/// The terms of one search query, in the order they are added.
#[derive(Debug, Default)]
pub(crate) struct SearchTerms(Vec<String>);

impl SearchTerms {
    /// Terms starting with `keywords`, each one term (quoted when it holds
    /// whitespace or search operators).
    pub(crate) fn new<'a>(keywords: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            keywords
                .into_iter()
                .map(quote_search_keyword)
                .filter(|term| !term.is_empty())
                .collect(),
        )
    }

    /// `repo:owner/repo` when both are named, else `user:owner`.
    pub(crate) fn scope(&mut self, owner: Option<&str>, repo: Option<&str>) {
        match (owner, repo) {
            (Some(owner), Some(repo)) => self.text("repo", Some(&format!("{owner}/{repo}"))),
            (owner, _) => self.text("user", owner),
        }
    }

    /// `key:value` as one term, quoted when the value holds whitespace,
    /// quotes, or parentheses.
    pub(crate) fn text(&mut self, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            let value = qualifier_value(value);
            if !value.is_empty() {
                self.0.push(format!("{key}:{value}"));
            }
        }
    }

    /// A range qualifier (`>100`, `a..b`, a date): whitespace is dropped so
    /// the range syntax still applies, and a date key's relative window
    /// resolves ([`range_value`]).
    pub(crate) fn range(&mut self, key: &str, value: Option<&str>) {
        let value = value.map(|value| range_value(value, is_date_key(key)));
        self.text(key, value.as_deref());
    }

    /// `key:value` that must already be one term: a range is compacted and
    /// a date key's relative window resolves, then a value that would open
    /// a new term, or a date key's value that is no date, is refused.
    pub(crate) fn checked(&mut self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
        if let Some(value) = value {
            self.0.push(checked_term(key, value)?);
        }
        Ok(())
    }

    /// `-key:value` under [`Self::checked`]'s rule.
    pub(crate) fn excluded(&mut self, key: &str, value: &str) -> Result<(), ProviderError> {
        self.0.push(format!("-{}", checked_term(key, value)?));
        Ok(())
    }

    /// A term the caller built from fixed text (`is:pr`, `archived:false`).
    pub(crate) fn push(&mut self, term: impl Into<String>) {
        self.0.push(term.into());
    }

    /// `qualifiers`: space-separated `key:value` filters (the contract
    /// allowlists the keys), each re-emitted through [`Self::range`] (`is:`
    /// as plain text), so a value can never start a new term.
    pub(crate) fn qualifiers(&mut self, qualifiers: &str) {
        for term in qualifiers.split_whitespace() {
            let Some((key, value)) = term.split_once(':') else {
                continue;
            };
            if key == "is" {
                self.text(key, Some(value));
            } else {
                self.range(key, Some(value));
            }
        }
    }

    pub(crate) fn finish(self) -> String {
        self.0.join(" ").trim().into()
    }
}

/// One `key:value` term under [`SearchTerms::checked`]'s rule.
fn checked_term(key: &str, value: &str) -> Result<String, ProviderError> {
    let date = is_date_key(key);
    let range = date || value.trim_start().starts_with(['<', '>', '=']) || value.contains("..");
    let value = if range {
        range_value(value, date)
    } else {
        value.trim().to_owned()
    };
    octocode_github::validate_qualifier_value(key, &value)?;
    if date && !is_date_range(&value) {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "{key}: \"{value}\" is not a date; use a relative window like \"30d\", \"2w\", \"6m\", \"1y\", an ISO date like \"2026-01-01\", or a range like \">=2026-01-01\" or \"2026-01-01..2026-02-01\"."
            ),
        ));
    }
    Ok(format!("{key}:{value}"))
}

/// GitHub's date qualifier syntax: an optional comparison and one date, or
/// `start..end` where either side may be `*`. A date is `YYYY-MM-DD`,
/// optionally followed by a time (`T10:00:00Z`, `T10:00:00+02:00`).
fn is_date_range(value: &str) -> bool {
    let is_date = |text: &str| {
        let bytes = text.as_bytes();
        let digits = |at: usize, count: usize| {
            bytes
                .get(at..at + count)
                .is_some_and(|run| run.iter().all(u8::is_ascii_digit))
        };
        let time = |rest: &str| {
            rest.strip_prefix('T').is_some_and(|clock| {
                !clock.is_empty()
                    && clock
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | 'Z' | '+' | '-'))
            })
        };
        digits(0, 4)
            && bytes.get(4) == Some(&b'-')
            && digits(5, 2)
            && bytes.get(7) == Some(&b'-')
            && digits(8, 2)
            && (bytes.len() == 10 || text.get(10..).is_some_and(time))
    };
    if let Some((start, end)) = value.split_once("..") {
        return (start == "*" || is_date(start)) && (end == "*" || is_date(end)) && start != end;
    }
    let date = [">=", "<=", ">", "<"]
        .iter()
        .find_map(|operator| value.strip_prefix(operator))
        .unwrap_or(value);
    is_date(date)
}

/// Range qualifiers (`>100`, `a..b`) never contain meaningful whitespace;
/// strip it instead of quoting so the range syntax still applies. A bare
/// relative window (`30d`, `2w`, `6m`, `1y`) on a date qualifier resolves to
/// an absolute `>=YYYY-MM-DD` lower bound, which GitHub understands.
fn range_value(value: &str, date: bool) -> String {
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let relative = compact
        .as_bytes()
        .split_last()
        .is_some_and(|(unit, digits)| {
            matches!(unit, b'h' | b'd' | b'w' | b'm' | b'y')
                && !digits.is_empty()
                && digits.iter().all(u8::is_ascii_digit)
        });
    if date
        && relative
        && let Some(resolved) = crate::providers::github::resolve_date_window(&compact).value
    {
        return format!(">={}", resolved.chars().take(10).collect::<String>());
    }
    compact
}

/// A typed search query parsed from a JSON test fixture.
#[cfg(test)]
pub(crate) fn parse<Q: serde::de::DeserializeOwned>(value: serde_json::Value) -> Q {
    serde_json::from_value(value).expect("query fixture")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_qualifiers_preserve_unicode_without_panicking() {
        for value in ["💥", "10💥", "é", "１２d"] {
            assert_eq!(range_value(value, false), value);
            assert_eq!(range_value(value, true), value);
        }
    }

    #[test]
    fn date_ranges_follow_githubs_syntax() {
        for value in [
            "2026-01-01",
            ">=2026-01-01",
            "<2026-01-01T10:00:00Z",
            "2026-01-01T10:00:00+02:00",
            "2026-01-01..2026-02-01",
            "*..2026-01-01",
            "2026-01-01..*",
        ] {
            assert!(is_date_range(value), "{value}");
        }
        for value in [
            "30d",
            "yesterday",
            "2026",
            "2026-1-1",
            "*..*",
            ">=",
            "2026-01-01X",
            "2026-01-01T",
        ] {
            assert!(!is_date_range(value), "{value}");
        }
    }
}
