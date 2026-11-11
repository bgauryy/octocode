//! What a value test reads of the key before a value (B165).
//!
//! A value test ([`super::credential_values::ValueTest`]) reads its key
//! through a few facts. The keys of one path (`secret/data/app/token: v`) share
//! their value and its end and differ in where they start, and each fact of a
//! key is whether some mark lies at or after its start: a chosen word in the
//! key's name, a separator that makes the name a file path, a dot, or a word of
//! a fallback read. So each is read off the last such mark of the first key of
//! the path, and judging one more key of the path costs nothing, however long
//! the path is. The key is the first key's text with its head cut off, and
//! what a test reads of its end (quotes, an operator, the name's tail) is the
//! same for every key of the path.
//!
//! That holds for a key text `path + tail`: the path is made of key
//! characters, `/`, `[` and `]`, and the tail starts with the operator.

use std::cell::OnceCell;

use super::credential_values::{
    CHOSEN_CREDENTIAL_WORDS, QUOTES, ends_in_locator, has_file_extension, is_fallback_read,
    key_name, opens_interpolation,
};
use super::is_source_code_path;
use super::named_values::{describes_a_credential, names_a_published_value};
use crate::security::patterns::CREDENTIAL_WORDS;

/// The facts of a key a value test reads.
pub(super) trait KeyFacts {
    /// Its name, or a name of the line when it reads a fallback, holds a word
    /// for a value a person chooses ([`CHOSEN_CREDENTIAL_WORDS`]).
    fn names_a_chosen_credential(&self) -> bool;
    /// Its name ends in a locator word (`path`, `file`, `url`).
    fn names_a_locator(&self) -> bool;
    /// Its name describes or identifies a credential instead of holding it
    /// (`secretName`, `API_KEY_HEADER`, `kmsKeyId`:
    /// [`describes_a_credential`]).
    fn describes_a_credential(&self) -> bool;
    /// Its name says its value is published (`publicKey`:
    /// [`names_a_published_value`]).
    fn names_a_published_value(&self) -> bool;
    /// Its name holds a credential word, or it reads a fallback, where a
    /// name of the line does. A key with neither is another name the
    /// pattern read past a credential word (`type="password" name="…"`).
    fn names_a_credential(&self) -> bool;
    /// Its name is a file: a path with a file name's extension, or a source file.
    fn names_a_file(&self) -> bool;
    /// The value opens with a quote.
    fn ends_with_quote(&self) -> bool;
    /// The key ends in a colon, blanks aside.
    fn ends_with_colon(&self) -> bool;
    /// The value opens a literal whose prefix interpolates its `{…}` holes
    /// (`f'`, `$"`: [`opens_interpolation`]).
    fn opens_interpolation(&self) -> bool;
}

/// The keys that start in one text before one value, as the text of the
/// first and facts read off it once.
pub(super) struct SpanKeys<'a> {
    /// The first key's text, to the value.
    text: &'a str,
    name: OnceCell<NameFacts>,
    fallback: OnceCell<Option<usize>>,
}

/// What the first key's name says, as byte offsets in its text.
struct NameFacts {
    start: usize,
    end: usize,
    last_chosen: Option<usize>,
    last_separator: Option<usize>,
    last_dot: Option<usize>,
    has_extension: bool,
    is_source: bool,
}

impl<'a> SpanKeys<'a> {
    /// The keys of `text`, its first key's text to the value.
    pub(super) fn new(text: &'a str) -> Self {
        Self {
            text,
            name: OnceCell::new(),
            fallback: OnceCell::new(),
        }
    }

    /// The key that starts `drop` bytes after the first.
    pub(super) fn key(&self, drop: usize) -> SpanKey<'_, 'a> {
        SpanKey { span: self, drop }
    }

    fn name(&self) -> &NameFacts {
        self.name.get_or_init(|| {
            let name = key_name(self.text);
            let start = (name.as_ptr() as usize).saturating_sub(self.text.as_ptr() as usize);
            let lower = name.to_ascii_lowercase();
            NameFacts {
                start,
                end: start + name.len(),
                last_chosen: CHOSEN_CREDENTIAL_WORDS
                    .iter()
                    .filter_map(|word| lower.rfind(word))
                    .max(),
                last_separator: name.rfind(['/', '\\']),
                last_dot: name.rfind('.'),
                has_extension: has_file_extension(name),
                is_source: is_source_code_path(name),
            }
        })
    }

    /// For a key that reads a fallback: where the last word of the line that
    /// says "chosen" starts. `None` when the key reads no fallback or no word
    /// says it.
    fn last_fallback_word(&self) -> Option<usize> {
        *self.fallback.get_or_init(|| {
            if !is_fallback_read(self.text) {
                return None;
            }
            let lower = self.text.to_ascii_lowercase();
            let bytes = lower.as_bytes();
            // A `pass` segment: no letter or digit beside it.
            let edge = |at: Option<usize>| {
                at.and_then(|at| bytes.get(at))
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric())
            };
            let pass = lower
                .match_indices("pass")
                .map(|(at, _)| at)
                .filter(|&at| edge(at.checked_sub(1)) && edge(Some(at + "pass".len())));
            CHOSEN_CREDENTIAL_WORDS
                .iter()
                .filter_map(|word| lower.rfind(word))
                .chain(pass)
                .max()
        })
    }
}

