//! Which strings of a request (a search term, a keyword, an address, a
//! question) hold a credential.
//!
//! A request names what it looks for, so the name beside a value says nothing
//! of the value: `password: str` and `token = None` are declarations to find,
//! `password: <a generated string>` is a secret pasted into the request. A
//! request is also no file: a name and its value sit anywhere in a sentence
//! (`locate password: …`), and a search term may spell its value as a regex
//! does (`eyJ…\.eyJ…`). Nothing is redacted here: the caller refuses the
//! request.

use super::code_syntax::{self, Key};
use super::*;

/// Whether `text`, a string of a request, holds a credential: as written,
/// in the literal a regex spelling of it matches ([`literal_form`]), or in a
/// text its regex wildcards match where the value found is mostly written
/// out ([`spells_most_of_a_credential`]).
pub(crate) fn holds_credential_as_request(text: &str) -> bool {
    holds_credential_as_written(text)
        || literal_form(text).is_some_and(|plain| holds_credential_as_written(&plain))
        || spells_most_of_a_credential(text)
}

/// Whether `text` holds a credential: a value a pattern knows by its own
/// shape (a provider's token, a key block), a value a pattern finds by where
/// it stands (its `secret` group: a URL's password) that is written as a
/// secret is ([`ValueTest::Written`]) or opaque, under a pattern that reads a
/// credential's name a value that is opaque ([`named_values::is_opaque`]),
/// or an opaque value after a credential's name anywhere in the text
/// ([`names_an_opaque_value`]).
fn holds_credential_as_written(text: &str) -> bool {
    let scope = Scope::Pathless;
    let by_form = matching_patterns(text, scope).iter().any(|idx| {
        document::Form::of(idx)
            .is_some_and(|form| form.redact(text, replacement_for(idx)).0 != text)
    });
    by_form
        || !pattern_values(text).is_empty()
        || split_literals::redact_pieces(text).is_some()
        || names_an_opaque_value(text)
}

/// The credentials a pattern finds in `text` by its regex, by byte position.
fn pattern_values(text: &str) -> Vec<std::ops::Range<usize>> {
    placed_values(text, true)
}

/// The values a pattern's regex places in `text` that are no credential as a
/// request writes them (a URL's `password`, a name's `str`), by byte
/// position: what the request rules read and cleared, which a rule for a
/// file's text may still hide.
pub(crate) fn cleared_values(text: &str) -> Vec<std::ops::Range<usize>> {
    placed_values(text, false)
}

/// The values a pattern's regex places in `text` that are a credential
/// (`held`) or are none.
fn placed_values(text: &str, held: bool) -> Vec<std::ops::Range<usize>> {
    let scope = Scope::Pathless;
    let mut values = Vec::new();
    for idx in matching_patterns(text, scope).iter() {
        if document::Form::of(idx).is_some() {
            continue;
        }
        let regex = pattern_regex_for(idx, text);
        // A pattern with no `secret` group spells the credential itself; one
        // with a group spells where a credential stands, and its own test
        // judged what stands there as a file's read judges it: a placeholder
        // (`postgres://user:password@localhost`, `Bearer <token>`) is none,
        // any other value is one ([`ValueTest::Placed`]).
        let own_judgement = matches!(VALUE_TESTS[idx], ValueTest::Every | ValueTest::Placed);
        Judge::of(idx, scope, text, None).secrets(regex, text, |_, secret| {
            let Some(value) = text.get(secret.clone()) else {
                return;
            };
            // A value a rule finds under a name is one its own test judged
            // a credential as a file holds it, at any length: a request
            // clears only a value that names a type or a field to find
            // (`password: str`), which no opaque word of any length is.
            let is_credential = own_judgement || is_opaque_text(value);
            if is_credential == held {
                values.push(secret);
            }
        });
    }
    values
}

/// Whether a word of `value` is opaque ([`named_values::is_opaque_word`]),
/// its quotes aside.
fn is_opaque_text(value: &str) -> bool {
    value
        .split_whitespace()
        .any(|piece| named_values::is_opaque_word(piece.trim_matches(credential_values::QUOTES)))
}

/// Whether a name with a credential word, anywhere in `text`, is followed by
/// its operator and a value whose first word is a credential by that name
/// ([`named_values::named_word`]: opaque, and no value such a name
/// publishes or locates). The file rules that read `name: value` and
/// `NAME=value` start at a line's start and read to its end, and the rule
/// that reads a name in any layout takes one run of
/// [`crate::security::patterns::OPAQUE_CHARS`]: a request is a sentence,
/// where the name follows other words, words follow the value, and a
/// generated password holds any symbol (`&`).
fn names_an_opaque_value(text: &str) -> bool {
    !named_opaque_values(text).is_empty()
}

