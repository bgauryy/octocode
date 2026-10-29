//! The one split of identifiers and free text into lowercase search words.

/// Lowercase words of `text`: runs of letters and digits, split again where
/// camelCase, PascalCase or an acronym starts a word (`HTTPServer` gives
/// `http`, `server`). Every other character, such as space, `_`, `-`, `/`,
/// `.`, `#` or `::`, separates words.
#[must_use]
pub fn identifier_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous = None::<char>;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let boundary = ch.is_uppercase()
            && previous.is_some_and(|last| {
                last.is_lowercase()
                    || last.is_numeric()
                    || (last.is_uppercase() && chars.peek().is_some_and(|next| next.is_lowercase()))
            });
        if (!ch.is_alphanumeric() || boundary) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if ch.is_alphanumeric() {
            word.extend(ch.to_lowercase());
        }
        previous = Some(ch);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::identifier_words;

    #[test]
    fn identifier_words_and_unicode_are_searchable() {
        assert_eq!(
            identifier_words("HTTPServer retryConnection snake_case café"),
            [
                "http",
                "server",
                "retry",
                "connection",
                "snake",
                "case",
                "café"
            ]
        );
    }

    #[test]
    fn paths_and_qualified_names_split_at_every_separator() {
        assert_eq!(
            identifier_words("src/pool.rs#OriginPool.read crate::io::read_to-end"),
            [
                "src", "pool", "rs", "origin", "pool", "read", "crate", "io", "read", "to", "end"
            ]
        );
    }
}
