//! Bare words in a walk: what a run of name bytes ends in says whether it named a
//! member. A word of any length passes with only its head kept: no role's name is
//! long.
use super::{Open, Piece, Step, Walk, is_name_byte, role_of};

/// Bytes of a word that can still name a role.
const NAME_BYTES: usize = 16;

/// A bare word so far.
#[derive(Default)]
pub(super) struct OpenName {
    head: [u8; NAME_BYTES],
    len: usize,
}

impl OpenName {
    fn take(&mut self, bytes: &[u8]) {
        if let Some(room) = self.head.get_mut(self.len..) {
            let copied = room.len().min(bytes.len());
            room[..copied].copy_from_slice(&bytes[..copied]);
        }
        self.len = self.len.saturating_add(bytes.len());
    }

    /// The word, when it is short enough to name a role.
    fn name(&self) -> Option<&[u8]> {
        self.head.get(..self.len)
    }
}

impl Walk {
    pub(super) fn in_name(&mut self, piece: &Piece<'_>, at: usize, mut name: OpenName) -> Step {
        let run = piece.bytes[at..]
            .iter()
            .take_while(|byte| is_name_byte(**byte))
            .count();
        name.take(&piece.bytes[at..at + run]);
        let end = at + run;
        if end == piece.bytes.len() && !piece.last {
            self.open = Open::Name(name);
            return Ok(end);
        }
        self.name_ended(piece, end, name)
    }

    /// The word before `end` is a member's name when a colon follows it, past an
    /// optional closing quote and any space. Otherwise only its bytes were read.
    pub(super) fn name_ended(&mut self, piece: &Piece<'_>, end: usize, name: OpenName) -> Step {
        let bytes = piece.bytes;
        let closed = end + usize::from(bytes.get(end) == Some(&b'\''));
        let colon = piece.skip_space(closed);
        let shown = |at: usize| at < bytes.len() || piece.last;
        if !shown(closed) || !shown(colon) || (bytes.get(colon) == Some(&b':') && !shown(colon + 1))
        {
            self.open = Open::NameEnded(name);
            return Err(end);
        }
        if bytes.get(colon) != Some(&b':') || bytes.get(colon + 1) == Some(&b':') {
            self.expected = None;
            return Ok(end);
        }
        let role = name.name().and_then(|name| role_of(name, false));
        self.member(piece, role, colon + 1)
    }
}
