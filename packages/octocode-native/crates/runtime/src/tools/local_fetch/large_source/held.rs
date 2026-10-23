//! The requested line windows a streamed source keeps as it passes once.

use super::index::{VIEW_POINT_BYTES, ViewPoint};
use super::{MARKER_PREFIX_BYTES, WINDOW_BYTES};
use crate::security::{BlockLine, KeyBlockState, LineShape, LineShapeScan, is_key_header_prefix};

/// The lines one streamed window asks for.
#[derive(Clone, Copy)]
pub(super) struct Want {
    pub(super) first: usize,
    pub(super) last: usize,
    /// Bytes of those lines the window skips before it holds any: a byte
    /// page starts its window behind its own offset ([`BYTE_LOOKBEHIND`]).
    pub(super) from: usize,
    /// A byte page: later pages of the same lines resume inside them
    /// ([`RangeView`]).
    pub(super) byte_page: bool,
}

#[cfg(test)]
impl Want {
    pub(super) fn lines(first: usize, last: usize) -> Self {
        Self {
            first,
            last,
            from: 0,
            byte_page: false,
        }
    }

    /// A byte page of lines `first..=last` whose window starts at `from`.
    pub(super) fn bytes(first: usize, last: usize, from: usize) -> Self {
        Self {
            first,
            last,
            from,
            byte_page: true,
        }
    }
}

fn is_marker(head: &[u8], edge: &str) -> bool {
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    text.contains(edge) && text.contains("PRIVATE KEY")
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
    /// Requested lines inside a private-key block, held or not.
    pub(super) inside_lines: usize,
    /// The line the window is cut inside: it is the window's only line.
    cut_line: Option<usize>,
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
            inside_lines: 0,
            cut_line: None,
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
        if self.full {
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
        } else {
            // Drop the partial line; the window ends before it.
            self.window.truncate(self.line_start);
        }
        self.full = true;
    }

    /// Close `line`, replacing it when it sits `inside` a private-key block.
    fn finish(&mut self, line: usize, inside: bool) {
        let held = self.full && self.cut_line != Some(line);
        if (self.first..=self.last).contains(&line) {
            if inside {
                self.inside_lines += 1;
                // The view counts the placeholder, not the key's bytes.
                let placeholder = format!("{}\n", crate::security::key_fragment_placeholder());
                self.view_pos = self.line_view_start + placeholder.len();
                if !held {
                    self.window.truncate(self.line_start);
                    if self.window.is_empty() {
                        self.start = None;
                    }
                    if self.cut_line == Some(line) {
                        self.cut_line = None;
                        self.full = false;
                    }
                    self.hold(self.line_view_start, placeholder.as_bytes(), line);
                    self.key_lines += 1;
                }
            }
            if !held {
                self.held_last = line;
            }
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
    /// The current line's leading bytes, for private-key marker detection.
    head: Vec<u8>,
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
            block,
            shape: LineShapeScan::default(),
        }
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    pub(super) fn push(&mut self, piece: &[u8], line: usize) {
        if self.head.len() < MARKER_PREFIX_BYTES {
            let take = (MARKER_PREFIX_BYTES - self.head.len()).min(piece.len());
            self.head.extend_from_slice(&piece[..take]);
        }
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
        let begins = is_marker(&self.head, "-----BEGIN ");
        let ends = is_marker(&self.head, "-----END ");
        let (head, shape) = (&self.head, self.shape);
        let inside = self.block.advance(begins, ends, || match shape.finish() {
            LineShape::Other if is_key_header_prefix(head) => BlockLine::Header,
            shape => BlockLine::Shape(shape),
        });
        for held in &mut self.held {
            held.finish(line, inside);
        }
        self.head.clear();
        self.shape = LineShapeScan::default();
    }

    /// Whether every window is full: no later byte adds to one.
    pub(super) fn full(&self) -> bool {
        self.held.iter().all(|held| held.full)
    }

    /// Close full windows of a range no key block reaches without reading
    /// on: the lines past them change no window, only the view's size,
    /// `total`, and a cut window's line, which closes as its only line.
    pub(super) fn settle(&mut self, total: usize) {
        for held in &mut self.held {
            held.view_pos = total;
            if let Some(line) = held.cut_line {
                held.held_last = line;
            }
        }
    }

    /// The window's state before the piece of `line` at source `offset`, as
    /// a [`ViewPoint`] of its range, when it is [`VIEW_POINT_BYTES`] past
    /// the last one.
    pub(super) fn mark(&self, points: &mut Vec<ViewPoint>, line: usize, offset: u64) {
        let [held] = self.held.as_slice() else {
            return;
        };
        if !(held.first..=held.last).contains(&line)
            || points
                .last()
                .is_some_and(|point| held.view_pos < point.view + VIEW_POINT_BYTES)
        {
            return;
        }
        points.push(ViewPoint {
            view: held.view_pos,
            offset,
            line,
            line_view_start: held.line_view_start,
            head: self.head.clone(),
            block: self.block,
            shape: self.shape,
            inside_lines: held.inside_lines,
        });
    }

    /// The window of `want` resumed at `point`: the state a pass from the
    /// range's start leaves there while its window is still empty.
    pub(super) fn resumed(want: Want, point: &ViewPoint) -> Self {
        let mut lines = Self::new(&[want], point.block);
        lines.head.clone_from(&point.head);
        lines.shape = point.shape;
        for held in &mut lines.held {
            held.view_pos = point.view;
            held.line_view_start = point.line_view_start;
            held.held_last = point.line - 1;
            // Before the window every replaced line counts.
            held.key_lines = point.inside_lines;
        }
        lines
    }
}