/// The values [`names_an_opaque_value`] finds in `text`, in text order.
fn named_opaque_values(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive(LINE_END_CHARS) {
        code_syntax::line_keys(line.trim_end_matches(LINE_END_CHARS), offset, &mut lines);
        offset += line.len();
    }
    let mut values: Vec<std::ops::Range<usize>> = Vec::new();
    for line in &lines {
        for &Key { start, value, .. } in &line.keys {
            // A key inside a value found before names nothing of its own.
            if values.last().is_some_and(|last| start < last.end) {
                continue;
            }
            let (Some(name), Some(rest)) =
                (text.get(start..value), text.get(value..line.range.end))
            else {
                continue;
            };
            let Some(word) = rest.split_whitespace().next() else {
                continue;
            };
            if let Some(own) = named_values::named_word(name, word) {
                values.push(value + own.start..value + own.end);
            }
        }
    }
    values.extend(stated_opaque_values(text));
    values.sort_by_key(|value| value.start);
    values
}

/// The values a sentence of `text` states for a credential's name with no
/// operator (`the password is …`, `my password was: …`): the word after the
/// name states a value ([`named_values::states_a_value`]), and the one after
/// that is a credential by that name ([`named_values::named_word`]), which no
/// word of prose is (`the password is required`, `the token was expired`).
fn stated_opaque_values(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut values = Vec::new();
    let (mut name, mut states) = ("", "");
    let mut start = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        let at = start;
        start += piece.len();
        if word.is_empty() {
            continue;
        }
        if named_values::states_a_value(states)
            && code_syntax::names_a_credential_word(name)
            && name.chars().all(code_syntax::is_key_char)
            && let Some(own) = named_values::named_word(name, word)
        {
            values.push(at + own.start..at + own.end);
        }
        (name, states) = (states, word);
    }
    values
}

/// `text` with each value [`names_an_opaque_value`] finds replaced by the
/// placeholder of the rule that reads a credential's name; `None` when it
/// finds none. A request string that is evidence keeps the text around its
/// secret.
pub(crate) fn hide_named_values(text: &str) -> Option<String> {
    let values = named_opaque_values(text);
    if values.is_empty() {
        return None;
    }
    let placeholder = crate::security::redaction_placeholder(named_values::PATTERN);
    let mut hidden = String::with_capacity(text.len());
    let mut from = 0;
    for value in values {
        hidden.push_str(text.get(from..value.start)?);
        hidden.push_str(&placeholder);
        from = value.end;
    }
    hidden.push_str(text.get(from..)?);
    Some(hidden)
}

/// The literal a regex spelling of `text` matches, where each element that
/// matches exactly one fixed character is that character: an escaped mark
/// (`\.`, `\-`, `\/`), an escaped line break or tab (`\n`, `\r`, `\t`), a
/// class of one member (`[I]`, `[\.]`), and an empty group (`(?:)`), which
/// matches nothing between two characters. Every other element stays as
/// written. `None` when `text` spells none of them: it is its own literal.
pub(crate) fn literal_form(text: &str) -> Option<String> {
    if !text.contains(['\\', '[', '(']) {
        return None;
    }
    let mut plain = String::with_capacity(text.len());
    let mut changed = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let (read, element) = one_character_element(rest).unwrap_or((c.len_utf8(), Some(c)));
        changed |= element != Some(c) || read != c.len_utf8();
        plain.extend(element);
        rest = rest.get(read..).unwrap_or_default();
    }
    changed.then_some(plain)
}

/// The regex element `text` starts with when it matches one fixed character
/// or nothing: the bytes it takes and that character.
fn one_character_element(text: &str) -> Option<(usize, Option<char>)> {
    if text.starts_with("(?:)") {
        return Some((4, None));
    }
    if let Some(class) = text.strip_prefix('[') {
        let (member_len, member) = escaped_character(class).or_else(|| {
            let member = class.chars().next().filter(|c| !matches!(c, ']' | '^'))?;
            Some((member.len_utf8(), member))
        })?;
        return class
            .get(member_len..)?
            .starts_with(']')
            .then_some((member_len + 2, Some(member)));
    }
    escaped_character(text).map(|(len, c)| (len, Some(c)))
}

