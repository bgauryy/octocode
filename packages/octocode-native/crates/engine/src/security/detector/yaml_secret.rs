//! A Kubernetes Secret in YAML: `kind: Secret` says what the `data` and `stringData`
//! blocks of its own mapping are, on either side of it and however long the
//! annotations between them run. Indentation says which mapping a line is in, so
//! one pass over the lines finds them, and `data` in another mapping, or in
//! another document (after a `---`), stays.
//!
//! It takes the text in pieces of any size, keeping only each line's head: what a
//! line says of its mapping is at its start. A line that outruns its head and
//! names a `kind` or a block counts as saying the most it could.
use std::ops::Range;

/// Bytes of a line kept for what it says of its mapping.
const LINE_HEAD_BYTES: usize = 4096;

struct Mapping {
    indent: usize,
    secret: bool,
    /// Where this mapping's blocks start in [`Blocks::held`].
    held_from: usize,
    /// Where its open block's text starts.
    block: Option<usize>,
}

/// A line's place in the source.
#[derive(Clone, Copy)]
struct Line {
    start: usize,
    /// Past its last byte before its line break.
    body_end: usize,
    /// Where the next line starts.
    next: usize,
    /// Its head is not all of it.
    cut: bool,
}

/// The values of every Secret's `data` and `stringData` blocks in `text`.
pub(super) fn secret_blocks(text: &str) -> Vec<Range<usize>> {
    let mut blocks = Blocks::default();
    blocks.feed(text.as_bytes());
    blocks.finish().unwrap_or_default()
}

/// The values of every Secret's `data` and `stringData` blocks in `text`:
/// what [`block_values`] reads of each block.
pub(super) fn secret_values(text: &str) -> Vec<Range<usize>> {
    let mut values = Vec::new();
    for block in secret_blocks(text) {
        block_values(text, block, &mut values);
    }
    values
}

/// Adds to `values` the values the lines of `block` hold in `text`, each
/// without the name before it, the quotes around it and the comment after
/// it: a member's value, and each line of a value that runs over several (a
/// block scalar's, where a line that reads as a member keeps its name too).
/// The names, the indentation, a comment, a template directive and a
/// placeholder stay. A block that starts within its line is a flow value:
/// all of it is one.
pub(super) fn block_values(text: &str, block: Range<usize>, values: &mut Vec<Range<usize>>) {
    let bytes = text.as_bytes();
    if block.start > 0 && bytes.get(block.start - 1) != Some(&b'\n') {
        values.push(block);
        return;
    }
    let mut line = block.start;
    while line < block.end {
        let Some(rest) = bytes.get(line..block.end) else {
            return;
        };
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(block.end, |at| line + at);
        let Some(body) = bytes.get(line..end) else {
            return;
        };
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        let mut member = trim_start(body);
        while member == b"-" || member.starts_with(b"- ") || member.starts_with(b"-\t") {
            member = trim_start(&member[1..]);
        }
        let value = match name_and_value(member) {
            Some((_, value_at)) => before_comment(trim_start(&member[value_at..])),
            None => member,
        };
        if !(value.is_empty()
            || member.starts_with(b"#")
            || member.starts_with(b"{{")
            || opens_a_block_scalar(value))
        {
            // `value` is a slice of the line: where it lies in the text.
            let start = line + (value.as_ptr() as usize).saturating_sub(body.as_ptr() as usize);
            let inner = unquoted(value);
            let quote = usize::from(inner.len() < value.len());
            let range = start + quote..start + quote + inner.len();
            // What an earlier pass left in a value's place is none.
            if !text.get(range.clone()).is_some_and(super::is_placeholder) {
                values.push(range);
            }
        }
        line = end + 1;
    }
}

/// Whether `value` is a block scalar's header (`|`, `>-`, `|2+`): its text
/// is on the lines after it.
fn opens_a_block_scalar(value: &[u8]) -> bool {
    value
        .first()
        .is_some_and(|byte| matches!(byte, b'|' | b'>'))
        && value[1..]
            .iter()
            .all(|byte| matches!(byte, b'+' | b'-') || byte.is_ascii_digit())
}

