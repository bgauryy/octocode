//! Key paths of matched lines in a JSON document (`matchKeys`).
//!
//! A matchString window over a JSON file shows the hit, not the section it
//! sits in: `"vite": "^7"` reads the same under `dependencies` and
//! `devDependencies`. This scanner walks the whole source once, line by
//! line, and names each requested line's key path. It never parses: comments
//! (JSONC), trailing commas, unquoted keys and broken nesting all degrade to
//! the nearest container it can still see, never to an error.

/// One open container and its current member.
enum Frame {
    /// `key`: byte span of the current member's key; `expect_key`: the next
    /// string names a member.
    Object {
        key: Option<(usize, usize)>,
        expect_key: bool,
    },
    /// `index`: the current element; `started`: it has begun.
    Array { index: usize, started: bool },
}

/// The key path of each of `lines` (1-based, ascending) in `text`: the
/// member or element that starts first on the line, else the container the
/// line sits in. Lines at the document root are left out.
pub(super) fn line_key_paths(text: &str, lines: &[usize]) -> Vec<(usize, String)> {
    let mut first = super::FirstKey::default();
    visit_lines(text, lines, &mut first);
    first.into_paths()
}

/// Walks `text` once and tells `visitor`, for each of `lines` (1-based,
/// ascending), the container the line opens in and each member or element
/// that starts on it, in order, while the visitor wants more of them.
pub(super) fn visit_lines(text: &str, lines: &[usize], visitor: &mut dyn super::LineVisitor) {
    let mut wanted = lines.iter().copied().peekable();
    let Some(&last) = lines.last() else {
        return;
    };
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut line = 1;
    let mut at = 0;
    // Whether `line` is requested; the visitor is told of it first.
    let mut begin_line = |line: usize, stack: &[Frame], visitor: &mut dyn super::LineVisitor| {
        while wanted.next_if(|&wanted| wanted < line).is_some() {}
        let requested = wanted.next_if_eq(&line).is_some();
        if requested {
            visitor.line(line, &path(text, stack, false));
        }
        requested
    };
    // The line being scanned is requested and the visitor still wants the
    // starts on it.
    let mut current = begin_line(line, &stack, visitor);
    while at < bytes.len() && line <= last {
        let byte = bytes[at];
        match byte {
            b'\n' => {
                line += 1;
                at += 1;
                current = begin_line(line, &stack, visitor);
                continue;
            }
            b' ' | b'\t' | b'\r' | b':' => {}
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = memchr_newline(bytes, at);
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = text[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| at + 2 + end + 2);
                for _ in text[at..end].matches('\n') {
                    line += 1;
                    current = begin_line(line, &stack, visitor);
                }
                at = end;
                continue;
            }
            b',' => match stack.last_mut() {
                Some(Frame::Object { expect_key, .. }) => *expect_key = true,
                Some(Frame::Array { index, started }) => {
                    *index += 1;
                    *started = false;
                }
                None => {}
            },
            b'}' | b']' => {
                stack.pop();
            }
            _ => {
                // A key, or the start of a value.
                let token_end = token_end(bytes, at);
                if let Some(Frame::Object { key, expect_key }) = stack.last_mut()
                    && *expect_key
                    && !matches!(byte, b'{' | b'[')
                {
                    *key = Some(key_span(bytes, at, token_end));
                    *expect_key = false;
                    current = current && visitor.start(&path(text, &stack, true));
                    at = token_end;
                    continue;
                }
                if let Some(Frame::Array { started, .. }) = stack.last_mut() {
                    *started = true;
                }
                current = current && visitor.start(&path(text, &stack, true));
                match byte {
                    b'{' => stack.push(Frame::Object {
                        key: None,
                        expect_key: true,
                    }),
                    b'[' => stack.push(Frame::Array {
                        index: 0,
                        started: false,
                    }),
                    _ => {
                        at = token_end;
                        continue;
                    }
                }
            }
        }
        at += 1;
    }
}

/// The byte after a token starting at `at`: a quoted string (to its closing
/// quote, or the line end when unterminated), else a bare word.
fn token_end(bytes: &[u8], at: usize) -> usize {
    match bytes[at] {
        quote @ (b'"' | b'\'') => {
            let mut end = at + 1;
            while end < bytes.len() {
                match bytes[end] {
                    // An escaped line end still ends the line.
                    b'\\' if bytes.get(end + 1) == Some(&b'\n') => return end + 1,
                    b'\\' => end += 2,
                    b'\n' => return end,
                    byte if byte == quote => return end + 1,
                    _ => end += 1,
                }
            }
            bytes.len()
        }
        b'{' | b'[' => at + 1,
        _ => {
            let mut end = at + 1;
            while end < bytes.len()
                && !matches!(
                    bytes[end],
                    b' ' | b'\t' | b'\r' | b'\n' | b',' | b':' | b'{' | b'}' | b'[' | b']' | b'/'
                )
            {
                end += 1;
            }
            end
        }
    }
}

/// A key's text: inside its quotes, or the bare word.
fn key_span(bytes: &[u8], start: usize, end: usize) -> (usize, usize) {
    let quoted = matches!(bytes[start], b'"' | b'\'');
    if quoted {
        let close = end > start + 1 && bytes.get(end - 1) == Some(&bytes[start]);
        (start + 1, if close { end - 1 } else { end })
    } else {
        (start, end)
    }
}

fn memchr_newline(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| from + offset)
}

/// The path of the open containers; with `member`, also the innermost
/// container's current member.
fn path(text: &str, stack: &[Frame], member: bool) -> String {
    let depth = if member {
        stack.len()
    } else {
        stack.len().saturating_sub(1)
    };
    let mut out = String::new();
    for frame in &stack[..depth] {
        match frame {
            Frame::Object {
                key: Some((start, end)),
                ..
            } => super::push_key(&mut out, text.get(*start..*end).unwrap_or_default()),
            Frame::Object { key: None, .. } => {}
            Frame::Array { index, .. } => {
                out.push('[');
                out.push_str(&index.to_string());
                out.push(']');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
