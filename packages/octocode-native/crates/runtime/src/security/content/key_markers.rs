//! Whether a line holds a private-key armor edge, decided on its bytes as they
//! arrive, so a streamed line of any length needs no buffer. Both forms are
//! ASCII: a lossy decode copies every ASCII byte and adds none, so the verdict
//! on raw bytes is the verdict on the decoded line.
//!
//! An armor marker is at least four dashes (RFC 4716 uses four, PEM five),
//! optional blanks, `BEGIN` or `END`, then a label holding `PRIVATE KEY` before
//! the next four dashes. The closing dashes may be cut off a line that ends
//! right after the label. PuTTY's format has no armor, and its lines are named
//! at their start instead: `PuTTY-User-Key-File-N:` and `Private-Lines:` open
//! a block (the second holds the secret half), `Private-MAC:` closes it.

use std::sync::LazyLock;

use memchr::memmem::Finder;

/// The fewest dashes that open or close a marker.
const RUN: u8 = 4;
/// `<PRIVATE><blanks><KEY>` matched, as a [`next_phrase`] position.
const PHRASE_DONE: u8 = 11;
const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

/// Which edges of a private-key block a line holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Markers {
    pub(crate) begins: bool,
    pub(crate) ends: bool,
    /// The last edge on the line is a BEGIN, so a block is open after it:
    /// `-----END …----------BEGIN …-----` from two files joined without a
    /// newline, not a whole block on one line.
    pub(crate) opens: bool,
}

impl Markers {
    pub(crate) fn any(self) -> bool {
        self.begins || self.ends
    }

    fn mark(&mut self, edge: Edge) {
        match edge {
            Edge::Begin => self.begins = true,
            Edge::End => self.ends = true,
        }
        self.opens = matches!(edge, Edge::Begin);
    }
}

#[derive(Clone, Copy, Debug)]
enum Edge {
    Begin,
    End,
}

impl Edge {
    const fn word(self) -> &'static [u8] {
        match self {
            Self::Begin => b"BEGIN",
            Self::End => b"END",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Step {
    /// Counting a run of dashes, up to [`RUN`].
    Seek { dashes: u8 },
    /// Blanks after an opening run.
    Gap,
    /// `matched` bytes of the edge's word.
    Word { edge: Edge, matched: u8 },
    /// The label: how much of the key phrase it has shown, the current run of
    /// dashes, and whether only blanks and dashes followed the whole phrase.
    Label {
        edge: Edge,
        phrase: u8,
        dashes: u8,
        bare: bool,
    },
}

/// [`Markers`] of one line, fed in pieces of any size.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MarkerScan {
    step: Step,
    found: Markers,
}

impl Default for MarkerScan {
    fn default() -> Self {
        Self {
            step: Step::Seek { dashes: 0 },
            found: Markers::default(),
        }
    }
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// Where the key phrase stands after `byte`, from position `at`.
fn next_phrase(at: u8, byte: u8) -> u8 {
    const PRIVATE: &[u8] = b"PRIVATE";
    match at {
        PHRASE_DONE => PHRASE_DONE,
        0..=6 if byte == PRIVATE[usize::from(at)] => at + 1,
        7 | 8 if is_blank(byte) => 8,
        8 if byte == b'K' => 9,
        9 if byte == b'E' => 10,
        10 if byte == b'Y' => PHRASE_DONE,
        _ => u8::from(byte == b'P'),
    }
}

impl MarkerScan {
    pub(crate) fn feed(&mut self, mut bytes: &[u8]) {
        // Almost every line holds no run of dashes at all.
        static OPENING: LazyLock<Finder<'static>> = LazyLock::new(|| Finder::new(b"----"));
        while let Some(&byte) = bytes.first() {
            if matches!(self.step, Step::Seek { dashes: 0 }) {
                let Some(at) = OPENING.find(bytes) else {
                    let dashes = bytes.iter().rev().take_while(|byte| **byte == b'-').count();
                    self.step = Step::Seek {
                        dashes: u8::try_from(dashes).unwrap_or(RUN),
                    };
                    return;
                };
                self.step = Step::Seek { dashes: RUN };
                bytes = &bytes[at + usize::from(RUN)..];
                continue;
            }
            self.step = self.after(byte);
            bytes = &bytes[1..];
        }
    }

