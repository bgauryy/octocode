//! What a text's objects hold, found by walking its nesting once. A Kubernetes
//! Secret's `kind` and a JSON Web Key's `kty` say what a member of the same object
//! is, however much lies between them: a walk knows which object a member belongs
//! to, where a pattern could only bound the distance.
//!
//! The walk is lenient, as the text may be source code, a cut of a document or
//! prose around one. Unmatched closers are read past, and whatever is still open
//! at the end counts as closed. Text outside every brace is one object of its own
//! (a window of a document, or YAML), until a closer with no opener ends it.
//! Double-quoted strings are opaque, except an object held escaped inside one
//! (`"{\"kind\":\"Secret\"…}"`), which is walked in place: a quote's backslashes
//! say how deeply it is held, so no text is walked twice.
//!
//! It takes the text in pieces of any size, each byte once, so a source too large
//! to hold is walked as it passes. What it keeps is its open frames and the spans
//! it found, weighed by [`Walk::weight`]; a token waits in [`Walk::carry`] only
//! while the next byte decides what it is, and a string or a name is summed up as
//! it passes, however long.
use super::is_identifier_name;
use std::ops::Range;

mod frames;
mod names;
mod strings;

use names::OpenName;
use strings::OpenString;

/// Bytes a token may wait for the one that decides it. Past this the walk gives
/// up, and its caller treats the whole source as unjudged.
const CARRY_BYTES: usize = 1024 * 1024;

/// What a member's name says about its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// `kty`: this object is a JSON Web Key.
    KeyType,
    /// `kind`: this object is a Kubernetes Secret when the value says so.
    Kind,
    /// `data`, `stringData`: a Secret's values.
    SecretValues,
    /// A private member of a JSON Web Key.
    Private,
    /// `oth`: RFC 7518's further primes, each of its members private.
    OtherPrimes,
}

fn role_of(name: &[u8], quoted: bool) -> Option<Role> {
    match name {
        b"kty" => Some(Role::KeyType),
        b"d" | b"p" | b"q" | b"dp" | b"dq" | b"qi" | b"k" | b"priv" => Some(Role::Private),
        b"oth" => Some(Role::OtherPrimes),
        // JSON spells these quoted; `kind: …` in code or prose is no manifest.
        b"kind" if quoted => Some(Role::Kind),
        b"data" | b"stringData" if quoted => Some(Role::SecretValues),
        _ => None,
    }
}

/// The spans an object's own members hold; they count once the object has closed
/// and its `kty` or `kind` is known, on either side of them.
#[derive(Default)]
pub(super) struct Found {
    pub(super) secret_values: Vec<Range<usize>>,
    pub(super) private_members: Vec<Range<usize>>,
}

struct Held {
    range: Range<usize>,
    secret_values: bool,
}

struct Frame {
    /// Where it opened.
    open: usize,
    /// Where its own entries start in [`Walk::held`]: frames close last-opened
    /// first, so all of them share one stack.
    held_from: usize,
    level: u32,
    /// The byte that closes it; `0` for the text outside every brace.
    closer: u8,
    /// It is the value of a member of the frame below, whose span it is.
    value_of: Option<Role>,
    key_type: bool,
    secret: bool,
}

impl Frame {
    fn outermost() -> Self {
        Self {
            closer: 0,
            level: 0,
            open: 0,
            value_of: None,
            key_type: false,
            secret: false,
            held_from: 0,
        }
    }
}

/// The token a piece ended inside, and what the walk knows of it.
enum Open {
    Nothing,
    /// Inside a string.
    String(OpenString),
    /// A string's closing quote was read: is it a member's name?
    StringEnded(OpenString),
    /// Inside a bare word.
    Name(OpenName),
    /// A bare word ended: is it a member's name?
    NameEnded(OpenName),
    /// A private member's name was read: what does its value look like?
    PrivateValue,
    /// Inside a comment before a member's value: a block one (`/* */`) ends
    /// after `star` and `/`, a line one at its newline.
    Comment {
        block: bool,
        star: bool,
    },
}

/// Bytes of the source the walk has in hand: `bytes[0]` is byte `base` of the
/// source, and `last` says none follow them.
struct Piece<'a> {
    bytes: &'a [u8],
    base: usize,
    last: bool,
}

/// A step's outcome: where the walk goes on, or the byte it must wait at for more.
type Step = Result<usize, usize>;

impl Piece<'_> {
    fn skip_space(&self, mut at: usize) -> usize {
        while matches!(self.bytes.get(at), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            at += 1;
        }
        at
    }
}