/// The escape `text` starts with when it names one fixed character: its
/// length and that character. An escaped letter or digit is a class, an
/// anchor or a reference (`\w`, `\b`, `\1`), line breaks and the tab aside.
fn escaped_character(text: &str) -> Option<(usize, char)> {
    let escaped = text.strip_prefix('\\')?.chars().next()?;
    let character = match escaped {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        mark if mark.is_ascii_punctuation() => mark,
        _ => return None,
    };
    Some((1 + escaped.len_utf8(), character))
}

/// Whether `text`, read as a regex, spells most of a credential: in a text
/// its wildcards match ([`FilledForms`]), a credential is found
/// ([`pattern_values`], [`named_opaque_values`]) of whose characters at least
/// [`WRITTEN_SHARE`] were written and the rest a wildcard stands for. A
/// token whose last characters are spelled `\w\w`, `..` or `[A-Za-z0-9]{2}`
/// is that token to whoever reads the request; a pattern that writes a
/// provider's prefix and leaves the value to a class (`ghp_[A-Za-z0-9]{36}`,
/// `sk-[A-Za-z0-9]{20,}`) writes none of any token.
fn spells_most_of_a_credential(text: &str) -> bool {
    let Some(forms) = FilledForms::of(text) else {
        return false;
    };
    forms.into_iter().any(|(filled, stands_in)| {
        // The characters written, without those a wildcard stands for: a
        // value that writes only a placeholder (`pass` in `user:pass.@`)
        // spells no credential, whatever the wildcard adds.
        let written_text = |value: &std::ops::Range<usize>| {
            filled
                .char_indices()
                .filter(|(at, _)| value.contains(at) && stands_in.get(*at) == Some(&false))
                .map(|(_, c)| c)
                .collect::<String>()
        };
        let mostly_written = |value: &std::ops::Range<usize>| {
            let stood_in = stands_in
                .get(value.clone())
                .map_or(0, |bytes| bytes.iter().filter(|stands| **stands).count());
            let written = value.len() - stood_in;
            stood_in > 0
                && written * WRITTEN_SHARE.1 >= value.len() * WRITTEN_SHARE.0
                && !credential_values::is_written_placeholder(&written_text(value))
        };
        pattern_values(&filled).iter().any(mostly_written)
            || named_opaque_values(&filled).iter().any(mostly_written)
    })
}

/// The share of a credential's characters (three of four) a request writes
/// out for the rest to count as that credential spelled with wildcards.
const WRITTEN_SHARE: (usize, usize) = (3, 4);

/// One element of a regex spelling: the character it matches (a written one,
/// or one a wildcard stands for) and how many times.
struct Element {
    character: char,
    /// A wildcard: a class, `.`, or a class escape (`\w`).
    wild: bool,
    least: usize,
    /// `None`: any number of times (`+`, `*`, `{2,}`).
    most: Option<usize>,
}

/// The texts a regex spelling matches when each wildcard is one character it
/// matches and each open count (`+`, `*`, `{2,}`, `{2,5}`) is every length a
/// mostly written credential could need.
struct FilledForms {
    elements: Vec<Element>,
    /// The longest an open count is tried past its least.
    open: usize,
}

impl FilledForms {
    /// `None` when `text` spells no wildcard and no count: it is its own
    /// only form.
    fn of(text: &str) -> Option<Self> {
        if !text.contains(['\\', '[', '.', '{', '+', '*', '?']) {
            return None;
        }
        let mut elements: Vec<Element> = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let (read, element) = regex_element(rest, elements.last_mut());
            elements.extend(element);
            rest = rest.get(read..).unwrap_or_default();
        }
        if !elements
            .iter()
            .any(|element| element.wild || element.least != 1 || element.most != Some(1))
        {
            return None;
        }
        // A wildcard run longer than a third of what is written beside it
        // leaves less than the written share, so no longer run is tried.
        let written = |element: &Element| !element.wild && !element.character.is_whitespace();
        let mut open = 0;
        for (at, element) in elements.iter().enumerate() {
            if element.most == Some(element.least) {
                continue;
            }
            let before = elements[..at].iter().rev().take_while(|e| written(e)).count();
            let after = elements[at + 1..].iter().take_while(|e| written(e)).count();
            open = open.max((before + after) * (WRITTEN_SHARE.1 - WRITTEN_SHARE.0) / WRITTEN_SHARE.0);
        }
        Some(Self { elements, open })
    }

    /// Each form with, per byte, whether a wildcard or a count stands for it.
    fn into_iter(self) -> impl Iterator<Item = (String, Vec<bool>)> {
        let written = self.elements.iter().filter(|element| !element.wild).count();
        // No value holds more stood-in characters than a third of all that
        // is written, so a longer count (`[a-f0-9]{64}`) is cut there.
        let longest = written * (WRITTEN_SHARE.1 - WRITTEN_SHARE.0) / WRITTEN_SHARE.0 + 1;
        (0..=self.open).map(move |extra| {
            let mut filled = String::new();
            let mut stands_in = Vec::new();
            for element in &self.elements {
                let times = match element.most {
                    Some(most) if most == element.least => element.least,
                    Some(most) => (element.least + extra).min(most),
                    None => element.least + extra,
                }
                .min(longest);
                for time in 0..times {
                    filled.push(element.character);
                    let stood = element.wild || time > 0;
                    stands_in.extend(std::iter::repeat_n(stood, element.character.len_utf8()));
                }
            }
            (filled, stands_in)
        })
    }
}

