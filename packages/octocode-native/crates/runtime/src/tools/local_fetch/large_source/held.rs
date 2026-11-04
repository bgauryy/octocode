//! The requested line windows a streamed source keeps as it passes once.

use super::{MARKER_PREFIX_BYTES, WINDOW_BYTES};
use crate::security::{KeyBlockState, LineShapeScan, MarkerScan};

/// The lines one streamed window asks for.
#[derive(Clone, Copy)]
pub(super) struct Want {
    pub(super) first: usize,
    pub(super) last: usize,
    /// Bytes of those lines the window skips before it holds any: a byte
    /// page starts its window behind its own offset ([`super::BYTE_LOOKBEHIND`]).
    pub(super) from: usize,
}

#[cfg(test)]
impl Want {
    pub(super) fn lines(first: usize, last: usize) -> Self {
        Self {
            first,
            last,
            from: 0,
        }
    }

    /// A byte page of lines `first..=last` whose window starts at `from`.
    pub(super) fn bytes(first: usize, last: usize, from: usize) -> Self {
        Self { first, last, from }
    }
}

/// Where a window that holds bytes starts.
#[derive(Clone, Copy)]
pub(super) struct WindowStart {
    /// The requested lines' byte the window starts at.
    pub(super) base: usize,
    /// The first line the window holds bytes of.
    pub(super) line: usize,
}

/// The requested lines of one window kept while a source streams past:
/// byte-capped, with every line inside a private-key block replaced. The
/// bytes seen are counted whole, so the view's size is known whatever the
/// window holds.
pub(super) struct Held {
    pub(super) first: usize,
    last: usize,
    /// Bytes of the requested lines still to skip before the window starts
    /// (moved on to the next character boundary).
    from: usize,
    pub(super) window: Vec<u8>,
    /// Where `window` starts; `None` while it holds no byte.
    pub(super) start: Option<WindowStart>,
    /// Bytes of the requested lines seen so far, key blocks replaced.
    pub(super) view_pos: usize,
    /// `view_pos` at the current line's start.
    line_view_start: usize,
    /// Offset of the current line in `window`.
    line_start: usize,
    pub(super) held_last: usize,
    pub(super) key_lines: usize,
    /// The line the window is cut inside: it is the window's only line.
    cut_line: Option<usize>,
    /// The current line did not fit behind the lines held: its bytes are
    /// dropped as they pass, until [`Held::finish`] knows whether a key
    /// block replaces it (a placeholder may fit where its raw bytes did not).
    overflow: bool,
    full: bool,
}