pub(super) struct Walk {
    frames: Vec<Frame>,
    held: Vec<Held>,
    found: Found,
    /// How deeply the current text is held in strings: a quote at level `n` has
    /// `2^n - 1` backslashes before it.
    level: u32,
    objects: usize,
    arrays: usize,
    /// The role of the member whose name was just read.
    expected: Option<Role>,
    open: Open,
    /// The bytes after [`Self::taken`] that wait for what decides their token.
    carry: Vec<u8>,
    /// Source bytes consumed.
    taken: usize,
    /// Bytes its open frames may grow to: past it the walk gives up before it
    /// allocates, not after.
    budget: usize,
    gave_up: bool,
}

/// Every span the objects of `text` say is secret.
pub(super) fn walk(text: &str) -> Found {
    let mut walk = Walk::new();
    walk.run(text.as_bytes(), true);
    walk.close_all();
    walk.found
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'$') || byte >= 0x80
}

fn is_value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'/' | b'-')
}

/// Whether `value` has the shape of a private member and does not read as a code
/// name (`d: privateScalarEncoded`).
fn is_private_value(value: &[u8]) -> bool {
    let body = value.strip_suffix(b"==").or(value.strip_suffix(b"="));
    let body = body.unwrap_or(value);
    body.len() >= 16
        && body.iter().copied().all(is_value_byte)
        && !std::str::from_utf8(value).is_ok_and(is_identifier_name)
}

impl Walk {
    pub(super) fn new() -> Self {
        Self {
            frames: vec![Frame::outermost()],
            held: Vec::new(),
            found: Found::default(),
            level: 0,
            objects: 0,
            arrays: 0,
            expected: None,
            open: Open::Nothing,
            carry: Vec::new(),
            taken: 0,
            budget: usize::MAX,
            gave_up: false,
        }
    }

    /// A walk that stops before its frames outgrow `budget` bytes.
    pub(super) fn within(budget: usize) -> Self {
        Self {
            budget,
            ..Self::new()
        }
    }

