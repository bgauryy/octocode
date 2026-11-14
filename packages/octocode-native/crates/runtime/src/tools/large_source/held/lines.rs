//! What a pass over a streamed source holds of its requested windows, and the
//! private-key block state over all of it.
use super::super::MARKER_PREFIX_BYTES;
use super::super::edit_map::EditMap;
use super::super::error::ReadError;
use super::{Held, SealMap, Want};
use crate::security::{KeyBlockState, LineShapeScan, MarkerScan};

/// The requested windows kept while a source streams past once, with the
/// private-key block state over the whole source, whichever side of a
/// window a block's markers fall on.
pub(in crate::tools::large_source) struct WindowLines {
    pub(in crate::tools::large_source) held: Vec<Held>,
    /// The current line's leading bytes, where a key's named lines and armor
    /// headers are read.
    head: Vec<u8>,
    /// The private-key markers of the current line, over all of it.
    markers: MarkerScan,
    /// The current line's bytes while it is no longer than a key's line
    /// (`KEY_LINE_BYTES`) and read from its start, so an armor line shows
    /// what a whole-file read shows of it ([`crate::security::key_line_bytes`]).
    line: Vec<u8>,
    line_whole: bool,
    /// Private-key block state over the lines closed so far.
    pub(in crate::tools::large_source) block: KeyBlockState,
    /// The current line's shape, scanned only while a block is open.
    shape: LineShapeScan,
    /// The source byte the next piece starts at.
    at: usize,
    /// The source byte the current line starts at.
    line_at: usize,
    /// The bytes of the placeholder line that replaced the last closed line,
    /// when a private-key block replaced it.
    pub(in crate::tools::large_source) replaced: Option<usize>,
    /// What an earlier pass recorded: a byte page's sealed offset is placed on
    /// the source with it. `None` in the pass that records it, whose windows
    /// all start at their lines' first byte.
    seal: Option<SealMap>,
}

/// Where a pass was inside one long line, so a later read can start there.
#[derive(Clone)]
pub(in crate::tools::large_source) struct InLine {
    pub(in crate::tools::large_source) offset: usize,
    pub(in crate::tools::large_source) line: usize,
    pub(in crate::tools::large_source) line_at: usize,
    head: Vec<u8>,
    markers: MarkerScan,
    block: KeyBlockState,
    shape: LineShapeScan,
}

impl InLine {
    /// About how many bytes this holds.
    pub(in crate::tools::large_source) fn weight(&self) -> usize {
        std::mem::size_of::<Self>() + self.head.capacity()
    }
}

impl WindowLines {
    /// The windows of `wants`, streamed from a line start where the
    /// private-key block state is `block`.
    pub(in crate::tools::large_source) fn new(wants: &[Want], block: KeyBlockState) -> Self {
        Self {
            held: wants.iter().map(|&want| Held::new(want)).collect(),
            head: Vec::new(),
            markers: MarkerScan::default(),
            line: Vec::new(),
            line_whole: true,
            block,
            shape: LineShapeScan::default(),
            at: 0,
            line_at: 0,
            replaced: None,
            seal: None,
        }
    }

    /// These windows, read after a pass recorded `map`.
    pub(in crate::tools::large_source) fn sealed_by(mut self, map: SealMap) -> Self {
        self.seal = Some(map);
        self
    }

    /// Where this pass is, inside `line`.
    pub(in crate::tools::large_source) fn in_line(&self, line: usize) -> InLine {
        InLine {
            offset: self.at,
            line,
            line_at: self.line_at,
            head: self.head.clone(),
            markers: self.markers,
            block: self.block,
            shape: self.shape,
        }
    }

    /// Continue from where `point` was, its line's bytes before it all skipped: the
    /// only window starts past them, and they are all of its view so far.
    pub(in crate::tools::large_source) fn resume(&mut self, point: &InLine) {
        self.at = point.offset;
        self.line_at = point.line_at;
        self.head.clone_from(&point.head);
        self.line.clear();
        self.line_whole = false;
        self.markers = point.markers;
        self.block = point.block;
        self.shape = point.shape;
        for held in &mut self.held {
            held.view_pos = point.offset - point.line_at;
            held.range = Some(point.line_at..point.offset);
            held.line_source = point.line_at;
            if let Some(map) = &self.seal {
                match map.keyed(point.line_at, held.sealed_from) {
                    Some(from) => held.from = from,
                    None => held.unplaced = true,
                }
            }
        }
    }