impl Held {
    fn new(want: Want) -> Self {
        Self {
            first: want.first,
            last: want.last,
            from: want.from,
            window: Vec::new(),
            start: None,
            view_pos: 0,
            line_view_start: 0,
            line_start: 0,
            held_last: want.first.saturating_sub(1),
            key_lines: 0,
            cut_line: None,
            overflow: false,
            full: false,
        }
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    fn push(&mut self, piece: &[u8], line: usize) {
        if !(self.first..=self.last).contains(&line) {
            return;
        }
        let start = self.view_pos;
        self.view_pos += piece.len();
        self.hold(start, piece, line);
    }

    /// Keep `bytes`, which sit at `start` of the requested lines' bytes,
    /// from the window's first byte on.
    fn hold(&mut self, start: usize, bytes: &[u8], line: usize) {
        if self.full || self.overflow {
            return;
        }
        let mut skip = self.from.saturating_sub(start);
        if self.window.is_empty() && self.from > 0 {
            // The window starts on a whole character: a continuation byte
            // belongs to the character before it.
            while bytes
                .get(skip)
                .is_some_and(|byte| byte & 0b1100_0000 == 0b1000_0000)
            {
                skip += 1;
            }
        }
        if skip >= bytes.len() {
            self.from = start + skip;
            return;
        }
        if self.window.is_empty() {
            self.start = Some(WindowStart {
                base: start + skip,
                line,
            });
        }
        let piece = &bytes[skip..];
        let room = WINDOW_BYTES.saturating_sub(self.window.len());
        if piece.len() <= room {
            self.window.extend_from_slice(piece);
            return;
        }
        self.window.extend_from_slice(&piece[..room]);
        if self.line_start == 0 {
            // One line larger than the whole window: keep its head; the
            // rest of it is a later byte window.
            self.cut_line = Some(line);
            if let Err(error) = std::str::from_utf8(&self.window)
                && error.error_len().is_none()
            {
                self.window.truncate(error.valid_up_to());
            }
            self.full = true;
        } else {
            // Drop the partial line; the window ends before it, unless a key
            // block replaces the line when it closes.
            self.window.truncate(self.line_start);
            self.overflow = true;
        }
    }

    /// Close `line`, replacing it when it sits `inside` a private-key block.
    fn finish(&mut self, line: usize, inside: bool) {
        if (self.first..=self.last).contains(&line) {
            if inside {
                // The view counts the placeholder, not the key's bytes; a raw
                // line that overflowed the window may still fit as one.
                let placeholder = format!("{}\n", crate::security::key_fragment_placeholder());
                self.view_pos = self.line_view_start + placeholder.len();
                self.overflow = false;
                if !self.full || self.cut_line == Some(line) {
                    self.window.truncate(self.line_start);
                    if self.window.is_empty() {
                        self.start = None;
                    }
                    if self.cut_line == Some(line) {
                        self.cut_line = None;
                        self.full = false;
                    }
                    self.hold(self.line_view_start, placeholder.as_bytes(), line);
                    self.key_lines += usize::from(!self.overflow);
                }
            }
            if !self.full && !self.overflow {
                self.held_last = line;
            }
        }
        if self.overflow {
            // Not replaced (or not even as a placeholder): the line stays out
            // and ends the window.
            self.overflow = false;
            self.full = true;
        }
        if self.cut_line == Some(line) {
            self.held_last = line;
        }
        self.line_start = self.window.len();
        self.line_view_start = self.view_pos;
    }
}

/// The requested windows kept while a source streams past once, with the
/// private-key block state over the whole source, whichever side of a
/// window a block's markers fall on.
pub(super) struct WindowLines {
    pub(super) held: Vec<Held>,
    /// The current line's leading bytes, where a key's named lines and armor
    /// headers are read.
    head: Vec<u8>,
    /// The private-key markers of the current line, over all of it.
    markers: MarkerScan,
    /// Private-key block state over the lines closed so far.
    pub(super) block: KeyBlockState,
    /// The current line's shape, scanned only while a block is open.
    shape: LineShapeScan,
}

impl WindowLines {
    /// The windows of `wants`, streamed from a line start where the
    /// private-key block state is `block`.
    pub(super) fn new(wants: &[Want], block: KeyBlockState) -> Self {
        Self {
            held: wants.iter().map(|&want| Held::new(want)).collect(),
            head: Vec::new(),
            markers: MarkerScan::default(),
            block,
            shape: LineShapeScan::default(),
        }
    }

    /// Continue at a later line start where the block state is `block`.
    pub(super) fn restart(&mut self, block: KeyBlockState) {
        self.block = block;
        self.head.clear();
        self.markers = MarkerScan::default();
        self.shape = LineShapeScan::default();
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    pub(super) fn push(&mut self, piece: &[u8], line: usize) {
        if self.head.len() < MARKER_PREFIX_BYTES {
            let take = (MARKER_PREFIX_BYTES - self.head.len()).min(piece.len());
            self.head.extend_from_slice(&piece[..take]);
        }
        self.markers.feed(piece);
        if self.block.is_open() {
            self.shape.feed(piece);
        }
        for held in &mut self.held {
            held.push(piece, line);
        }
    }

    /// Close `line`: track the private-key block's markers, and replace the
    /// line in every window it sits in when it is inside a block.
    pub(super) fn finish(&mut self, line: usize) {
        let markers = self.markers.finish(&self.head);
        let (head, shape) = (&self.head, self.shape);
        let inside = self.block.advance(markers, || shape.finish(head));
        for held in &mut self.held {
            held.finish(line, inside);
        }
        self.head.clear();
        self.markers = MarkerScan::default();
        self.shape = LineShapeScan::default();
    }
}

#[cfg(test)]
mod tests;
