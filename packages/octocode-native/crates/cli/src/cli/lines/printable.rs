//! Control characters in text output. A value prints as data and nothing
//! else: a terminal control code (ESC, BEL, …) never reaches the terminal,
//! and a line break inside a value never starts a line of its own, so a file
//! name or file content cannot print a forged `# status:` or `# next:` line.
//!
//! Each control character prints as one visible character, so a line keeps
//! its length in characters and columns and offsets still point where the
//! JSON answer's do: C0 controls and DEL as their Unicode Control Pictures
//! (`␛` for ESC, `␇` for BEL, `␊` for a line feed, U+2400–U+241F and U+2421).
//! C1 controls (U+0080–U+009F) have no picture and print as `\u{9b}`. A tab
//! stays a tab. The JSON answer (`--json`) keeps every character exact.
use serde_json::{Map, Value};
use std::borrow::Cow;

/// The one visible character a C0 control or DEL prints as.
fn picture(control: char) -> Option<char> {
    match control {
        '\0'..='\x1f' => char::from_u32(0x2400 + u32::from(control)),
        '\x7f' => Some('\u{2421}'),
        _ => None,
    }
}

/// `text` with its control characters visible. `layout` keeps what lays a
/// multi-line value out as lines: each line feed, and a carriage return that
/// ends a line (CRLF text prints as rg prints it).
fn escaped(text: &str, layout: bool) -> Cow<'_, str> {
    let kept = |control: char, next: Option<char>| {
        control == '\t'
            || (layout
                && (control == '\n' || (control == '\r' && matches!(next, None | Some('\n')))))
    };
    let mut chars = text.chars().peekable();
    let mut out: Option<String> = None;
    let mut at = 0;
    while let Some(c) = chars.next() {
        if c.is_control() && !kept(c, chars.peek().copied()) {
            let out = out.get_or_insert_with(|| {
                let mut out = String::with_capacity(text.len() + 2);
                // `at` advances only by complete characters from this text.
                out.push_str(text.get(..at).unwrap_or_default());
                out
            });
            match picture(c) {
                Some(picture) => out.push(picture),
                None => out.extend(c.escape_unicode()),
            }
        } else if let Some(out) = out.as_mut() {
            out.push(c);
        }
        at += c.len_utf8();
    }
    out.map_or(Cow::Borrowed(text), Cow::Owned)
}

/// `text` for a place that prints on one line (a path, a name, a note): every
/// control character visible, a line break too.
pub(super) fn one_line(text: &str) -> Cow<'_, str> {
    escaped(text, false)
}

/// `text` with every control character visible except the line breaks that
/// lay it out as lines.
pub(super) fn lines(text: &str) -> Cow<'_, str> {
    escaped(text, true)
}

/// `value` with every control character of its strings visible, except the
/// line breaks that lay a multi-line string out as lines (each then prints
/// indented under its key or behind its line number), and every key on one
/// line. The renderer reads this, never the raw answer.
pub(super) fn printable(value: Value) -> Value {
    match value {
        Value::String(text) => match lines(&text) {
            Cow::Borrowed(_) => Value::String(text),
            Cow::Owned(text) => Value::String(text),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(printable).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (one_line(&key).into_owned(), printable(value)))
                .collect::<Map<_, _>>(),
        ),
        other => other,
    }
}
