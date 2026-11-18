//! JSON members, scanned (never parsed) so broken nesting degrades to the
//! nearest container still visible.
use super::Entry;
use std::collections::HashSet;

/// A string member of an object that is an array element: its key, its
/// value and its line.
struct Scalar<'a> {
    key: &'a str,
    value: &'a str,
    line: usize,
}

/// The member being read in an object: its entry, key, line, and whether
/// its value is still to come.
struct Member<'a> {
    entry: usize,
    key: &'a str,
    value_pending: bool,
}

/// One open JSON container.
struct Frame<'a> {
    object: bool,
    expect_key: bool,
    member: Option<Member<'a>>,
    /// The entry this container's key (or element slot) opened.
    opened: Option<usize>,
    /// An object that is an array element: its string members.
    scalars: Option<Vec<Scalar<'a>>>,
    /// An array: the string members of each object element, with the
    /// element's depth.
    elements: Vec<(usize, Vec<Scalar<'a>>)>,
}

impl Frame<'_> {
    fn open(object: bool, opened: Option<usize>, element: bool) -> Self {
        Self {
            object,
            expect_key: object,
            member: None,
            opened,
            scalars: element.then(Vec::new),
            elements: Vec::new(),
        }
    }
}

/// Members of JSON objects: each key's line and the span of a container
/// value; each object of an array, with its naming member.
pub(super) fn json_entries(content: &str) -> Vec<Entry> {
    let bytes = content.as_bytes();
    let mut entries = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut line = 1;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => line += 1,
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = bytes[at..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |end| at + end);
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = content[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| at + 2 + end + 2);
                line += content[at..end].matches('\n').count();
                at = end;
                continue;
            }
            b'"' | b'\'' => {
                let end = string_end(bytes, at);
                // The token's text without its quotes (to the source end
                // when unterminated).
                let close = if end > at + 1 && bytes[end - 1] == bytes[at] {
                    end - 1
                } else {
                    end
                };
                let token = &content[at + 1..close];
                let token_line = line;
                line += content[at..end].matches('\n').count();
                if let Some(frame) = stack.last_mut()
                    && frame.object
                {
                    if frame.expect_key {
                        frame.expect_key = false;
                        frame.member = Some(Member {
                            entry: entries.len(),
                            key: token,
                            value_pending: true,
                        });
                        entries.push(Entry::at(token_line, stack.len()));
                    } else if let Some(member) = frame.member.as_mut()
                        && member.value_pending
                    {
                        member.value_pending = false;
                        if let Some(scalars) = frame.scalars.as_mut() {
                            scalars.push(Scalar {
                                key: member.key,
                                value: token,
                                line: token_line,
                            });
                        }
                    }
                }
                at = end;
                continue;
            }
            b',' => {
                if let Some(frame) = stack.last_mut() {
                    frame.expect_key = frame.object;
                    frame.member = None;
                }
            }
            open @ (b'{' | b'[') => {
                let depth = stack.len();
                let (opened, element) = match stack.last_mut() {
                    // A container value of a named member spans from its key.
                    Some(frame) if frame.object => (
                        frame.member.as_mut().map(|member| {
                            member.value_pending = false;
                            member.entry
                        }),
                        false,
                    ),
                    // An object element of a listed array spans from its
                    // opening line.
                    Some(_) if open == b'{' => {
                        entries.push(Entry::at(line, depth));
                        (Some(entries.len() - 1), true)
                    }
                    _ => (None, false),
                };
                stack.push(Frame::open(open == b'{', opened, element));
            }
            b'}' | b']' => {
                if let Some(frame) = stack.pop() {
                    if let Some(index) = frame.opened {
                        entries[index].end = line;
                    }
                    if let (Some(scalars), Some(index), Some(parent)) =
                        (frame.scalars, frame.opened, stack.last_mut())
                    {
                        parent.elements.push((entries[index].depth, scalars));
                    }
                    for (depth, scalar) in identity(&frame.elements) {
                        entries.push(Entry {
                            names_element: true,
                            ..Entry::at(scalar.line, depth)
                        });
                    }
                }
            }
            _ => {}
        }
        at += 1;
    }
    entries
}

/// The member that names each object of an array: a key every object
/// carries as a string, with a different value in each; of several, the one
/// with the shortest values (a name over a description). An array of fewer
/// than two objects needs no names.
fn identity<'s, 'a>(elements: &'s [(usize, Vec<Scalar<'a>>)]) -> Vec<(usize, &'s Scalar<'a>)> {
    let [(_, first), _, ..] = elements else {
        return Vec::new();
    };
    let named_by = |key: &str| -> Option<(usize, Vec<(usize, &'s Scalar<'a>)>)> {
        let mut seen = HashSet::new();
        let mut names = Vec::with_capacity(elements.len());
        for (depth, scalars) in elements {
            let scalar = scalars.iter().find(|scalar| scalar.key == key)?;
            seen.insert(scalar.value).then_some(())?;
            names.push((*depth, scalar));
        }
        let length = names.iter().map(|(_, scalar)| scalar.value.len()).sum();
        Some((length, names))
    };
    first
        .iter()
        .filter_map(|candidate| named_by(candidate.key))
        .min_by_key(|(length, _)| *length)
        .map(|(_, names)| names)
        .unwrap_or_default()
}

/// The byte after the string token starting at `at`: its closing quote, or
/// the end of the source when unterminated.
fn string_end(bytes: &[u8], at: usize) -> usize {
    let quote = bytes[at];
    let mut end = at + 1;
    while end < bytes.len() {
        match bytes[end] {
            b'\\' => end += 2,
            byte if byte == quote => return end + 1,
            _ => end += 1,
        }
    }
    bytes.len()
}
