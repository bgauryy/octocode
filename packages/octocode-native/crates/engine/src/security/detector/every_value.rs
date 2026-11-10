//! Every value of a document, for when no walk could keep the document's
//! structure: names and structure stay, and no value can be told from another.
//!
//! [`EveryValueScan`] reads the document in pieces of any size and finds the same
//! spans as one read of all of it, so a source of any length is judged through a
//! bounded buffer and its sealed view is one view, whatever window shows it.
use super::source_names::{Named, SecretForm, SecretSpan};
use std::ops::Range;

/// The spans of every value `bytes` shows of what `named` says it holds, ordered
/// by start.
#[must_use]
pub fn every_value_spans(bytes: &[u8], named: Named) -> Vec<SecretSpan> {
    let mut scan = EveryValueScan::new(named);
    scan.feed(bytes);
    scan.finish()
}

/// Where the scan of a document's JSON strings stands.
#[derive(Clone, Copy)]
enum Text {
    /// Between strings.
    Outside,
    /// Inside a string whose content starts at `start`. No string spans a raw
    /// line, so one left open ends at the line's end; a backslash takes the
    /// byte after it, whatever it is.
    Inside { start: usize, escaped: bool },
    /// A string closed at `end`, and the blanks after it: a `:` next makes it a
    /// member's name.
    Closed { start: usize, end: usize },
    /// The `:` after the blanks: a second `:` makes it no name.
    Colon { start: usize, end: usize },
}

/// Bytes of a line's body that tell `---` and `...` (with a `\r`) from a value.
const HEAD: usize = 4;

/// Where the scan of a YAML line stands.
#[derive(Clone, Copy)]
enum Line {
    /// In the indentation and `- ` markers that come before the line's body.
    Indent,
    /// A `-` that is a marker when a blank follows.
    Dash { at: usize },
    /// The body, which starts at `body`.
    Body {
        body: usize,
        /// The first colon that ends a name, once seen.
        colon: Option<usize>,
        /// A `:` not yet known to end a name: a blank or the line's end must follow.
        pending: Option<usize>,
        /// The body's first bytes, to tell a document separator.
        head: [u8; HEAD],
        len: usize,
    },
}

/// A scan of every value of a document, fed in pieces.
pub struct EveryValueScan {
    named: Named,
    form: SecretForm,
    /// Bytes fed so far.
    at: usize,
    text: Text,
    line: Line,
    spans: Vec<SecretSpan>,
}

impl EveryValueScan {
    #[must_use]
    pub fn new(named: Named) -> Self {
        Self::resume(named, 0)
    }

    /// A scan that starts at byte `at` of its document, in the state every scan
    /// starts in: right where an earlier scan was [`quiet`](Self::quiet).
    #[must_use]
    pub fn resume(named: Named, at: usize) -> Self {
        Self {
            named,
            form: if named.objects.any() {
                named.objects.form()
            } else {
                SecretForm::SecretInYaml
            },
            at,
            text: Text::Outside,
            line: Line::Indent,
            spans: Vec::new(),
        }
    }

    fn push(&mut self, range: Range<usize>, form: SecretForm) {
        if !range.is_empty() {
            self.spans.push(SecretSpan { range, form });
        }
    }

    /// The document's next bytes.
    pub fn feed(&mut self, chunk: &[u8]) {
        let strings = self.named.objects.any();
        let yaml = self.named.yaml;
        for &byte in chunk {
            if strings {
                self.string_byte(byte);
            }
            if yaml {
                self.yaml_byte(byte);
            }
            self.at += 1;
        }
    }

    /// Whether the scan stands where a fresh one starts: between strings, at a
    /// YAML line's start. A scan resumed here finds the spans this one finds
    /// from here on, whatever came before.
    #[must_use]
    pub fn quiet(&self) -> bool {
        matches!(self.text, Text::Outside)
            && (!self.named.yaml || matches!(self.line, Line::Indent))
    }

    /// Feed `chunk` up to and including the first byte that ends at or past
    /// document byte `from` with the scan [`quiet`](Self::quiet); how many
    /// bytes were taken (all of them when none was).
    pub fn feed_until_quiet(&mut self, chunk: &[u8], from: usize) -> usize {
        let strings = self.named.objects.any();
        let yaml = self.named.yaml;
        for (index, &byte) in chunk.iter().enumerate() {
            if strings {
                self.string_byte(byte);
            }
            if yaml {
                self.yaml_byte(byte);
            }
            self.at += 1;
            if self.at >= from && self.quiet() {
                return index + 1;
            }
        }
        chunk.len()
    }