/// The regex element `text` starts with: the bytes it takes and the element,
/// `None` for one that matches no character (an anchor, an empty group). A
/// count (`{2}`, `+`, `*`, `?`) is no element of its own: it sets how many
/// times `last`, the element before it, matches.
fn regex_element(text: &str, last: Option<&mut Element>) -> (usize, Option<Element>) {
    let once = |character, wild| Element {
        character,
        wild,
        least: 1,
        most: Some(1),
    };
    let Some(first) = text.chars().next() else {
        return (0, None);
    };
    if let Some(last) = last
        && let Some((read, least, most)) = count(text)
    {
        (last.least, last.most) = (least, most);
        return (read, None);
    }
    if text.starts_with("(?:)") {
        return (4, None);
    }
    if let Some((read, member)) = class(text) {
        return (read, Some(member.map_or(once('a', true), |c| once(c, false))));
    }
    if let Some(escaped) = text.strip_prefix('\\').and_then(|rest| rest.chars().next()) {
        let read = 1 + escaped.len_utf8();
        return match escaped {
            'w' | 'S' | 'D' | 'h' | 'x' | 'p' => (read, Some(once('a', true))),
            'd' => (read, Some(once('0', true))),
            's' => (read, Some(once(' ', true))),
            'W' => (read, Some(once('-', true))),
            'b' | 'B' | 'A' | 'z' | 'Z' => (read, None),
            _ => match escaped_character(text) {
                Some((read, character)) => (read, Some(once(character, false))),
                None => (1, Some(once('\\', false))),
            },
        };
    }
    if first == '.' {
        return (1, Some(once('a', true)));
    }
    (first.len_utf8(), Some(once(first, false)))
}

/// The count `text` starts with: its bytes, the least and the most times
/// (`None`: unbounded), a lazy or possessive mark after it included.
fn count(text: &str) -> Option<(usize, usize, Option<usize>)> {
    let (read, least, most) = match text.chars().next()? {
        '+' => (1, 1, None),
        '*' => (1, 0, None),
        '?' => (1, 0, Some(1)),
        '{' => {
            let end = text.find('}')?;
            let body = text.get(1..end)?;
            let (least, most) = match body.split_once(',') {
                None => {
                    let times = body.parse().ok()?;
                    (times, Some(times))
                }
                Some((least, "")) => (least.parse().ok()?, None),
                Some((least, most)) => (least.parse().ok()?, Some(most.parse().ok()?)),
            };
            (end + 1, least, most)
        }
        _ => return None,
    };
    let marked = usize::from(text.get(read..).is_some_and(|rest| rest.starts_with(['?', '+'])));
    Some((read + marked, least, most))
}

/// The bracket class `text` starts with: its bytes and its only member, or
/// `None` for a class of several (a wildcard).
fn class(text: &str) -> Option<(usize, Option<char>)> {
    if let Some((read, member)) = text
        .starts_with('[')
        .then(|| one_character_element(text))
        .flatten()
    {
        return Some((read, member));
    }
    let body = text.strip_prefix('[')?;
    let mut escaped = false;
    for (at, c) in body.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            // A `]` that opens the class (`[]a]`, `[^]a]`) is a member.
            ']' if at > 0 && body.get(..at) != Some("^") => return Some((at + 2, None)),
            _ => {}
        }
    }
    None
}
