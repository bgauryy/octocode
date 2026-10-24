// The production text of a Rust source file, shared by every source sweep
// (guard tests that grep production code). Included, not declared as a
// module: `tools/test_support.rs` includes it for the library's unit tests
// and `tests/lock_poison_policy.rs` for the integration binary, which cannot
// reach `#[cfg(test)]` library items.

/// `source` with every item, statement or block gated by `#[cfg(test)]` (or
/// `#[cfg(all(test, …))]`) blanked: an inline test module, a test-only
/// `fn`, `impl`, `use`, `thread_local!` or `{ … }` block. Everything else
/// stays, including production code after a test item, comments and string
/// literals; blanked text keeps its newlines, so a byte's line number is the
/// same as in the file. A file whose inner attribute is `#![cfg(test)]` is
/// all test code.
pub(crate) fn production_text(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"#![cfg(test)]") {
            blank(&mut out, 0..bytes.len());
            break;
        }
        if bytes[at] == b'#'
            && let Some(attribute) = attribute_end(bytes, at)
            && test_gated(&source[at..attribute])
        {
            let end = item_end(bytes, attribute);
            blank(&mut out, at..end);
            at = end;
            continue;
        }
        at = skip_token(bytes, at);
    }
    String::from_utf8(out).unwrap_or_default()
}

/// `text` with every comment and every string, raw string and character
/// literal blanked (newlines kept): the code alone, for sweeps that match
/// code shapes such as imports or lock calls, never prose or fixtures.
pub(crate) fn code_only(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        let end = skip_token(bytes, at);
        // Only a comment or a literal spans more than one byte.
        if end > at + 1 {
            blank(&mut out, at..end);
        }
        at = end.max(at + 1);
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Spaces for every byte of `range` but a newline. A blanked range starts and
/// ends on ASCII, so it holds whole characters and the text stays UTF-8.
fn blank(out: &mut [u8], range: std::ops::Range<usize>) {
    for byte in &mut out[range] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

/// Whether the outer attribute text `#[…]` gates on `test`.
fn test_gated(attribute: &str) -> bool {
    let compact: String = attribute.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "#[cfg(test)]" || compact.starts_with("#[cfg(all(test,")
}

/// The end of the outer attribute `#[…]` starting at `at`.
fn attribute_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at + 1) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    let mut index = at + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            b'"' | b'\'' | b'/' => {
                index = skip_token(bytes, index);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// The end of the gated item that follows the attribute ending at `from`,
/// past any further attributes. A declaration (`fn`, `impl`, `mod`, a type)
/// ends at its closing `}` or its `;`; a `use`, `let`, `type`, `static` or
/// `const` item at its `;`, even after a brace group; anything else (a
/// block, statement, brace macro, field, element or match arm) at its `}`,
/// `;` or `,`. An unbalanced closer ends it before.
fn item_end(bytes: &[u8], from: usize) -> usize {
    let mut at = skip_trivia(bytes, from);
    while bytes.get(at) == Some(&b'#')
        && let Some(end) = attribute_end(bytes, at)
    {
        at = skip_trivia(bytes, end);
    }
    let kind = item_kind(&bytes[at..]);
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return at;
                }
                depth -= 1;
                if depth == 0 && bytes[at] == b'}' && kind != Item::Statement {
                    return at + 1;
                }
            }
            b';' if depth == 0 => return at + 1,
            b',' if depth == 0 && kind == Item::Other => return at + 1,
            _ => {}
        }
        at = skip_token(bytes, at);
    }
    bytes.len()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    /// Ends at its `;` only.
    Statement,
    /// Ends at its closing `}` or `;`; a `,` is inside its generics.
    Declaration,
    /// Ends at its `}`, `;` or `,`.
    Other,
}

/// The kind of the item at the start of `text`, by its first keyword.
fn item_kind(text: &[u8]) -> Item {
    let words = String::from_utf8_lossy(&text[..text.len().min(80)])
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty() && !matches!(*word, "pub" | "crate" | "super"))
        .take(2)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let second = words.get(1).map(String::as_str);
    match words.first().map(String::as_str) {
        Some("const") if matches!(second, Some("fn" | "unsafe" | "async")) => Item::Declaration,
        Some("use" | "let" | "type" | "static" | "extern" | "const") => Item::Statement,
        Some(
            "fn" | "impl" | "mod" | "struct" | "enum" | "trait" | "union" | "async" | "unsafe"
            | "macro_rules",
        ) => Item::Declaration,
        _ => Item::Other,
    }
}

/// Past whitespace and comments from `at`.
fn skip_trivia(bytes: &[u8], mut at: usize) -> usize {
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes[at..].starts_with(b"//") || bytes[at..].starts_with(b"/*") {
            at = skip_token(bytes, at);
        } else {
            return at;
        }
    }
}

/// Past one token at `at`: a whole comment, string, raw string or character
/// literal, else one byte (a lifetime's quote is one byte).
fn skip_token(bytes: &[u8], at: usize) -> usize {
    let rest = &bytes[at..];
    if rest.starts_with(b"//") {
        return rest
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |line| at + line);
    }
    if rest.starts_with(b"/*") {
        let (mut depth, mut index) = (0usize, at);
        while index < bytes.len() {
            if bytes[index..].starts_with(b"/*") {
                depth += 1;
                index += 2;
            } else if bytes[index..].starts_with(b"*/") {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return index;
                }
            } else {
                index += 1;
            }
        }
        return bytes.len();
    }
    let ident_before = |index: usize| {
        index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_')
    };
    if rest[0] == b'r'
        && (!ident_before(at) || (at > 0 && bytes[at - 1] == b'b' && !ident_before(at - 1)))
    {
        let hashes = rest[1..].iter().take_while(|byte| **byte == b'#').count();
        if rest.get(1 + hashes) == Some(&b'"') {
            let mut closing = vec![b'"'];
            closing.extend(std::iter::repeat_n(b'#', hashes));
            let body = at + 2 + hashes;
            return bytes[body..]
                .windows(closing.len())
                .position(|window| window == closing.as_slice())
                .map_or(bytes.len(), |end| body + end + closing.len());
        }
    }
    match rest[0] {
        b'"' => {
            let mut index = at + 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 2,
                    b'"' => return index + 1,
                    _ => index += 1,
                }
            }
            bytes.len()
        }
        b'\'' => {
            // An escape (`'\''`, `'\n'`, `'\u{..}'`): its escaped char is
            // never the closing quote.
            if rest.get(1) == Some(&b'\\') {
                return rest
                    .get(3..)
                    .and_then(|tail| tail.iter().position(|byte| *byte == b'\''))
                    .map_or(bytes.len(), |end| at + 4 + end);
            }
            let width = rest.get(1).map_or(1, |lead| match lead {
                0xF0.. => 4,
                0xE0.. => 3,
                0xC0.. => 2,
                _ => 1,
            });
            if rest.get(1 + width) == Some(&b'\'') {
                at + 2 + width
            } else {
                at + 1
            }
        }
        _ => at + 1,
    }
}
