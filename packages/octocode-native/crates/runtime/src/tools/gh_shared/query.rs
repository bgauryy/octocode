//! GitHub search query syntax shared by ghSearchRepo and ghSearchCode,
//! matching the canonical provider query builders.
use crate::providers::github::{ProviderError, SearchName, qualifier_value, validate_search_name};

/// Emit `key:value` as one term (quoted when the value holds whitespace,
/// quotes, or parentheses) so it cannot split into stray keywords.
pub(crate) fn push(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        let value = qualifier_value(value);
        if !value.is_empty() {
            parts.push(format!("{key}:{value}"));
        }
    }
}

/// Owner and repository names become `repo:`/`user:` scopes; reject any value
/// that is not a GitHub name before it can rewrite the scope.
pub(crate) fn validate_scope(owner: Option<&str>, repo: Option<&str>) -> Result<(), ProviderError> {
    if let Some(owner) = owner {
        validate_search_name("owner", owner, SearchName::Owner)?;
    }
    if let Some(repo) = repo {
        validate_search_name("repo", repo, SearchName::Repository)?;
    }
    Ok(())
}

/// Range qualifiers (`>100`, `a..b`) never contain meaningful whitespace;
/// strip it instead of quoting so the range syntax still applies. A bare
/// relative window (`30d`, `2w`, `6m`, `1y`) on a date qualifier resolves to
/// an absolute `>=YYYY-MM-DD` lower bound, which GitHub understands.
pub(crate) fn range_value(value: &str, date: bool) -> String {
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

/// `qualifiers`: space-separated `key:value` filters (the contract allowlists
/// the keys). Each is re-emitted through the same range/date normalization
/// as the dedicated fields, so a value can never start a new term.
pub(crate) fn push_qualifiers(parts: &mut Vec<String>, qualifiers: &str) {
    for term in qualifiers.split_whitespace() {
        let Some((key, value)) = term.split_once(':') else {
            continue;
        };
        if key == "is" {
            push(parts, key, Some(value));
        } else {
            let value = range_value(value, matches!(key, "created" | "pushed"));
            push(parts, key, Some(&value));
        }
    }
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
}
