//! TOML key paths: `[table]` and `[[array]]` headers, `key =` lines, and
//! the elements of multi-line arrays, inline tables and strings. A header
//! line names its table (`bin[1]` for the second `[[bin]]`), a key line its
//! key under the current table (`dependencies.serde`), and a line inside a
//! multi-line value the element that starts first on it
//! (`workspace.members[0]`), else the container it sits in.
use super::push_key;

/// One open container of a multi-line value.
enum Frame {
    /// `index`: the current element.
    Array { index: usize },
    /// `key`: the current member's key segments; `expect_key`: the next
    /// token names a member.
    Inline {
        key: Option<Vec<String>>,
        expect_key: bool,
    },
}

/// A value still open at a line end: the path of the key that owns it, its
/// open containers, and an open multi-line string's delimiter.
struct Open {
    key: String,
    frames: Vec<Frame>,
    string: Option<&'static str>,
}

impl Open {
    /// The owning key and the open containers; with `member`, also the
    /// innermost container's current member.
    fn path(&self, member: bool) -> String {
        let depth = if member {
            self.frames.len()
        } else {
            self.frames.len().saturating_sub(1)
        };
        let mut out = self.key.clone();
        for frame in &self.frames[..depth] {
            match frame {
                Frame::Array { index } => {
                    out.push('[');
                    out.push_str(&index.to_string());
                    out.push(']');
                }
                Frame::Inline {
                    key: Some(segments),
                    ..
                } => {
                    for segment in segments {
                        push_key(&mut out, segment);
                    }
                }
                Frame::Inline { key: None, .. } => {}
            }
        }
        out
    }

    /// Scan `line` from `at` to its end: containers open and close, and
    /// the first element or member starting on the line sets `note`.
    fn scan(&mut self, line: &str, mut at: usize, note: &mut Option<String>) {
        let bytes = line.as_bytes();
        while at < bytes.len() {
            if let Some(delimiter) = self.string {
                match line[at..].find(delimiter) {
                    Some(offset) => {
                        self.string = None;
                        at += offset + delimiter.len();
                        continue;
                    }
                    None => return,
                }
            }
            match bytes[at] {
                b' ' | b'\t' | b'\r' | b'=' => at += 1,
                b'#' => return,
                b',' => {
                    match self.frames.last_mut() {
                        Some(Frame::Array { index }) => *index += 1,
                        Some(Frame::Inline { expect_key, .. }) => *expect_key = true,
                        None => {}
                    }
                    at += 1;
                }
                b']' | b'}' => {
                    self.frames.pop();
                    at += 1;
                }
                byte => {
                    let member_key = matches!(
                        self.frames.last(),
                        Some(Frame::Inline {
                            expect_key: true,
                            ..
                        })
                    );
                    if member_key {
                        let Some((segments, end)) = parse_key(line, at, b'=') else {
                            return;
                        };
                        if let Some(Frame::Inline { key, expect_key }) = self.frames.last_mut() {
                            *key = Some(segments);
                            *expect_key = false;
                        }
                        note.get_or_insert_with(|| self.path(true));
                        at = end;
                        continue;
                    }
                    note.get_or_insert_with(|| self.path(true));
                    at = match byte {
                        b'[' => {
                            self.frames.push(Frame::Array { index: 0 });
                            at + 1
                        }
                        b'{' => {
                            self.frames.push(Frame::Inline {
                                key: None,
                                expect_key: true,
                            });
                            at + 1
                        }
                        b'"' | b'\'' => self.string_end(line, at),
                        _ => bare_end(bytes, at),
                    };
                }
            }
        }
    }

    /// The byte after the string starting at `at`; a multi-line string not
    /// closed on this line stays open.
    fn string_end(&mut self, line: &str, at: usize) -> usize {
        let bytes = line.as_bytes();
        let quote = bytes[at];
        if bytes.get(at..at + 3) == Some(&[quote; 3]) {
            let delimiter = if quote == b'"' { "\"\"\"" } else { "'''" };
            return match line[at + 3..].find(delimiter) {
                Some(offset) => at + 3 + offset + 3,
                None => {
                    self.string = Some(delimiter);
                    bytes.len()
                }
            };
        }
        closing_quote(bytes, at + 1, quote).map_or(bytes.len(), |close| close + 1)
    }
}

/// The document read so far.
#[derive(Default)]
struct Doc {
    /// The current table's path.
    table: String,
    /// `[[array]]` tables seen: header keys and their current element.
    arrays: Vec<(Vec<String>, usize)>,
    open: Option<Open>,
}

