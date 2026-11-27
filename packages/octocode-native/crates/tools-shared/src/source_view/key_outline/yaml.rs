//! YAML mapping keys by indentation; block and multi-line quoted scalars
//! are text, never keys.
use super::Entry;

/// A YAML scalar whose text continues on the lines after its first.
enum Continued {
    /// A block scalar (`|`, `>`): every following line indented past the
    /// node holding it, or blank.
    Block { parent_indent: usize },
    /// A quoted scalar: through the line that closes the quote.
    Quoted(char),
}

/// YAML mapping keys: each key line, its depth by indentation, and the span
/// through its last more-indented line. Lines inside a block scalar or a
/// multi-line quoted scalar are text, never keys.
pub(super) fn yaml_entries(content: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    // Open keys: indentation and entry index.
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut last_content = 0;
    let mut continued: Option<Continued> = None;
    for (index, raw) in content.lines().enumerate() {
        let line = index + 1;
        let body = raw.trim_start();
        let raw_indent = raw.len() - body.len();
        match &continued {
            Some(Continued::Block { parent_indent }) => {
                if body.is_empty() || raw_indent > *parent_indent {
                    last_content = line;
                    continue;
                }
                continued = None;
            }
            Some(Continued::Quoted(quote)) => {
                last_content = line;
                if quote_closes(raw, *quote) {
                    continued = None;
                }
                continue;
            }
            None => {}
        }
        if body.is_empty() || body.starts_with('#') || body.starts_with("---") || body == "..." {
            continue;
        }
        // A list item's key sits past its `- ` markers.
        let mut indent = raw_indent;
        let mut item = body;
        let mut dash_indent = None;
        while let Some(rest) = item.strip_prefix("- ") {
            dash_indent = Some(indent);
            indent += 2 + (rest.len() - rest.trim_start().len());
            item = rest.trim_start();
        }
        while open
            .last()
            .is_some_and(|(depth_indent, _)| *depth_indent >= indent)
        {
            if let Some((_, entry)) = open.pop() {
                entries[entry].end = last_content.max(entries[entry].start);
            }
        }
        last_content = line;
        let (value, parent_indent, key) = match yaml_key(item) {
            Some(colon) => {
                open.push((indent, entries.len()));
                entries.push(Entry::at(line, open.len()));
                (
                    Some(item[colon + 1..].trim_start()),
                    indent,
                    Some(entries.len() - 1),
                )
            }
            // A bare list item's own scalar belongs to its dash.
            None => (
                dash_indent.map(|_| item),
                dash_indent.unwrap_or(indent),
                None,
            ),
        };
        if let Some(value) = value {
            continued = continues(value, parent_indent);
            // A key whose value is a multi-line scalar holds text, not keys.
            if let Some(entry) = key.filter(|_| continued.is_some()) {
                entries[entry].scalar = true;
            }
        }
    }
    for (_, entry) in open {
        entries[entry].end = last_content.max(entries[entry].start);
    }
    entries
}

/// The continuation a value opens on its first line: a block scalar
/// header (`|`, `>`, with indentation and chomping indicators and an
/// optional comment), or a quote left open.
fn continues(value: &str, parent_indent: usize) -> Option<Continued> {
    let head = value.split(" #").next().unwrap_or(value).trim_end();
    if let Some(indicators) = head.strip_prefix(['|', '>'])
        && indicators
            .chars()
            .all(|c| c.is_ascii_digit() || c == '+' || c == '-')
    {
        return Some(Continued::Block { parent_indent });
    }
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    (!quote_closes(&value[1..], quote)).then_some(Continued::Quoted(quote))
}

/// Whether `text` holds the quote that closes a YAML `quote` scalar: an
/// unescaped `"`, or a `'` not doubled.
fn quote_closes(text: &str, quote: char) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quote == '"' => {
                chars.next();
            }
            '\'' if quote == '\'' && chars.peek() == Some(&'\'') => {
                chars.next();
            }
            c if c == quote => return true,
            _ => {}
        }
    }
    false
}

/// The colon of `item` when it opens a mapping key: `key:` followed by a
/// space or the line end, before any comment; a quoted key may hold `: `.
fn yaml_key(item: &str) -> Option<usize> {
    if item.starts_with(['{', '[', '|', '>', '&', '*', '!', '%', '@', '`']) {
        return None;
    }
    let body = item.split(" #").next().unwrap_or(item).trim_end();
    let colon = match body.chars().next() {
        Some(quote @ ('"' | '\'')) => body[1..].find(quote).map(|end| end + 2),
        _ => body
            .find(": ")
            .or_else(|| body.ends_with(':').then(|| body.len() - 1)),
    };
    colon.filter(|at| {
        *at > 0
            && body[*at..].starts_with(':')
            && body[*at + 1..]
                .chars()
                .next()
                .is_none_or(|next| next == ' ')
    })
}