    /// The document byte the next fed byte is.
    #[must_use]
    pub fn at(&self) -> usize {
        self.at
    }

    /// The spans found so far, closed ones only, in no order: a scan fed on
    /// holds none of them.
    pub fn take(&mut self) -> Vec<SecretSpan> {
        std::mem::take(&mut self.spans)
    }

    /// The spans of everything fed, ordered by start.
    #[must_use]
    pub fn finish(mut self) -> Vec<SecretSpan> {
        match self.text {
            Text::Inside { start, .. } => self.push(start..self.at, self.form),
            Text::Closed { start, end } => self.push(start..end, self.form),
            Text::Outside | Text::Colon { .. } => {}
        }
        if self.named.yaml {
            self.end_line(self.at);
        }
        self.spans
            .sort_by_key(|span| (span.range.start, span.range.end));
        self.spans
    }

    fn string_byte(&mut self, byte: u8) {
        let at = self.at;
        match self.text {
            Text::Outside => {
                if byte == b'"' {
                    self.text = Text::Inside {
                        start: at + 1,
                        escaped: false,
                    };
                }
            }
            Text::Inside { start, escaped } => {
                self.text = match (escaped, byte) {
                    (true, _) => Text::Inside {
                        start,
                        escaped: false,
                    },
                    (false, b'"') => Text::Closed { start, end: at },
                    (false, b'\n') => {
                        self.push(start..at, self.form);
                        Text::Outside
                    }
                    (false, b'\\') => Text::Inside {
                        start,
                        escaped: true,
                    },
                    (false, _) => return,
                };
            }
            Text::Closed { start, end } => match byte {
                b' ' | b'\t' | b'\r' | b'\n' => {}
                b':' => self.text = Text::Colon { start, end },
                _ => {
                    self.push(start..end, self.form);
                    self.text = Text::Outside;
                    self.string_byte(byte);
                }
            },
            Text::Colon { start, end } => {
                if byte == b':' {
                    self.push(start..end, self.form);
                }
                self.text = Text::Outside;
                self.string_byte(byte);
            }
        }
    }

    fn yaml_byte(&mut self, byte: u8) {
        let at = self.at;
        if byte == b'\n' {
            self.end_line(at);
            return;
        }
        match self.line {
            Line::Indent => match byte {
                b' ' | b'\t' => {}
                b'-' => self.line = Line::Dash { at },
                _ => self.body_starts(at, byte),
            },
            Line::Dash { at: dash } => {
                if matches!(byte, b' ' | b'\t') {
                    self.line = Line::Indent;
                } else {
                    self.body_starts(dash, b'-');
                    self.yaml_byte(byte);
                }
            }
            Line::Body {
                body,
                mut colon,
                mut pending,
                mut head,
                len,
            } => {
                if colon.is_none() {
                    if let Some(candidate) = pending.take()
                        && matches!(byte, b' ' | b'\t' | b'\r')
                    {
                        colon = Some(candidate);
                    }
                    if colon.is_none() && byte == b':' {
                        pending = Some(at);
                    }
                }
                if len < HEAD {
                    head[len] = byte;
                }
                self.line = Line::Body {
                    body,
                    colon,
                    pending,
                    head,
                    len: len + 1,
                };
            }
        }
    }
    /// The body of the line starts at `body` with `first`.
    fn body_starts(&mut self, body: usize, first: u8) {
        let mut head = [0; HEAD];
        head[0] = first;
        self.line = Line::Body {
            body,
            colon: None,
            // A `:` that starts the body is the colon of a name when a blank follows.
            pending: (first == b':').then_some(body),
            head,
            len: 1,
        };
    }

    /// The line ends at `end` (its `\n`, or the document's end): its value is
    /// what follows its name's colon, or all of its body when it has no name.
    fn end_line(&mut self, end: usize) {
        let (body, colon, head, len) = match std::mem::replace(&mut self.line, Line::Indent) {
            Line::Indent => return,
            Line::Dash { at } => (at, None, [b'-', 0, 0, 0], 1),
            Line::Body {
                body,
                colon,
                pending,
                head,
                len,
            } => (body, colon.or(pending), head, len),
        };
        let separator =
            len <= HEAD && matches!(&head[..len], b"---" | b"..." | b"---\r" | b"...\r");
        if !separator {
            let value = colon.map_or(body, |colon| colon + 1);
            self.push(value..end, SecretForm::SecretInYaml);
        }
    }
}
