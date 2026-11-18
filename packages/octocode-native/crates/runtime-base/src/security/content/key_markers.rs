//! Whether a line holds a private-key armor edge, decided on its bytes as they
//! arrive, so a streamed line of any length needs no buffer. Both forms are
//! ASCII: a lossy decode copies every ASCII byte and adds none, so the verdict
//! on raw bytes is the verdict on the decoded line.
//!
//! An armor marker is at least four dashes (RFC 4716 uses four, PEM five),
//! optional blanks, `BEGIN` or `END`, then a label holding `PRIVATE KEY`,
//! `SECRET KEY` or `Static key` (OpenVPN's shared key) before the next four dashes. The closing dashes may be cut off a line that ends
//! right after the label. PuTTY's format has no armor, and its lines are named
//! at their start instead: `Private-Lines:` opens a block (the secret half;
//! the header lines and public lines before it hold no private key), and
//! `Private-MAC:` closes it, each only with its value after the name.

use std::sync::LazyLock;

use memchr::memmem::Finder;

/// The fewest dashes that open or close a marker.
const RUN: u8 = 4;
/// `<PRIVATE><blanks><KEY>` matched, as a [`next_phrase`] position. Positions
/// from [`SECRET_AT`] and [`STATIC_AT`] spell the phrase's other first words.
const PHRASE_DONE: u8 = 11;
const SECRET_AT: u8 = 12;
const STATIC_AT: u8 = 17;

/// Which edges of a private-key block a line holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Markers {
    pub(crate) begins: bool,
    pub(crate) ends: bool,
    /// The last edge on the line is a BEGIN, so a block is open after it:
    /// `-----END …----------BEGIN …-----` from two files joined without a
    /// newline, not a whole block on one line.
    pub(crate) opens: bool,
}

impl Markers {
    pub fn any(self) -> bool {
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
pub struct MarkerScan {
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
    const SECRET: &[u8] = b"SECRET";
    const STATIC: &[u8] = b"Static";
    match at {
        PHRASE_DONE => PHRASE_DONE,
        0..=6 if byte == PRIVATE[usize::from(at)] => at + 1,
        // The last letter of a first word stands where `PRIVATE`'s does.
        // After `S`, the second letter picks the word.
        SECRET_AT if byte == b't' => STATIC_AT,
        SECRET_AT..=16 if byte == SECRET[usize::from(at - SECRET_AT + 1)] => {
            if at == 16 {
                7
            } else {
                at + 1
            }
        }
        STATIC_AT..=20 if byte == STATIC[usize::from(at - STATIC_AT + 2)] => {
            if at == 20 {
                7
            } else {
                at + 1
            }
        }
        7 | 8 if is_blank(byte) => 8,
        8 if matches!(byte, b'K' | b'k') => 9,
        9 if matches!(byte, b'E' | b'e') => 10,
        10 if matches!(byte, b'Y' | b'y') => PHRASE_DONE,
        _ => match byte {
            b'P' => 1,
            b'S' => SECRET_AT,
            _ => 0,
        },
    }
}

impl MarkerScan {
    pub fn feed(&mut self, mut bytes: &[u8]) {
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
    pub fn finish(self, head: &[u8]) -> Markers {
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
        if let Some(edge) = putty_edge(after_line_lead(head)) {
            found.mark(edge);
        }
        found
    }
}

/// The edge a PuTTY line names at its start, `named`: `Private-Lines:` and
/// its line count open a block, `Private-MAC:` and its hex digest close it.
/// A name without its value (prose or a doc comment that mentions the field)
/// names no edge. The value follows the name's blanks and ends where `named`
/// ends or at a byte that is no letter or digit.
fn putty_edge(named: &[u8]) -> Option<Edge> {
    let (rest, edge, holds): (_, _, fn(&u8) -> bool) =
        if let Some(rest) = named.strip_prefix(b"Private-Lines:".as_slice()) {
            (rest, Edge::Begin, u8::is_ascii_digit)
        } else {
            let rest = named.strip_prefix(b"Private-MAC:".as_slice())?;
            (rest, Edge::End, u8::is_ascii_hexdigit)
        };
    let blanks = rest
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let value = &rest[blanks..];
    let digits = value.iter().take_while(|byte| holds(byte)).count();
    let ends = value
        .get(digits)
        .is_none_or(|byte| !byte.is_ascii_alphanumeric());
    (digits > 0 && ends).then_some(edge)
}

pub(crate) fn line_markers(line: &[u8]) -> Markers {
    let mut scan = MarkerScan::default();
    scan.feed(line);
    scan.finish(line)
}

/// Whether `content` can hold a line that opens a block. Most sources stop
/// here, untouched.
pub(super) fn may_hold_key_marker(content: &str) -> bool {
    // `memmem` searches in memchr's own compiled code, one fast pass per marker
    // a long text lacks; each marker's searcher is built once, not per text.
    static MARKERS: LazyLock<[Finder<'static>; 4]> = LazyLock::new(|| {
        ["PRIVATE", "SECRET KEY", "Static key", "Private-Lines:"]
            .map(|marker| Finder::new(marker.as_bytes()))
    });
    MARKERS
        .iter()
        .any(|marker| marker.find(content.as_bytes()).is_some())
}

/// Quoting and punctuation a key's line can carry on either side of its text.
pub(super) fn is_decoration(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'"' | b'\'' | b'`' | b',' | b';' | b'.' | b'(' | b')' | b'[' | b']' | b'}'
        )
}

/// Whether `ch` may stand before a line's text as its lead: whitespace,
/// punctuation or a symbol of any script (a comment mark, a quote, a bullet,
/// a box-drawing tree, an ellipsis, a control or format character), anything
/// but a letter or digit of any script, a base64 byte (`+`, `/`, `=`) or the
/// escape `\`. Every byte of a key's text is an ASCII letter, digit, `+`, `/`
/// or `=`, so a lead holds none of it: it ends at the key's first byte. A
/// word in any script before a key's run is no lead but its label. A `//` is
/// two base64 characters and is read apart.
pub(super) fn is_lead_char(ch: char) -> bool {
    !(ch.is_alphanumeric() || matches!(ch, '+' | '/' | '=' | '\\'))
}

/// The first character of `bytes` and how many bytes it takes; a byte no
/// character decodes from reads as U+FFFD, as a lossy decode shows it, so a
/// verdict on raw bytes is the verdict on their decoded text. `None` when
/// `bytes` is empty.
fn first_char(bytes: &[u8]) -> Option<(char, usize)> {
    let &first = bytes.first()?;
    if first.is_ascii() {
        return Some((char::from(first), 1));
    }
    let head = bytes.get(..4).unwrap_or(bytes);
    let valid = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(error) => head
            .get(..error.valid_up_to())
            .and_then(|valid| std::str::from_utf8(valid).ok())
            .unwrap_or_default(),
    };
    Some(
        valid
            .chars()
            .next()
            .map_or((char::REPLACEMENT_CHARACTER, 1), |ch| (ch, ch.len_utf8())),
    )
}

/// `line` from its first byte of text, past its lead ([`is_lead_char`]: a
/// byte-order mark, indentation, quoting, a comment mark, a bullet, a tree).
pub(super) fn after_line_lead(line: &[u8]) -> &[u8] {
    let mut rest = line;
    loop {
        if let [b'/', b'/', tail @ ..] = rest {
            rest = tail;
            continue;
        }
        match first_char(rest) {
            Some((ch, len)) if is_lead_char(ch) => rest = rest.get(len..).unwrap_or_default(),
            _ => return rest,
        }
    }
}