impl Doc {
    /// The path `line` names, after the lines before it.
    fn line(&mut self, line: &str) -> String {
        if let Some(open) = self.open.as_mut() {
            let container = open.path(open.string.is_some());
            let mut note = None;
            open.scan(line, 0, &mut note);
            if open.frames.is_empty() && open.string.is_none() {
                self.open = None;
            }
            return note.unwrap_or(container);
        }
        let bytes = line.as_bytes();
        let at = skip_blanks(bytes, 0);
        match bytes.get(at) {
            None | Some(b'#') => return self.table.clone(),
            Some(b'[') => {
                let array = bytes.get(at + 1) == Some(&b'[');
                if let Some((segments, _)) = parse_key(line, at + 1 + usize::from(array), b']') {
                    if array {
                        self.enter_array(&segments);
                    }
                    self.table = self.header(&segments);
                }
                return self.table.clone();
            }
            Some(_) => {}
        }
        let Some((segments, after)) = parse_key(line, at, b'=') else {
            return self.table.clone();
        };
        let mut key = self.table.clone();
        for segment in &segments {
            push_key(&mut key, segment);
        }
        let mut open = Open {
            key: key.clone(),
            frames: Vec::new(),
            string: None,
        };
        // The key names its own line, whatever its value opens.
        let mut note = Some(String::new());
        open.scan(line, after, &mut note);
        if !open.frames.is_empty() || open.string.is_some() {
            self.open = Some(open);
        }
        key
    }

    /// A new element of the `[[array]]` table `segments`: arrays nested in
    /// its previous element are done.
    fn enter_array(&mut self, segments: &[String]) {
        self.arrays
            .retain(|(path, _)| !(path.len() > segments.len() && path.starts_with(segments)));
        match self.arrays.iter_mut().find(|(path, _)| path == segments) {
            Some((_, index)) => *index += 1,
            None => self.arrays.push((segments.to_vec(), 0)),
        }
    }

    /// A header's path: every `[[array]]` table on it indexed by its
    /// current element.
    fn header(&self, segments: &[String]) -> String {
        let mut out = String::new();
        for end in 1..=segments.len() {
            push_key(&mut out, &segments[end - 1]);
            if let Some((_, index)) = self
                .arrays
                .iter()
                .find(|(path, _)| path.as_slice() == &segments[..end])
            {
                out.push('[');
                out.push_str(&index.to_string());
                out.push(']');
            }
        }
        out
    }
}

/// The key path of each of `lines` (1-based, ascending) in `text`. Lines at
/// the document root are left out.
pub(super) fn line_key_paths(text: &str, lines: &[usize]) -> Vec<(usize, String)> {
    let Some(&last) = lines.last() else {
        return Vec::new();
    };
    let mut wanted = lines.iter().copied().peekable();
    let mut doc = Doc::default();
    let mut out = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index + 1;
        if number > last {
            break;
        }
        let path = doc.line(line);
        while wanted.next_if(|&wanted| wanted < number).is_some() {}
        if wanted.next_if_eq(&number).is_some() && !path.is_empty() {
            out.push((number, path));
        }
    }
    out
}

/// A dotted key from `at` through its `end` byte (`=` for a key, `]` for a
/// header): its segments and the byte after `end`; `None` when the text is
/// not a key.
fn parse_key(line: &str, mut at: usize, end: u8) -> Option<(Vec<String>, usize)> {
    let bytes = line.as_bytes();
    let mut segments = Vec::new();
    loop {
        at = skip_blanks(bytes, at);
        match *bytes.get(at)? {
            b'\'' => {
                let close = closing_quote(bytes, at + 1, b'\'')?;
                segments.push(line[at + 1..close].to_owned());
                at = close + 1;
            }
            b'"' => {
                let close = closing_quote(bytes, at + 1, b'"')?;
                segments.push(
                    line[at + 1..close]
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\"),
                );
                at = close + 1;
            }
            _ => {
                let start = at;
                while bytes
                    .get(at)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    at += 1;
                }
                if at == start {
                    return None;
                }
                segments.push(line[start..at].to_owned());
            }
        }
        at = skip_blanks(bytes, at);
        match *bytes.get(at)? {
            b'.' => at += 1,
            byte if byte == end => return Some((segments, at + 1)),
            _ => return None,
        }
    }
}

/// The closing `quote` from `at`; a basic string (`"`) skips escapes.
fn closing_quote(bytes: &[u8], mut at: usize, quote: u8) -> Option<usize> {
    while at < bytes.len() {
        match bytes[at] {
            b'\\' if quote == b'"' => at += 2,
            byte if byte == quote => return Some(at),
            _ => at += 1,
        }
    }
    None
}

fn skip_blanks(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        at += 1;
    }
    at
}

/// The byte after a bare value (number, boolean, date) starting at `at`.
fn bare_end(bytes: &[u8], at: usize) -> usize {
    let mut end = at + 1;
    while end < bytes.len()
        && !matches!(
            bytes[end],
            b' ' | b'\t' | b'\r' | b',' | b']' | b'}' | b'#' | b'='
        )
    {
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests;