    /// End the windows inside a `line` of `bytes` bytes that the pass found to be no
    /// key's: the rest of it is known, so it need not be read.
    pub(in crate::tools::large_source) fn close_known(&mut self, line: usize, bytes: usize) {
        for held in &mut self.held {
            held.view_pos = bytes;
            if let Some(range) = &mut held.range {
                range.end = self.line_at + bytes;
            }
            held.finish(line, false, &[]);
        }
        self.head.clear();
        self.line.clear();
        self.line_whole = true;
    }

    /// Whether no window can take more bytes.
    pub(in crate::tools::large_source) fn windows_full(&self) -> bool {
        self.held.iter().all(|held| held.full)
    }

    /// Seal every window with `edits`.
    pub(in crate::tools::large_source) fn seal(
        &mut self,
        edits: &EditMap,
    ) -> Result<(), ReadError> {
        for held in &mut self.held {
            held.seal(edits)?;
        }
        Ok(())
    }

    /// Continue at byte `offset`, a later line start where the block state is
    /// `block`.
    pub(in crate::tools::large_source) fn restart(&mut self, block: KeyBlockState, offset: u64) {
        self.at = octocode_engine::num::saturating_usize(offset);
        self.line_at = self.at;
        self.block = block;
        self.head.clear();
        self.line.clear();
        self.line_whole = true;
        self.markers = MarkerScan::default();
        self.shape = LineShapeScan::default();
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    pub(in crate::tools::large_source) fn push(&mut self, piece: &[u8], line: usize) {
        if self.head.len() < MARKER_PREFIX_BYTES {
            let take = (MARKER_PREFIX_BYTES - self.head.len()).min(piece.len());
            self.head.extend_from_slice(&piece[..take]);
        }
        if self.line_whole {
            if self.line.len() + piece.len() <= super::super::KEY_LINE_BYTES {
                self.line.extend_from_slice(piece);
            } else {
                self.line.clear();
                self.line_whole = false;
            }
        }
        self.markers.feed(piece);
        if self.block.is_open() {
            self.shape.feed(piece);
        }
        for held in &mut self.held {
            held.push(piece, line, self.at, self.seal.as_ref());
        }
        self.at += piece.len();
    }

    /// Close `line`: track the private-key block's markers, and replace the
    /// line in every window it sits in when it is inside a block: whether it did.
    /// A line that only spells a marker somewhere along great length is no key's
    /// own, so its bytes alone judge it.
    pub(in crate::tools::large_source) fn finish(&mut self, line: usize) -> bool {
        let markers = self.markers.finish(&self.head);
        let (head, shape) = (&self.head, self.shape);
        let in_block = self.block.advance(markers, || shape.finish(head))
            && (!markers.any() || self.at - self.line_at <= super::super::KEY_LINE_BYTES);
        // An armor line read whole shows its armor: only key text written
        // beside it is hidden, from where that starts to the line's end (a
        // window's line is its lead and a placeholder). One the pass entered
        // midway is hidden from its lead.
        let whole = (markers.any() && self.line_whole)
            .then(|| std::str::from_utf8(&self.line).ok())
            .flatten()
            .map(|line| line.trim_end_matches(['\n', '\r']));
        let key = whole.map(crate::security::key_line_bytes);
        let inside = in_block && key.as_ref().is_none_or(Option::is_some);
        let lead = match (key.flatten(), whole) {
            (Some(key), Some(line)) if inside => &line.as_bytes()[..key.start],
            _ if inside => crate::security::key_line_lead(&self.head, &shape),
            _ => &[],
        };
        self.replaced = inside.then(|| {
            String::from_utf8_lossy(lead).len()
                + crate::security::key_fragment_placeholder().len()
                + 1
        });
        for held in &mut self.held {
            held.finish(line, inside, lead);
        }
        self.line_at = self.at;
        self.head.clear();
        self.line.clear();
        self.line_whole = true;
        self.markers = MarkerScan::default();
        self.shape = LineShapeScan::default();
        inside
    }
}
