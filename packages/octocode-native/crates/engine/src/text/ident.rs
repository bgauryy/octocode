//! The one identifier-boundary rule: which characters continue an
//! identifier, whether a match of a name is a whole word, and the regex
//! that finds the same whole words.
//!
//! An identifier character is a Unicode word character (`\w`: letters,
//! marks, decimal digits, connector punctuation such as `_`, and the join
//! controls), a Unicode `ID_Continue` character (which adds characters
//! such as `℘` and `·`), or `$`. The union is conservative: a match next to
//! any character some language lets into an identifier is not a whole
//! word. `$` continues an identifier in JavaScript and
//! TypeScript (`$foo` is not `foo`); in the other supported languages it is
//! a sigil or macro metavariable prefix, which no server reports as part of
//! the plain name either, so one rule serves every language.

/// The identifier characters as the body of a regex class.
const CLASS: &str = r"\w\p{ID_Continue}$";

/// Whether `ch` continues an identifier.
#[must_use]
pub fn is_ident_char(ch: char) -> bool {
    if ch.is_ascii() {
        return ch == '_' || ch == '$' || ch.is_ascii_alphanumeric();
    }
    // Constant pattern: compile failure is a build-time bug, so fail loud.
    #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
    static IDENT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(&format!("^[{CLASS}]$")).expect("valid identifier class")
    });
    IDENT.is_match(ch.encode_utf8(&mut [0; 4]))
}

/// What sits next to a match: the text's edge, a character, or bytes that
/// do not decode as UTF-8.
enum Neighbor {
    Edge,
    Char(char),
    Undecodable,
}

impl Neighbor {
    /// Whether this neighbor joins the match into a longer identifier.
    /// Undecodable bytes may be part of one, so they count as joined.
    fn joins(&self) -> bool {
        match self {
            Self::Edge => false,
            Self::Char(ch) => is_ident_char(*ch),
            Self::Undecodable => true,
        }
    }
}

/// The character ending at byte `end` of `bytes`.
fn char_before(bytes: &[u8], end: usize) -> Neighbor {
    if end == 0 {
        return Neighbor::Edge;
    }
    (1..=4.min(end))
        .find_map(|len| std::str::from_utf8(&bytes[end - len..end]).ok())
        .and_then(|text| text.chars().next_back())
        .map_or(Neighbor::Undecodable, Neighbor::Char)
}

/// The character starting at byte `start` of `bytes`.
fn char_after(bytes: &[u8], start: usize) -> Neighbor {
    if start >= bytes.len() {
        return Neighbor::Edge;
    }
    (1..=4.min(bytes.len() - start))
        .find_map(|len| std::str::from_utf8(&bytes[start..start + len]).ok())
        .and_then(|text| text.chars().next())
        .map_or(Neighbor::Undecodable, Neighbor::Char)
}

/// Whether `bytes[start..end]` is a whole word: no identifier character
/// joins either end. An end of the word that is itself not an identifier
/// character (`.name`) needs no boundary. Bytes that are not UTF-8 next to
/// an identifier end count as joined.
#[must_use]
pub fn is_whole_word(bytes: &[u8], start: usize, end: usize) -> bool {
    if start >= end || end > bytes.len() {
        return false;
    }
    let word = &bytes[..end];
    let starts_ident = char_after(word, start).joins();
    let ends_ident = char_before(word, end).joins();
    (!starts_ident || !char_before(bytes, start).joins())
        && (!ends_ident || !char_after(bytes, end).joins())
}

/// Byte offsets in `text` where `word` occurs as a whole word
/// ([`is_whole_word`]).
pub fn whole_word_starts<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .map(|(at, _)| at)
        .filter(move |&at| !word.is_empty() && is_whole_word(text.as_bytes(), at, at + word.len()))
}

/// A regex (default `rust` engine, `^`/`$` at line boundaries) matching
/// `name` exactly where [`whole_word_starts`] finds it. The boundary is
/// consumed (the engine has no lookaround), so a match may include one
/// neighbor character; the matching lines and files are the same.
#[must_use]
pub fn word_pattern(name: &str) -> String {
    let edge = |ch: Option<char>| ch.is_some_and(is_ident_char);
    let mut pattern = regex::escape(name);
    if edge(name.chars().next()) {
        pattern.insert_str(0, &format!("(?:^|[^{CLASS}])"));
    }
    if edge(name.chars().next_back()) {
        pattern.push_str(&format!("(?:[^{CLASS}]|$)"));
    }
    pattern
}

#[cfg(test)]
mod tests;
