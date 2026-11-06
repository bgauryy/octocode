//! GitHub search query syntax: literal keyword terms, qualifier values, and
//! name validation shared by code, repository, and history search.
//!
//! Every caller-supplied value is emitted as either a bare word that GitHub
//! cannot read as syntax, or one quoted phrase whose inner quotes are escaped,
//! so it cannot close early. That
//! keeps a keyword such as `"hello" NOT` from splicing a boolean operator onto
//! the next qualifier, and keeps an owner such as `a OR is:public` from
//! widening the scope.

use super::{ProviderError, ProviderErrorKind};

/// GitHub search treats bare OR/AND/NOT as boolean operators.
fn reserved_operator(value: &str) -> bool {
    ["OR", "AND", "NOT"]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word))
}

/// A word GitHub reads as one literal term: word characters and inner
/// hyphens, not a leading `-` (exclusion) and not an operator.
fn bare_term(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|first| first.is_alphanumeric() || first == '_')
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        && !reserved_operator(value)
}

/// `"..."` with no interior quote or backslash: already one closed phrase.
fn closed_phrase(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"') && {
        let inner = &value[1..value.len() - 1];
        !inner.contains(['"', '\\']) && !inner.trim().is_empty()
    }
}

/// Quote `value` as one phrase. Double quotes and backslashes inside it are
/// escaped (`\"`, `\\`), as GitHub search reads them in a phrase: the phrase
/// cannot close early, and a keyword that spells quotes (`"name": "ws"` in
/// JSON) matches them; dropping them finds nothing there. A value of only
/// quotes, backslashes and whitespace has nothing to search.
pub fn search_phrase(value: &str) -> String {
    let words = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().all(|c| matches!(c, '"' | '\\' | ' ')) {
        return String::new();
    }
    let mut phrase = String::with_capacity(words.len() + 2);
    phrase.push('"');
    for c in words.chars() {
        if matches!(c, '"' | '\\') {
            phrase.push('\\');
        }
        phrase.push(c);
    }
    phrase.push('"');
    phrase
}

/// One literal keyword term; empty when nothing searchable remains.
pub fn quote_search_keyword(keyword: &str) -> String {
    let trimmed = keyword.trim();
    if closed_phrase(trimmed) || bare_term(trimmed) {
        trimmed.to_owned()
    } else {
        search_phrase(trimmed)
    }
}

/// A qualifier value (`key:value`): raw when it holds no whitespace, quote,
/// backslash, or parenthesis; otherwise one quoted phrase.
pub fn qualifier_value(value: &str) -> String {
    if closed_phrase(value) {
        return value.to_owned();
    }
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\\' | '(' | ')'))
    {
        search_phrase(value)
    } else {
        value.to_owned()
    }
}

/// Reject a person qualifier value (`author:`, `assignee:`…: a login,
/// `app/name`, `name[bot]`, or a commit email) that GitHub could not hold
/// and that would change the search syntax (spaces, quotes, colons,
/// parentheses, operators). Owner and repository names are validated by the
/// tool contract at admission.
pub fn validate_person_name(field: &str, value: &str) -> Result<(), ProviderError> {
    let allowed = |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']' | '/' | '@' | '+')
    };
    if !value.is_empty() && value.chars().all(allowed) && !value.starts_with('-') {
        return Ok(());
    }
    Err(ProviderError::new(
        ProviderErrorKind::Validation,
        format!(
            "{field}: \"{value}\" is not a GitHub login or email; spaces, quotes, colons, and search operators are not allowed"
        ),
    ))
}

/// Reject a free-form qualifier value that would break out of its term.
pub fn validate_qualifier_value(field: &str, value: &str) -> Result<(), ProviderError> {
    if !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\\' | '(' | ')'))
    {
        return Ok(());
    }
    Err(ProviderError::new(
        ProviderErrorKind::Validation,
        format!("{field}: \"{value}\" must be one term without spaces, quotes, or parentheses"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_never_carry_operators_or_qualifiers_out_of_their_term() {
        assert_eq!(quote_search_keyword("fix login"), "\"fix login\"");
        assert_eq!(quote_search_keyword("already"), "already");
        assert_eq!(quote_search_keyword("\"exact phrase\""), "\"exact phrase\"");
        assert_eq!(quote_search_keyword("\"hello\" NOT"), r#""\"hello\" NOT""#);
        assert_eq!(
            quote_search_keyword("x\" OR is:public"),
            r#""x\" OR is:public""#
        );
        // JSON text keeps its quotes: GitHub matches `"name": "ws"` only so.
        assert_eq!(
            quote_search_keyword("\"name\": \"ws\""),
            r#""\"name\": \"ws\"""#
        );
        assert_eq!(quote_search_keyword("repo:other/x"), "\"repo:other/x\"");
        assert_eq!(quote_search_keyword("-excluded"), "\"-excluded\"");
        assert_eq!(quote_search_keyword("NOT"), "\"NOT\"");
        assert_eq!(quote_search_keyword("trail\\"), r#""trail\\""#);
        assert_eq!(quote_search_keyword("\"\""), "");
    }

    #[test]
    fn qualifier_values_stay_one_term() {
        assert_eq!(qualifier_value("rust"), "rust");
        assert_eq!(qualifier_value("Common Lisp"), "\"Common Lisp\"");
        assert_eq!(qualifier_value("\"a b\""), "\"a b\"");
        assert_eq!(qualifier_value("\"a\" OR b"), r#""\"a\" OR b""#);
        assert_eq!(qualifier_value("x)"), "\"x)\"");
    }

    #[test]
    fn person_names_reject_syntax() {
        for bad in ["octocat OR is:public", "a:b", "a\"b", "", "-x", "a b"] {
            assert!(validate_person_name("author", bad).is_err(), "{bad}");
        }
        for good in [
            "octocat",
            "under_score",
            "dependabot[bot]",
            "app/dependabot",
            "dev+x@example.com",
        ] {
            assert!(validate_person_name("author", good).is_ok(), "{good}");
        }
        assert!(validate_qualifier_value("created", ">2026-01-01").is_ok());
        assert!(validate_qualifier_value("created", "x OR y").is_err());
    }
}
