use super::constructs::find_regex_literal;
use crate::minify::strategies::utf8_seq_len;

enum State {
    Template,
    Expression(usize),
}

/// Keep the complete template opaque, including nested interpolation syntax.
/// An unfinished construct protects the remaining source. The explicit stack
/// bounds call-stack use even for deeply nested templates.
pub(super) fn end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut states = vec![State::Template];
    let mut pos = start + 1;
    while pos < bytes.len() {
        let Some(state) = states.last_mut() else {
            return pos;
        };
        match state {
            State::Template => match bytes[pos] {
                b'\\' => pos = escaped_end(bytes, pos),
                b'`' => {
                    states.pop();
                    pos += 1;
                    if states.is_empty() {
                        return pos;
                    }
                }
                b'$' if bytes.get(pos + 1) == Some(&b'{') => {
                    states.push(State::Expression(1));
                    pos += 2;
                }
                _ => pos += utf8_seq_len(bytes[pos]),
            },
            State::Expression(depth) => match bytes[pos] {
                b'\'' | b'"' => pos = quoted_end(bytes, pos),
                b'`' => {
                    states.push(State::Template);
                    pos += 1;
                }
                b'{' => {
                    *depth += 1;
                    pos += 1;
                }
                b'}' => {
                    *depth -= 1;
                    if *depth == 0 {
                        states.pop();
                    }
                    pos += 1;
                }
                b'/' if source[pos..].starts_with("//") => {
                    pos = source[pos..].find('\n').map_or(bytes.len(), |at| pos + at);
                }
                b'/' if source[pos..].starts_with("/*") => {
                    pos = source[pos + 2..]
                        .find("*/")
                        .map_or(bytes.len(), |at| pos + 2 + at + 2);
                }
                b'/' => pos = find_regex_literal(source, pos).unwrap_or(pos + 1),
                _ => pos += utf8_seq_len(bytes[pos]),
            },
        }
    }
    bytes.len()
}

fn escaped_end(bytes: &[u8], pos: usize) -> usize {
    let next = pos + 1;
    bytes
        .get(next)
        .map_or(next, |byte| next + utf8_seq_len(*byte))
}

fn quoted_end(bytes: &[u8], start: usize) -> usize {
    let mut pos = start + 1;
    while pos < bytes.len() {
        if bytes[pos] == b'\\' {
            pos = escaped_end(bytes, pos);
        } else if bytes[pos] == bytes[start] {
            return pos + 1;
        } else {
            pos += utf8_seq_len(bytes[pos]);
        }
    }
    bytes.len()
}