/// [`secret_blocks`] of a source that passes in pieces.
#[derive(Default)]
pub(super) struct Blocks {
    mappings: Vec<Mapping>,
    held: Vec<Range<usize>>,
    found: Vec<Range<usize>>,
    /// The end of the last line read that can belong to a block.
    last_end: usize,
    /// The current line's head, its bytes so far and where it starts.
    head: Vec<u8>,
    line_len: usize,
    line_start: usize,
    /// The `\r`s that end the current line so far.
    returns: usize,
    gave_up: bool,
}

fn trim_start(text: &[u8]) -> &[u8] {
    let blanks = text
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    &text[blanks..]
}

impl Blocks {
    /// Take the source's next `bytes`.
    pub(super) fn feed(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() && !self.gave_up {
            match bytes.iter().position(|byte| *byte == b'\n') {
                Some(at) => {
                    self.take(&bytes[..at]);
                    self.close_line(true);
                    bytes = &bytes[at + 1..];
                }
                None => {
                    self.take(bytes);
                    bytes = &[];
                }
            }
        }
    }

    fn take(&mut self, piece: &[u8]) {
        let room = LINE_HEAD_BYTES
            .saturating_sub(self.head.len())
            .min(piece.len());
        self.head.extend_from_slice(&piece[..room]);
        self.line_len += piece.len();
        let ending = piece
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\r')
            .count();
        self.returns = if ending == piece.len() {
            self.returns + ending
        } else {
            ending
        };
    }

    fn close_line(&mut self, broken: bool) {
        let body_len = self.line_len - self.returns;
        let head = std::mem::take(&mut self.head);
        let line = Line {
            start: self.line_start,
            body_end: self.line_start + body_len,
            next: self.line_start + self.line_len + usize::from(broken),
            cut: body_len > head.len(),
        };
        self.line(&head[..head.len().min(body_len)], line);
        self.head = head;
        self.head.clear();
        self.line_start = line.next;
        self.line_len = 0;
        self.returns = 0;
    }

    /// Stop: this source is more than the walk can keep.
    pub(super) fn give_up(&mut self) {
        self.gave_up = true;
        self.mappings = Vec::new();
        self.held = Vec::new();
        self.found = Vec::new();
    }

    /// About how many bytes the walk holds.
    pub(super) fn weight(&self) -> usize {
        self.mappings.capacity() * std::mem::size_of::<Mapping>()
            + (self.held.capacity() + self.found.capacity()) * std::mem::size_of::<Range<usize>>()
            + self.head.capacity()
    }

    /// The blocks of the whole source, or `None` when the walk gave up.
    pub(super) fn finish(mut self) -> Option<Vec<Range<usize>>> {
        if self.line_len > 0 && !self.gave_up {
            self.close_line(false);
        }
        if self.gave_up {
            return None;
        }
        self.close_deeper_than(None);
        Some(self.found)
    }

    fn line(&mut self, body: &[u8], line: Line) {
        let mut rest = trim_start(body);
        let mut column = body.len() - rest.len();
        if rest.is_empty() {
            // Blanks past the head hide the column whatever follows them.
            if line.cut {
                self.give_up();
            }
            return;
        }
        if rest.starts_with(b"#") {
            return;
        }
        // A template directive sits at any indent and ends no mapping.
        if rest.starts_with(b"{{") {
            self.last_end = line.body_end;
            return;
        }
        // A document's start or end closes every mapping of the document: at
        // the margin, or in a text indented as a whole (a manifest quoted in
        // a list, a request's text) at that text's own margin, so a `kind:
        // Secret` says nothing of a ConfigMap in the document before it.
        if rest == b"---" || rest.starts_with(b"--- ") || rest == b"..." {
            self.close_deeper_than(column.checked_sub(1));
            return;
        }
        while rest == b"-" || rest.starts_with(b"- ") || rest.starts_with(b"-\t") {
            self.enter_item(column);
            let after = trim_start(&rest[1..]);
            column += rest.len() - after.len();
            rest = after;
        }
        if !rest.is_empty() {
            self.enter(column);
            let offset = line.start + body.len() - rest.len();
            self.member(rest, offset, line);
        }
        self.last_end = line.body_end;
    }

