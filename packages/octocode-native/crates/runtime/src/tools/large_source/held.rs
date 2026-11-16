//! The requested line windows a streamed source keeps as it passes once.

/// The window a test's [`Want`] holds: the window every build streams.
#[cfg(test)]
pub(super) const WINDOW_BYTES: usize = crate::security::SourceLimits::REAL.window;

/// The lines one streamed window asks for.
#[derive(Clone, Copy)]
pub(super) struct Want {
    pub(super) first: usize,
    pub(super) last: usize,
    /// Bytes of those lines' sealed view the window skips before it holds
    /// any: a byte page's own offset.
    pub(super) from: usize,
    /// Bytes the window holds at most.
    pub(super) room: usize,
}

#[cfg(test)]
impl Want {
    pub(super) fn lines(first: usize, last: usize) -> Self {
        Self {
            first,
            last,
            from: 0,
            room: WINDOW_BYTES,
        }
    }

    /// A byte page of lines `first..=last` whose window starts at `from`.
    pub(super) fn bytes(first: usize, last: usize, from: usize) -> Self {
        Self {
            first,
            last,
            from,
            room: WINDOW_BYTES,
        }
    }
}

/// Where a window that holds bytes starts.
#[derive(Clone, Copy)]
pub(super) struct WindowStart {
    /// The requested lines' byte the window starts at.
    pub(super) base: usize,
    /// The first line the window holds bytes of.
    pub(super) line: usize,
    /// Bytes of that line before the window's first byte: a window a byte page
    /// cut inside a line holds only the rest of it.
    pub(super) lead: usize,
}

/// A stretch of a window that is one run of source bytes, or text of ours.
struct Run {
    /// Where it starts in the window.
    at: usize,
    /// The source byte it starts at; `None` for a key block's placeholder.
    source: Option<usize>,
}

/// The requested lines of one window kept while a source streams past:
/// byte-capped, with every line inside a private-key block replaced. The
/// bytes seen are counted whole, so the view's size is known whatever the
/// window holds.
pub(super) struct Held {
    pub(super) first: usize,
    last: usize,
    /// Bytes of the requested lines still to skip before the window starts
    /// (moved on to the next character boundary), keyed: their sealed count
    /// placed on the source once its first byte is known ([`SealMap::keyed`]).
    from: usize,
    /// The want's offset in the sealed view.
    sealed_from: usize,
    /// Source bytes of the requested lines seen so far.
    range: Option<std::ops::Range<usize>>,
    /// The source byte the window starts at (a key placeholder's line start).
    window_source: Option<usize>,
    /// The source byte the current line starts at.
    line_source: usize,
    /// The window's lines its seal redacted, with their placeholders.
    pub(super) redacted: Vec<(usize, usize)>,
    /// Bytes `window` holds at most.
    room: usize,
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
    pub(super) full: bool,
    /// How the window maps onto the source: a key block's placeholder breaks it,
    /// so there is one run unless the window holds one.
    runs: Vec<Run>,
    /// The source byte the last run reaches.
    run_end: Option<usize>,
    /// A sealed offset of this window could not be placed on the source's
    /// bytes: sealing it fails the read.
    unplaced: bool,
}

impl Held {
    fn new(want: Want) -> Self {
        Self {
            first: want.first,
            last: want.last,
            from: want.from,
            sealed_from: want.from,
            range: None,
            window_source: None,
            line_source: 0,
            redacted: Vec::new(),
            room: want.room,
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
            runs: Vec::new(),
            run_end: None,
            unplaced: false,
        }
    }

    /// Take one piece of `line` (up to and including its newline, if any), which
    /// starts at byte `source` of the source; `map` places a sealed offset.
    fn push(&mut self, piece: &[u8], line: usize, source: usize, map: Option<&SealMap>) {
        if !(self.first..=self.last).contains(&line) {
            return;
        }
        let range = self.range.get_or_insert_with(|| {
            if let Some(map) = map {
                match map.keyed(source, self.sealed_from) {
                    Some(from) => self.from = from,
                    None => self.unplaced = true,
                }
            }
            source..source
        });
        range.end = source + piece.len();
        if self.view_pos == self.line_view_start {
            self.line_source = source;
        }
        let start = self.view_pos;
        self.view_pos += piece.len();
        self.hold(start, piece, line, Some(source));
    }

    /// Keep `bytes`, which sit at `start` of the requested lines' bytes and at
    /// byte `source` of the source, from the window's first byte on.
    fn hold(&mut self, start: usize, bytes: &[u8], line: usize, source: Option<usize>) {
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
            self.window_source = Some(source.map_or(self.line_source, |source| source + skip));
            self.start = Some(WindowStart {
                base: start + skip,
                line,
                lead: (start + skip).saturating_sub(self.line_view_start),
            });
        }
        let piece = &bytes[skip..];
        let source = source.map(|source| source + skip);
        if source.is_none() || source != self.run_end {
            self.runs.push(Run {
                at: self.window.len(),
                source,
            });
        }
        self.run_end = source.map(|source| source + piece.len());
        let room = self.room.saturating_sub(self.window.len());
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

    /// Close `line`, replacing it when it sits `inside` a private-key block, all
    /// but `lead`, its bytes before the key, and `trail`, its bytes after it.
    fn finish(&mut self, line: usize, inside: bool, lead: &[u8], trail: &[u8]) {
        if (self.first..=self.last).contains(&line) {
            if inside {
                // The view counts the placeholder, not the key's bytes; a raw
                // line that overflowed the window may still fit as one.
                let placeholder = format!(
                    "{}{}{}\n",
                    String::from_utf8_lossy(lead),
                    crate::security::key_fragment_placeholder(),
                    String::from_utf8_lossy(trail)
                );
                self.view_pos = self.line_view_start + placeholder.len();
                self.overflow = false;
                if !self.full || self.cut_line == Some(line) {
                    self.window.truncate(self.line_start);
                    self.runs.retain(|run| run.at < self.line_start);
                    if self.window.is_empty() {
                        self.start = None;
                    }
                    if self.cut_line == Some(line) {
                        self.cut_line = None;
                        self.full = false;
                    }
                    self.hold(self.line_view_start, placeholder.as_bytes(), line, None);
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

mod lines;
mod seal;
pub(super) use lines::{InLine, WindowLines};
pub(super) use seal::SealMap;

#[cfg(test)]
mod tests;
