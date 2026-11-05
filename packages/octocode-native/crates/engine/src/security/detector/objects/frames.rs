//! The nesting a walk is inside: frames that open and close, and the spans each
//! holds until it closes and says what they are.
use super::{Frame, Held, Role, Walk};
use std::ops::Range;

impl Walk {
    pub(super) fn hold(&mut self, range: Range<usize>, secret_values: bool) {
        self.held.push(Held {
            range,
            secret_values,
        });
    }

    pub(super) fn open_frame(&mut self, byte: u8, at: usize) {
        let (closer, count) = if byte == b'{' {
            (b'}', &mut self.objects)
        } else {
            (b']', &mut self.arrays)
        };
        *count += 1;
        let frame_bytes = std::mem::size_of::<Frame>();
        if self.frames.len() == self.frames.capacity()
            && self
                .frames
                .capacity()
                .saturating_mul(2)
                .saturating_mul(frame_bytes)
                > self.budget
        {
            self.give_up();
        }
        let value_of = self.expected.take().filter(|role| {
            matches!(
                (role, byte),
                (Role::SecretValues, b'{') | (Role::OtherPrimes, b'[')
            )
        });
        self.frames.push(Frame {
            closer,
            level: self.level,
            open: at,
            value_of,
            key_type: false,
            secret: false,
            held_from: self.held.len(),
        });
    }

    pub(super) fn close(&mut self, byte: u8, at: usize) {
        self.expected = None;
        let open = if byte == b'}' {
            self.objects
        } else {
            self.arrays
        };
        if open == 0 {
            // What came before ended in an object this text does not show.
            if byte == b'}' {
                while !self.frames.is_empty() {
                    self.close_frame(at);
                }
                self.frames.push(Frame {
                    held_from: self.held.len(),
                    ..Frame::outermost()
                });
            }
            return;
        }
        while let Some(top) = self.frames.last() {
            let closes = top.closer == byte;
            self.close_frame(at);
            if closes {
                return;
            }
        }
    }

    /// Close every frame held deeper than `level`, and leave that level.
    pub(super) fn unwind_to(&mut self, level: u32, at: usize) {
        while self
            .frames
            .last()
            .is_some_and(|top| top.level > level && top.closer != 0)
        {
            self.close_frame(at);
        }
        self.level = level;
    }

    pub(super) fn close_frame(&mut self, at: usize) {
        let Some(frame) = self.frames.pop() else {
            return;
        };
        match frame.closer {
            b'}' => self.objects -= 1,
            b']' => self.arrays -= 1,
            _ => {}
        }
        for held in self.held.drain(frame.held_from..) {
            if held.secret_values && frame.secret {
                self.found.secret_values.push(held.range);
            } else if !held.secret_values && frame.key_type {
                self.found.private_members.push(held.range);
            }
        }
        match frame.value_of {
            Some(Role::SecretValues) => self.hold(frame.open + 1..at, true),
            Some(Role::OtherPrimes) => self.hold(frame.open + 1..at, false),
            _ => {}
        }
    }

    /// The text ended: whatever is still open counts as closed there.
    pub(super) fn close_all(&mut self) {
        while !self.frames.is_empty() {
            self.close_frame(self.taken);
        }
    }
}