    /// A `-` at `column` starts an item: what the last item held has ended.
    fn enter_item(&mut self, column: usize) {
        self.close_deeper_than(Some(column));
        if let Some(top) = self.mappings.last_mut()
            && top.indent == column
        {
            close_block(top, self.last_end, &mut self.held);
        }
    }

    /// A line at `column` belongs to the mapping at that indent, or starts one.
    fn enter(&mut self, column: usize) {
        self.close_deeper_than(Some(column));
        match self.mappings.last_mut() {
            Some(top) if top.indent == column => {
                close_block(top, self.last_end, &mut self.held);
            }
            _ => self.mappings.push(Mapping {
                indent: column,
                secret: false,
                held_from: self.held.len(),
                block: None,
            }),
        }
    }

    /// Close every mapping deeper than `column`, or all of them.
    fn close_deeper_than(&mut self, column: Option<usize>) {
        while let Some(top) = self
            .mappings
            .last_mut()
            .filter(|top| column.is_none_or(|column| top.indent > column))
        {
            close_block(top, self.last_end, &mut self.held);
            let secret = top.secret;
            let held_from = top.held_from;
            self.mappings.pop();
            if secret {
                self.found.extend(self.held.drain(held_from..));
            } else {
                self.held.truncate(held_from);
            }
        }
    }

    /// `text`, starting at `offset`, in the current mapping of `line`.
    fn member(&mut self, text: &[u8], offset: usize, line: Line) {
        let Some((name, value_at)) = name_and_value(text) else {
            return;
        };
        let value = trim_start(&text[value_at..]);
        let value_offset = offset + text.len() - value.len();
        let value = before_comment(value);
        let Some(top) = self.mappings.last_mut() else {
            return;
        };
        if name.eq_ignore_ascii_case(b"kind") {
            top.secret |= line.cut || unquoted(value).eq_ignore_ascii_case(b"Secret");
        } else if name.eq_ignore_ascii_case(b"data") || name.eq_ignore_ascii_case(b"stringData") {
            top.block = if line.cut {
                Some(value_offset)
            } else if value.is_empty() {
                Some(line.next)
            } else if holds_nothing(value) {
                None
            } else {
                Some(value_offset)
            };
        }
    }
}

fn close_block(mapping: &mut Mapping, last_end: usize, held: &mut Vec<Range<usize>>) {
    if let Some(start) = mapping.block.take()
        && last_end > start
    {
        held.push(start..last_end);
    }
}

/// A `name: value` line's name, and where what follows its colon starts.
fn name_and_value(text: &[u8]) -> Option<(&[u8], usize)> {
    if let Some(quote) = text
        .first()
        .copied()
        .filter(|first| matches!(first, b'"' | b'\''))
    {
        let end = 1 + text[1..].iter().position(|byte| *byte == quote)?;
        return text[end + 1..]
            .starts_with(b":")
            .then_some((&text[1..end], end + 2));
    }
    let colon = text.iter().enumerate().position(|(at, byte)| {
        *byte == b':' && matches!(text.get(at + 1), None | Some(b' ' | b'\t'))
    })?;
    Some((&text[..colon], colon + 1))
}

/// A value without the comment after it.
fn before_comment(value: &[u8]) -> &[u8] {
    if value.starts_with(b"#") {
        return b"";
    }
    let comment = (1..value.len()).find(|&at| value[at] == b'#' && value[at - 1] == b' ');
    let value = comment.map_or(value, |at| &value[..at]);
    std::str::from_utf8(value).map_or_else(
        |_| value.trim_ascii_end(),
        |text| text.trim_end().as_bytes(),
    )
}

fn unquoted(value: &[u8]) -> &[u8] {
    value
        .strip_prefix(b"\"")
        .and_then(|inner| inner.strip_suffix(b"\""))
        .or_else(|| {
            value
                .strip_prefix(b"'")
                .and_then(|inner| inner.strip_suffix(b"'"))
        })
        .unwrap_or(value)
}

fn holds_nothing(value: &[u8]) -> bool {
    matches!(value, b"{}" | b"[]" | b"null" | b"~" | b"\"\"" | b"''")
}
