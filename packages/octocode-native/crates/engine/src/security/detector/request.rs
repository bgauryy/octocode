//! Which strings of a request (a search term, a keyword, an address, a
//! question) hold a credential.
//!
//! A request is judged by the one judgement every text gets ([`secret_spans`]),
//! so a request refuses whatever a read would hide. A request may also spell
//! its value as a regex does (`eyJ…\.eyJ…`), which a file never does: its
//! literal form is judged the same way. Nothing is redacted here: the caller
//! refuses the request.

use super::secret_spans;

/// Whether `text`, a string of a request, holds a credential: as written, or
/// in the literal a regex spelling of it matches ([`literal_form`]).
pub(crate) fn holds_credential_as_request(text: &str) -> bool {
    !secret_spans(text).is_empty()
        || literal_form(text).is_some_and(|plain| !secret_spans(&plain).is_empty())
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