/// One key of [`SpanKeys`].
pub(super) struct SpanKey<'s, 'a> {
    span: &'s SpanKeys<'a>,
    /// Bytes between the first key's start and this one's.
    drop: usize,
}

impl SpanKey<'_, '_> {
    /// Where this key's name starts in the first key's text, and the name's
    /// bytes cut to it: a name that runs from its key's start loses what the
    /// key loses; one that starts after a separator is the same for all.
    fn name_from(&self) -> (usize, &[u8]) {
        let name = self.span.name();
        let cut = if name.start == 0 { self.drop } else { 0 };
        let bytes = self
            .span
            .text
            .as_bytes()
            .get(name.start + cut..name.end)
            .unwrap_or_default();
        (cut, bytes)
    }

    /// The facts as plain values, for a value test to judge once for every key
    /// that has the same ([`KeyBits`]).
    pub(super) fn bits(&self) -> KeyBits {
        KeyBits {
            chosen: self.names_a_chosen_credential(),
            locator: self.names_a_locator(),
            describes: self.describes_a_credential(),
            published: self.names_a_published_value(),
            credential: self.names_a_credential(),
            file: self.names_a_file(),
            quote: self.ends_with_quote(),
            colon: self.ends_with_colon(),
            interpolates: self.opens_interpolation(),
        }
    }
}

impl KeyFacts for SpanKey<'_, '_> {
    fn names_a_chosen_credential(&self) -> bool {
        let (cut, _) = self.name_from();
        // A name that describes a chosen credential holds none.
        (self.span.name().last_chosen.is_some_and(|last| last >= cut)
            && !self.describes_a_credential())
            || self
                .span
                .last_fallback_word()
                .is_some_and(|last| last >= self.drop)
    }

    fn names_a_locator(&self) -> bool {
        ends_in_locator(self.name_from().1)
    }

    fn describes_a_credential(&self) -> bool {
        std::str::from_utf8(self.name_from().1).is_ok_and(describes_a_credential)
    }

    fn names_a_published_value(&self) -> bool {
        std::str::from_utf8(self.name_from().1).is_ok_and(names_a_published_value)
    }

    fn names_a_credential(&self) -> bool {
        let holds_word = |name: &[u8]| {
            let name = name.to_ascii_lowercase();
            CREDENTIAL_WORDS.split('|').any(|word| {
                name.windows(word.len())
                    .any(|window| window == word.as_bytes())
            })
        };
        holds_word(self.name_from().1)
            || holds_word(adjacent_word(self.span.text).as_bytes())
            || is_fallback_read(self.span.text)
    }

    fn names_a_file(&self) -> bool {
        let (cut, _) = self.name_from();
        let name = self.span.name();
        let at_or_after = |mark: Option<usize>| mark.is_some_and(|mark| mark >= cut);
        (at_or_after(name.last_separator) && name.has_extension)
            || (name.is_source && at_or_after(name.last_dot))
    }

    fn ends_with_quote(&self) -> bool {
        self.span.text.ends_with(QUOTES)
    }

    fn ends_with_colon(&self) -> bool {
        self.span.text.trim_end().ends_with(':')
    }

    fn opens_interpolation(&self) -> bool {
        opens_interpolation(self.span.text)
    }
}

/// The word right before the value in a key's `text`: what is left of its
/// end once the quotes, the operator and a call's bracket are off. In a line
/// that holds several entries it is the entry's own name, whatever name the
/// line's first colon follows.
fn adjacent_word(text: &str) -> &str {
    let head = text.trim_end_matches(|c: char| {
        c.is_whitespace()
            || QUOTES.contains(&c)
            || matches!(c, ':' | '=' | '>' | '|' | '?' | '<' | '(')
    });
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-'))
        .last()
        .map_or(head.len(), |(at, _)| at);
    head.get(start..).unwrap_or_default()
}

/// The facts of a key as values: keys with equal bits are judged alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeyBits {
    chosen: bool,
    locator: bool,
    describes: bool,
    published: bool,
    credential: bool,
    file: bool,
    quote: bool,
    colon: bool,
    interpolates: bool,
}

impl KeyFacts for KeyBits {
    fn names_a_chosen_credential(&self) -> bool {
        self.chosen
    }

    fn names_a_locator(&self) -> bool {
        self.locator
    }

    fn describes_a_credential(&self) -> bool {
        self.describes
    }

    fn names_a_published_value(&self) -> bool {
        self.published
    }

    fn names_a_credential(&self) -> bool {
        self.credential
    }

    fn names_a_file(&self) -> bool {
        self.file
    }

    fn ends_with_quote(&self) -> bool {
        self.quote
    }

    fn ends_with_colon(&self) -> bool {
        self.colon
    }

    fn opens_interpolation(&self) -> bool {
        self.interpolates
    }
}
