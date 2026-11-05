//! Double-quoted strings in a walk: opaque, except an object held escaped inside
//! one. A quote's backslashes say how deeply it is held. A string of any length
//! passes with only what its end can ask of it kept.
use super::{Open, Piece, Role, Step, Walk, is_private_value, is_value_byte, role_of};

/// Bytes of a string that can still name a role or a kind.
const NAME_BYTES: usize = 16;
/// Bytes of a value still read for whether it is a code name: longer ones are not.
const CODE_NAME_BYTES: usize = 256;

/// A string so far.
pub(super) struct OpenString {
    /// Its first and past-its-last byte in the source.
    start: usize,
    end: usize,
    head: [u8; NAME_BYTES],
    len: usize,
    /// Its value is asked about: a private member's.
    value: Option<Box<ValueShape>>,
}

/// What [`is_private_value`] asks of a value, kept as it passes.
#[derive(Default)]
struct ValueShape {
    /// Its first bytes.
    head: Vec<u8>,
    /// Value bytes before any padding.
    run: usize,
    padding: usize,
    broken: bool,
}

impl OpenString {
    fn new(start: usize, is_value: bool) -> Self {
        Self {
            start,
            end: start,
            head: [0; NAME_BYTES],
            len: 0,
            value: is_value.then(Box::default),
        }
    }

    fn take(&mut self, bytes: &[u8]) {
        if let Some(room) = self.head.get_mut(self.len..) {
            let copied = room.len().min(bytes.len());
            room[..copied].copy_from_slice(&bytes[..copied]);
        }
        self.len += bytes.len();
        if let Some(shape) = &mut self.value
            && !shape.broken
        {
            let room = CODE_NAME_BYTES
                .saturating_sub(shape.head.len())
                .min(bytes.len());
            shape.head.extend_from_slice(&bytes[..room]);
            for &byte in bytes {
                if byte == b'=' {
                    shape.padding += 1;
                } else if is_value_byte(byte) && shape.padding == 0 {
                    shape.run += 1;
                } else {
                    shape.broken = true;
                    break;
                }
            }
        }
    }

    /// The whole string, when it is short enough to name a role.
    fn text(&self) -> Option<&[u8]> {
        self.head.get(..self.len)
    }

    /// Whether it has the shape of a private member's value.
    fn is_private(&self) -> bool {
        let Some(shape) = &self.value else {
            return false;
        };
        if shape.broken || shape.padding > 2 {
            return false;
        }
        // Past what is kept it is too long to read as a code name.
        if self.len > shape.head.len() {
            return shape.run >= 16;
        }
        is_private_value(&shape.head)
    }
}

/// The backslashes at `at`, and the quote after them with its level: `Err` while
/// `piece` does not show what follows them.
fn quote_token(piece: &Piece<'_>, at: usize) -> Result<(usize, Option<u32>), usize> {
    let run = piece.bytes[at..]
        .iter()
        .take_while(|byte| **byte == b'\\')
        .count();
    if at + run >= piece.bytes.len() && !piece.last {
        return Err(at);
    }
    let level = (piece.bytes.get(at + run) == Some(&b'"')).then(|| run.trailing_ones());
    Ok((at + run, level))
}

impl Walk {
    pub(super) fn quote(&mut self, piece: &Piece<'_>, at: usize) -> Step {
        let (after_run, level) = quote_token(piece, at)?;
        let Some(level) = level else {
            // An escape such as `\n` in a held object's text.
            return Ok(after_run + 1);
        };
        if level < self.level {
            // It closes the string this text was held in.
            self.unwind_to(level, piece.base + at);
            self.expected = None;
            return Ok(after_run + 1);
        }
        self.level = level;
        let start = piece.base + after_run + 1;
        self.open = Open::String(OpenString::new(start, self.expected == Some(Role::Private)));
        Ok(after_run + 1)
    }

    pub(super) fn in_string(
        &mut self,
        piece: &Piece<'_>,
        from: usize,
        mut string: OpenString,
    ) -> Step {
        let bytes = piece.bytes;
        let mut at = from;
        loop {
            let Some(found) = bytes
                .get(at..)
                .unwrap_or_default()
                .iter()
                .position(|byte| matches!(byte, b'\n' | b'{' | b'"' | b'\\'))
            else {
                let rest = bytes.get(at..).unwrap_or_default();
                string.take(rest);
                if piece.last {
                    self.expected = None;
                } else {
                    self.open = Open::String(string);
                }
                return Ok(bytes.len());
            };
            string.take(&bytes[at..at + found]);
            at += found;
            match bytes[at] {
                b'\n' => {
                    self.unwind_to(0, piece.base + at);
                    self.expected = None;
                    return Ok(at);
                }
                b'{' => match self.holds_an_object(piece, at) {
                    Err(()) => {
                        self.open = Open::String(string);
                        return Err(at);
                    }
                    Ok(true) => {
                        self.level += 1;
                        self.expected = None;
                        return Ok(at);
                    }
                    Ok(false) => {
                        string.take(&bytes[at..=at]);
                        at += 1;
                    }
                },
                _ => {
                    let token = match quote_token(piece, at) {
                        Ok(token) => token,
                        Err(wait) => {
                            self.open = Open::String(string);
                            return Err(wait);
                        }
                    };
                    match token {
                        (after_run, Some(level)) if level < self.level => {
                            self.unwind_to(level, piece.base + at);
                            self.expected = None;
                            return Ok(after_run + 1);
                        }
                        (after_run, Some(level)) if level == self.level => {
                            string.end = piece.base + at;
                            return self.string_ended(piece, after_run + 1, string);
                        }
                        (after_run, _) => {
                            let past = (after_run + 1).min(bytes.len());
                            string.take(&bytes[at..past]);
                            at = after_run + 1;
                        }
                    }
                }
            }
        }
    }

    /// Whether the `{` at `at`, inside a string, opens an object held escaped
    /// in it: what follows is a name quoted one level deeper.
    fn holds_an_object(&self, piece: &Piece<'_>, at: usize) -> Result<bool, ()> {
        let mut next = at + 1;
        loop {
            match piece.bytes.get(next) {
                Some(b' ' | b'\t' | b'\r') => next += 1,
                Some(b'\\') => {
                    let (after_run, level) = quote_token(piece, next).map_err(drop)?;
                    match level {
                        Some(level) => return Ok(level == self.level + 1),
                        None => next = after_run + 1,
                    }
                }
                None if !piece.last => return Err(()),
                _ => return Ok(false),
            }
        }
    }

    /// A string's closing quote was read: a colon after it makes it a member's
    /// name, and otherwise it is a value, which the member before it may ask about.
    pub(super) fn string_ended(
        &mut self,
        piece: &Piece<'_>,
        after: usize,
        string: OpenString,
    ) -> Step {
        let bytes = piece.bytes;
        let colon = piece.skip_space(after);
        let shown = |at: usize| at < bytes.len() || piece.last;
        if !shown(colon) || (bytes.get(colon) == Some(&b':') && !shown(colon + 1)) {
            self.open = Open::StringEnded(string);
            return Err(after);
        }
        if bytes.get(colon) == Some(&b':') && bytes.get(colon + 1) != Some(&b':') {
            let role = string.text().and_then(|name| role_of(name, true));
            return self.member(piece, role, colon + 1);
        }
        match self.expected.take() {
            Some(Role::Kind) if string.text() == Some(b"Secret") => {
                if let Some(top) = self.top() {
                    top.secret = true;
                }
            }
            Some(Role::Private) if string.is_private() => {
                self.hold(string.start..string.end, false);
            }
            _ => {}
        }
        Ok(after)
    }
}
