//! Which strings of a request (a search term, a keyword, an address, a
//! question) hold a credential.
//!
//! A request is judged by the one judgement a text of no known file gets
//! ([`judged_values`]: [`redact`] in the pathless net), so a request runs
//! only what every read would show and refuses whatever a read would hide:
//! `password: str` and `token = None` are declarations to find in both,
//! `password = "hunter2-Sup3rS3cret"` is a secret in both. A request may
//! also spell its value as a regex does (`eyJ…\.eyJ…`), which a file never
//! does: its literal form and the texts its wildcards match are judged the
//! same way. Nothing is redacted here: the caller refuses the request.

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

/// Whether the file judge hides any of `text` ([`judged_values`]).
fn holds_credential_as_written(text: &str) -> bool {
    !judged_values(text).is_empty()
}

/// What the file judge hides of `text`, a text of no known file, by byte
/// position: the one judgement a read of such text makes ([`redact`] in
/// [`Scope::Pathless`]), every rule and every value test included.
fn judged_values(text: &str) -> Vec<std::ops::Range<usize>> {
    redact(text, Scope::Pathless)
        .map(|result| result.edits.redacted().collect())
        .unwrap_or_default()
}

/// The literal a regex spelling of `text` matches, where each element that
/// matches exactly one fixed character is that character: an escaped mark
/// (`\.`, `\-`, `\/`), an escaped line break or tab (`\n`, `\r`, `\t`), a
/// class of one member (`[I]`, `[\.]`), and an empty group (`(?:)`), which
/// matches nothing between two characters. Every other element stays as
/// written, an escaped quote too ([`escaped_character`]). `None` when `text`
/// is no regex spelling: it writes no regex operator (an escaped mark, a
/// class of one, an empty group), so it is the text a reader wrote, judged
/// as written alone. A line of code writes `\"` and `\n` as a literal's
/// escapes, which a regex shares: they alone make no regex spelling.
pub(crate) fn literal_form(text: &str) -> Option<String> {
    if !text.contains(['\\', '[', '(']) {
        return None;
    }
    let mut plain = String::with_capacity(text.len());
    let mut regex = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let (read, element) = one_character_element(rest).unwrap_or((c.len_utf8(), Some(c)));
        regex |= (element != Some(c) || read != c.len_utf8())
            && !matches!(element, Some('\n' | '\r' | '\t'));
        plain.extend(element);
        rest = rest.get(read..).unwrap_or_default();
    }
    regex.then_some(plain)
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
/// An escaped quote is none: no regex needs a quote escaped, so `\"` is a
/// string literal's escape, written as code writes it.
fn escaped_character(text: &str) -> Option<(usize, char)> {
    let escaped = text.strip_prefix('\\')?.chars().next()?;
    let character = match escaped {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        '"' | '\'' | '`' => return None,
        mark if mark.is_ascii_punctuation() => mark,
        _ => return None,
    };
    Some((1 + escaped.len_utf8(), character))
}

/// Whether `text`, read as a regex, spells most of a credential: in a text
/// its wildcards match ([`FilledForms`]), the file judge finds a credential
/// by its shape (a rule of its shape or place, or an opaque value) of whose
/// characters at least
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
        // A wildcard fills characters no one wrote: only a value whose own
        // shape is a credential's is spelled by it (a rule that knows a
        // credential by its shape or its place found it, or it is opaque),
        // never a word a value test reads under a name (`Optional[str]`).
        let Some(result) = redact(&filled, Scope::Pathless) else {
            return false;
        };
        let by_shape = result.secrets_detected.iter().any(|name| {
            PATTERNS
                .iter()
                .position(|pattern| pattern.name == name)
                .is_some_and(|idx| matches!(VALUE_TESTS[idx], ValueTest::Every | ValueTest::Placed))
        });
        result
            .edits
            .redacted()
            .filter(|value| {
                by_shape
                    || filled.get(value.clone()).is_some_and(|text| {
                        named_values::is_opaque_word(text.trim_matches(credential_values::QUOTES))
                    })
            })
            .any(|value| mostly_written(&value))
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
            let before = elements[..at]
                .iter()
                .rev()
                .take_while(|e| written(e))
                .count();
            let after = elements[at + 1..].iter().take_while(|e| written(e)).count();
            open =
                open.max((before + after) * (WRITTEN_SHARE.1 - WRITTEN_SHARE.0) / WRITTEN_SHARE.0);
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
        return (
            read,
            Some(member.map_or(once('a', true), |c| once(c, false))),
        );
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
    let marked = usize::from(
        text.get(read..)
            .is_some_and(|rest| rest.starts_with(['?', '+'])),
    );
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