    /// Take the source's next `bytes`.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        if self.gave_up {
            return;
        }
        if self.carry.is_empty() {
            let taken = self.run(bytes, false);
            self.carry.extend_from_slice(&bytes[taken..]);
        } else {
            let mut held = std::mem::take(&mut self.carry);
            held.extend_from_slice(bytes);
            let taken = self.run(&held, false);
            held.drain(..taken);
            self.carry = held;
        }
        if self.carry.len() > CARRY_BYTES {
            self.give_up();
        }
    }

    /// Stop: this source is more than the walk can keep.
    pub(super) fn give_up(&mut self) {
        self.gave_up = true;
        self.frames = Vec::new();
        self.held = Vec::new();
        self.found = Found::default();
        self.carry = Vec::new();
    }

    /// About how many bytes the walk holds.
    pub(super) fn weight(&self) -> usize {
        self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.held.capacity() * std::mem::size_of::<Held>()
            + (self.found.secret_values.capacity() + self.found.private_members.capacity())
                * std::mem::size_of::<Range<usize>>()
            + self.carry.capacity()
    }

    /// The spans of the whole source, or `None` when the walk gave up.
    pub(super) fn finish(mut self) -> Option<Found> {
        if self.gave_up {
            return None;
        }
        let held = std::mem::take(&mut self.carry);
        self.run(&held, true);
        self.close_all();
        Some(self.found)
    }

    /// Walk `bytes`, as far as they decide: how many were consumed.
    fn run(&mut self, bytes: &[u8], last: bool) -> usize {
        let piece = Piece {
            bytes,
            base: self.taken,
            last,
        };
        let mut at = 0;
        while at < bytes.len() || (last && !matches!(self.open, Open::Nothing)) {
            match self.advance(&piece, at) {
                Ok(next) => at = next,
                Err(wait) => {
                    at = wait;
                    break;
                }
            }
        }
        let taken = at.min(bytes.len());
        self.taken += taken;
        taken
    }

    fn advance(&mut self, piece: &Piece<'_>, at: usize) -> Step {
        if matches!(self.open, Open::Nothing) {
            return self.step(piece, at);
        }
        match std::mem::replace(&mut self.open, Open::Nothing) {
            Open::Nothing => Ok(at),
            Open::String(string) => self.in_string(piece, at, string),
            Open::StringEnded(string) => self.string_ended(piece, at, string),
            Open::Name(name) => self.in_name(piece, at, name),
            Open::NameEnded(name) => self.name_ended(piece, at, name),
            Open::PrivateValue => self.private_value(piece, at),
            Open::Comment { block, star } => self.in_comment(piece, at, block, star),
        }
    }

    fn step(&mut self, piece: &Piece<'_>, at: usize) -> Step {
        let byte = piece.bytes[at];
        match byte {
            b' ' | b'\t' | b'\r' => Ok(at + 1),
            // No string spans a raw line, so nothing is held in one past it.
            b'\n' => {
                self.unwind_to(0, piece.base + at);
                Ok(at + 1)
            }
            b'"' | b'\\' => self.quote(piece, at),
            b'/' | b'#' if self.expected.is_some() => self.comment(piece, at),
            b'{' | b'[' => {
                self.open_frame(byte, piece.base + at);
                Ok(at + 1)
            }
            b'}' | b']' => {
                self.close(byte, piece.base + at);
                Ok(at + 1)
            }
            _ if is_name_byte(byte) => self.in_name(piece, at, OpenName::default()),
            _ => {
                self.expected = None;
                Ok(at + 1)
            }
        }
    }

    fn top(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
    }

    /// A member with `role`, whose value starts after the colon before `after_colon`.
    fn member(&mut self, piece: &Piece<'_>, role: Option<Role>, after_colon: usize) -> Step {
        self.expected = None;
        match role {
            None => Ok(after_colon),
            Some(Role::KeyType) => {
                if let Some(top) = self.top() {
                    top.key_type = true;
                }
                Ok(after_colon)
            }
            Some(Role::Private) => self.private_value(piece, after_colon),
            Some(role) => {
                self.expected = Some(role);
                Ok(after_colon)
            }
        }
    }

    /// A private member's value: quoted, whose string then says, or bare or
    /// single-quoted, as YAML and source code write one.
    fn private_value(&mut self, piece: &Piece<'_>, after_colon: usize) -> Step {
        let value = piece.skip_space(after_colon);
        if value >= piece.bytes.len() && !piece.last {
            self.open = Open::PrivateValue;
            return Err(after_colon);
        }
        if matches!(piece.bytes.get(value), Some(b'"' | b'\\')) {
            self.expected = Some(Role::Private);
            return Ok(value);
        }
        match unquoted_value(piece, value) {
            Err(()) => {
                self.open = Open::PrivateValue;
                Err(after_colon)
            }
            Ok(Some(range)) => {
                self.hold(piece.base + range.start..piece.base + range.end, false);
                Ok(range.end)
            }
            Ok(None) => Ok(after_colon),
        }
    }

    /// A comment (`//`, `/* */`, `#`) between a member's name and its value,
    /// as JSONC, source code and YAML write one: read past, so the value keeps
    /// the role its name gave it. Any other `/` ends the wait for the value.
    fn comment(&mut self, piece: &Piece<'_>, at: usize) -> Step {
        let block = match (piece.bytes[at], piece.bytes.get(at + 1)) {
            (b'#', _) => false,
            (_, Some(b'/')) => false,
            (_, Some(b'*')) => true,
            (_, None) if !piece.last => return Err(at),
            _ => {
                self.expected = None;
                return Ok(at + 1);
            }
        };
        let body = if piece.bytes[at] == b'#' {
            at + 1
        } else {
            at + 2
        };
        self.in_comment(piece, body, block, false)
    }

    /// The rest of a comment from `at`; one that runs past the piece goes on
    /// in the next.
    fn in_comment(
        &mut self,
        piece: &Piece<'_>,
        mut at: usize,
        block: bool,
        mut star: bool,
    ) -> Step {
        while let Some(&byte) = piece.bytes.get(at) {
            if !block && byte == b'\n' {
                return Ok(at);
            }
            at += 1;
            if block && star && byte == b'/' {
                return Ok(at);
            }
            star = byte == b'*';
        }
        if !piece.last {
            self.open = Open::Comment { block, star };
        }
        Ok(at)
    }
}

/// A private member's bare or single-quoted value at `at`, as offsets in `piece`:
/// what ends it must end a value. `Err` while the piece does not show its end.
fn unquoted_value(piece: &Piece<'_>, at: usize) -> Result<Option<Range<usize>>, ()> {
    let bytes = piece.bytes;
    let start = at + usize::from(bytes.get(at) == Some(&b'\''));
    let run = bytes[start.min(bytes.len())..]
        .iter()
        .take_while(|byte| is_value_byte(**byte))
        .count();
    let padding = bytes[(start + run).min(bytes.len())..]
        .iter()
        .take(2)
        .take_while(|byte| **byte == b'=')
        .count();
    let end = start + run + padding;
    if end >= bytes.len() && !piece.last {
        return Err(());
    }
    let ends = matches!(
        bytes.get(end),
        None | Some(b'\'' | b'"' | b'\\' | b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}')
    );
    Ok(
        (ends && is_private_value(&bytes[start.min(bytes.len())..end.min(bytes.len())]))
            .then_some(start..end),
    )
}