    fn after(&mut self, byte: u8) -> Step {
        let restart = Step::Seek {
            dashes: u8::from(byte == b'-'),
        };
        let opening = |byte| match byte {
            b'B' => Step::Word {
                edge: Edge::Begin,
                matched: 1,
            },
            b'E' => Step::Word {
                edge: Edge::End,
                matched: 1,
            },
            _ if is_blank(byte) => Step::Gap,
            _ => restart,
        };
        match self.step {
            Step::Seek { dashes } if byte == b'-' => Step::Seek {
                dashes: (dashes + 1).min(RUN),
            },
            Step::Seek { dashes: RUN } | Step::Gap => opening(byte),
            Step::Seek { .. } => restart,
            Step::Word { edge, matched } => {
                if edge.word().get(usize::from(matched)) != Some(&byte) {
                    restart
                } else if usize::from(matched) + 1 == edge.word().len() {
                    Step::Label {
                        edge,
                        phrase: 0,
                        dashes: 0,
                        bare: false,
                    }
                } else {
                    Step::Word {
                        edge,
                        matched: matched + 1,
                    }
                }
            }
            Step::Label {
                edge,
                phrase,
                dashes,
                bare,
            } => {
                if byte == b'-' {
                    if dashes + 1 == RUN {
                        if phrase == PHRASE_DONE {
                            self.found.mark(edge);
                        }
                        Step::Seek { dashes: RUN }
                    } else {
                        Step::Label {
                            edge,
                            phrase: if phrase == PHRASE_DONE { phrase } else { 0 },
                            dashes: dashes + 1,
                            bare,
                        }
                    }
                } else {
                    let next = next_phrase(phrase, byte);
                    Step::Label {
                        edge,
                        phrase: next,
                        dashes: 0,
                        bare: next == PHRASE_DONE
                            && (phrase != PHRASE_DONE
                                || (bare && (is_blank(byte) || matches!(byte, b'\r' | b'\n')))),
                    }
                }
            }
        }
    }

    /// The line's verdict; `head` is its leading bytes, where PuTTY's lines
    /// are named.
    pub(crate) fn finish(self, head: &[u8]) -> Markers {
        let mut found = self.found;
        if let Step::Label {
            edge,
            phrase: PHRASE_DONE,
            bare: true,
            ..
        } = self.step
        {
            found.mark(edge);
        }
        let named = after_line_lead(head);
        if named.starts_with(b"PuTTY-User-Key-File-") || named.starts_with(b"Private-Lines:") {
            found.mark(Edge::Begin);
        } else if named.starts_with(b"Private-MAC:") {
            found.mark(Edge::End);
        }
        found
    }
}

pub(crate) fn line_markers(line: &[u8]) -> Markers {
    let mut scan = MarkerScan::default();
    scan.feed(line);
    scan.finish(line)
}

/// Whether `content` can hold a line that opens a block. Most sources stop
/// here, untouched.
pub(super) fn may_hold_key_marker(content: &str) -> bool {
    content.contains("PRIVATE")
        || content.contains("Private-Lines:")
        || content.contains("PuTTY-User-Key-File-")
}

/// Quoting and punctuation a key's line can carry on either side of its text.
pub(super) fn is_decoration(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'"' | b'\'' | b'`' | b',' | b';' | b'.' | b'(' | b')' | b'[' | b']' | b'}'
        )
}

/// What marks a line as quoted or commented before its text: `> `, `# `,
/// ` * `, `| `, `- `. A `//` is two base64 characters and is read apart.
pub(super) fn is_lead_mark(byte: u8) -> bool {
    matches!(byte, b'>' | b'#' | b'*' | b'|' | b'-')
}

/// `line` from its first byte of text, past a byte-order mark and any quoting.
pub(super) fn after_line_lead(line: &[u8]) -> &[u8] {
    let mut rest = line.strip_prefix(BYTE_ORDER_MARK).unwrap_or(line);
    loop {
        match rest {
            [b'/', b'/', tail @ ..] => rest = tail,
            [byte, tail @ ..] if is_decoration(*byte) || is_lead_mark(*byte) => rest = tail,
            _ => return rest,
        }
    }
}
