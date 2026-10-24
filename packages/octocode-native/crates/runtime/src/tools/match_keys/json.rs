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
    let mut wanted = lines.iter().copied().peekable();
    let mut out = Vec::new();
    let Some(&last) = lines.last() else {
        return out;
    };
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut line = 1;
    // The requested line being scanned: its path so far, and whether a
    // member or element already started on it.
    let mut current: Option<(String, bool)> = None;
    let mut at = 0;
    let mut begin_line = |line: usize, stack: &[Frame], current: &mut Option<(String, bool)>| {
        while wanted.next_if(|&wanted| wanted < line).is_some() {}
        *current = wanted
            .next_if_eq(&line)
            .map(|_| (path(text, stack, false), false));
    };
    begin_line(line, &stack, &mut current);
    while at < bytes.len() && line <= last {
        let byte = bytes[at];
        match byte {
            b'\n' => {
                finish(line, &mut current, &mut out);
                line += 1;
                at += 1;
                begin_line(line, &stack, &mut current);
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
                    finish(line, &mut current, &mut out);
                    line += 1;
                    begin_line(line, &stack, &mut current);
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
                    note(text, &stack, &mut current);
                    at = token_end;
                    continue;
                }
                if let Some(Frame::Array { started, .. }) = stack.last_mut() {
                    *started = true;
                }
                note(text, &stack, &mut current);
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
    finish(line, &mut current, &mut out);
    out
}

/// The first member or element starting on a requested line names it.
fn note(text: &str, stack: &[Frame], current: &mut Option<(String, bool)>) {
    if let Some((line_path, started)) = current
        && !*started
    {
        *line_path = path(text, stack, true);
        *started = true;
    }
}

/// Record the finished line's path, unless it is the document root.
fn finish(line: usize, current: &mut Option<(String, bool)>, out: &mut Vec<(usize, String)>) {
    if let Some((line_path, _)) = current.take()
        && !line_path.is_empty()
    {
        out.push((line, line_path));
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
mod tests {
    use super::*;

    const PACKAGE: &str = r#"{
  "name": "vitest",
  "keywords": [
    "vite",
    "test"
  ],
  "peerDependencies": {
    "vite": "^7"
  },
  "dependencies": {
    "@vitest/mocker": "workspace:*"
  },
  "devDependencies": {
    "vite": "^7"
  },
  "exports": {
    "./node": { "import": "./dist/node.js" }
  }
}
"#;

    /// The same entry reads differently under each section.
    #[test]
    fn hits_name_their_section() {
        assert_eq!(
            line_key_paths(PACKAGE, &[4, 8, 11, 14]),
            vec![
                (4, "keywords[0]".to_owned()),
                (8, "peerDependencies.vite".to_owned()),
                (11, "dependencies.@vitest/mocker".to_owned()),
                (14, "devDependencies.vite".to_owned()),
            ]
        );
    }

    /// A line that opens or closes a container names it; the root is left out.
    #[test]
    fn container_lines_name_the_container() {
        assert_eq!(
            line_key_paths(PACKAGE, &[1, 7, 9, 19]),
            vec![
                (7, "peerDependencies".to_owned()),
                (9, "peerDependencies".to_owned()),
            ]
        );
    }

    /// A key that a dot or bracket would misread is quoted.
    #[test]
    fn keys_with_dots_are_quoted() {
        assert_eq!(
            line_key_paths(PACKAGE, &[17]),
            vec![(17, r#"exports["./node"]"#.to_owned())]
        );
    }

    /// Comments, trailing commas and unquoted keys (JSONC/JSON5) are read
    /// through; array elements count past them.
    #[test]
    fn tolerates_jsonc() {
        let text = "{\n  // a comment, with \"quotes\" {\n  compilerOptions: {\n    /* block\n } */ \"strict\": true,\n  },\n  \"files\": [\"a\", /* x */ \"b\",\n    \"c\",\n  ],\n}\n";
        assert_eq!(
            line_key_paths(text, &[2, 3, 5, 8]),
            vec![
                (3, "compilerOptions".to_owned()),
                (5, "compilerOptions.strict".to_owned()),
                (8, "files[2]".to_owned()),
            ]
        );
    }

    /// Broken nesting degrades to the containers still open, never fails.
    #[test]
    fn broken_json_keeps_scanning() {
        let text =
            "{\n  \"a\": {\n    \"b\": [1, 2\n  },\n  \"c\": \"unterminated\n  \"d\": 1\n}\n";
        let found = line_key_paths(text, &[3, 6]);
        assert_eq!(found[0], (3, "a.b".to_owned()));
        assert_eq!(found.len(), 2, "{found:?}");
    }
}
