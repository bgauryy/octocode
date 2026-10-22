//! Bounded line windows over sources above the in-memory ceiling.
//!
//! `localFetch` normally reads the whole source (≤ `MAX_SOURCE_BYTES`) so it
//! can hash it, count lines, and redact private-key blocks across the entire
//! file before cutting a window. A larger source (logs, dumps, generated
//! code) is instead streamed once: the whole-file SHA-256 and line count are
//! computed on the fly, only the requested line window is kept (byte-capped,
//! so even one multi-GB line cannot allocate unbounded), and every window
//! line inside a private-key block is replaced before the window reaches the
//! regular sanitizer, whichever side of the window the block's markers fall
//! on. The window then goes through the normal pipeline, and its positions,
//! totals, digest, and continuations are mapped back to the whole file.

use super::executor::process_fetched_content;
use super::types::*;
use crate::execution::cancel::CancellationCheck;
use crate::num::usize_of;
use crate::security::scan::ContentScan;
use crate::security::{BlockLine, KeyBlockState, LineShape, LineShapeScan, is_key_header_prefix};
use std::path::Path;

/// Largest source a window may be streamed from.
pub(super) const MAX_STREAM_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
/// Lines held by one streamed window; response pages are sized separately.
const WINDOW_LINES: usize = 5_000;
/// Lines the first bare read previews before continuing through bounded
/// windows (contract `localFetch.semanticPolicy.streamedHeadLines`).
const STREAMED_HEAD_LINES: usize = crate::id::local_fetch_policy::STREAMED_HEAD_LINES;
/// Bytes one streamed window holds; a longer window is cut at a line end or,
/// for a single giant line, mid-line: its byte pages start the window at
/// their own offset, so the rest of the line is the next window.
pub(super) const WINDOW_BYTES: usize = 4 * 1024 * 1024;
/// View bytes a byte page's window keeps behind the page: the page finds its
/// line's start and its secret-scan margin there, as in an in-memory read.
const BYTE_LOOKBEHIND: usize = 1024 * 1024;
/// Line prefix kept for private-key marker detection outside the window.
const MARKER_PREFIX_BYTES: usize = 256;

struct Streamed {
    digest: String,
    total_lines: usize,
    window: Vec<u8>,
    /// First and last (1-based) lines `window` holds bytes of.
    first: usize,
    last: usize,
    key_lines: usize,
    /// Where `window` starts in the requested lines' bytes, and how many
    /// bytes those lines have (key blocks replaced): the byte view a
    /// window of any size is a part of.
    origin: ByteOrigin,
    /// The source holds a NUL byte: binary, whatever its head showed.
    binary: bool,
}

/// The lines one streamed window asks for.
#[derive(Clone, Copy)]
struct Want {
    first: usize,
    last: usize,
    /// Bytes of those lines the window skips before it holds any: a byte
    /// page starts its window behind its own offset ([`BYTE_LOOKBEHIND`]).
    from: usize,
    /// A byte page: later pages of the same lines resume inside them
    /// ([`RangeView`]).
    byte_page: bool,
}

#[cfg(test)]
impl Want {
    fn lines(first: usize, last: usize) -> Self {
        Self {
            first,
            last,
            from: 0,
            byte_page: false,
        }
    }

    /// A byte page of lines `first..=last` whose window starts at `from`.
    fn bytes(first: usize, last: usize, from: usize) -> Self {
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
struct WindowStart {
    /// The requested lines' byte the window starts at.
    base: usize,
    /// The first line the window holds bytes of.
    line: usize,
}

/// The requested lines of one window kept while a source streams past:
/// byte-capped, with every line inside a private-key block replaced. The
/// bytes seen are counted whole, so the view's size is known whatever the
/// window holds.
struct Held {
    first: usize,
    last: usize,
    /// Bytes of the requested lines still to skip before the window starts
    /// (moved on to the next character boundary).
    from: usize,
    window: Vec<u8>,
    /// Where `window` starts; `None` while it holds no byte.
    start: Option<WindowStart>,
    /// Bytes of the requested lines seen so far, key blocks replaced.
    view_pos: usize,
    /// `view_pos` at the current line's start.
    line_view_start: usize,
    /// Offset of the current line in `window`.
    line_start: usize,
    held_last: usize,
    key_lines: usize,
    /// Requested lines inside a private-key block, held or not.
    inside_lines: usize,
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
struct WindowLines {
    held: Vec<Held>,
    /// The current line's leading bytes, for private-key marker detection.
    head: Vec<u8>,
    /// Private-key block state over the lines closed so far.
    block: KeyBlockState,
    /// The current line's shape, scanned only while a block is open.
    shape: LineShapeScan,
}

impl WindowLines {
    /// The windows of `wants`, streamed from a line start where the
    /// private-key block state is `block`.
    fn new(wants: &[Want], block: KeyBlockState) -> Self {
        Self {
            held: wants.iter().map(|&want| Held::new(want)).collect(),
            head: Vec::new(),
            block,
            shape: LineShapeScan::default(),
        }
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    fn push(&mut self, piece: &[u8], line: usize) {
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
    fn finish(&mut self, line: usize) {
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
    fn full(&self) -> bool {
        self.held.iter().all(|held| held.full)
    }

    /// Close full windows of a range no key block reaches without reading
    /// on: the lines past them change no window, only the view's size,
    /// `total`, and a cut window's line, which closes as its only line.
    fn settle(&mut self, total: usize) {
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
    fn mark(&self, points: &mut Vec<ViewPoint>, line: usize, offset: u64) {
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
    fn resumed(want: Want, point: &ViewPoint) -> Self {
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

/// View bytes between two [`ViewPoint`]s: a later byte page of a range
/// streams about this much before its window.
const VIEW_POINT_BYTES: usize = 1024 * 1024;

/// A place inside one range's byte view where a later byte page resumes:
/// the source offset of a piece and the state the range's bytes before it
/// leave (line, private-key block, marker head and line shape).
#[derive(Clone, Debug)]
struct ViewPoint {
    /// Bytes of the range's view before the piece.
    view: usize,
    offset: u64,
    line: usize,
    /// `view` at `line`'s start.
    line_view_start: usize,
    /// `line`'s leading bytes before the piece.
    head: Vec<u8>,
    block: KeyBlockState,
    shape: LineShapeScan,
    /// Range lines inside a private-key block before the piece.
    inside_lines: usize,
}

/// One byte-paged range's view, learned on a pass that read all of it: its
/// size, whether a private-key block reaches it, and a [`ViewPoint`] about
/// every [`VIEW_POINT_BYTES`]. A later byte page of the range on the same
/// source version resumes at the point before its window and, when no key
/// block reaches the range, stops once its window is full.
#[derive(Debug)]
struct RangeView {
    stamp: octocode_engine::stamp::SourceStamp,
    total: usize,
    keyed: bool,
    points: Vec<ViewPoint>,
}

impl RangeView {
    /// The last point at or before view byte `from`.
    fn resume_at(&self, from: usize) -> Option<&ViewPoint> {
        let at = self.points.partition_point(|point| point.view <= from);
        self.points.get(at.checked_sub(1)?)
    }
}

/// Range views of streamed sources by canonical path and line range. The
/// byte weight bounds the memo; the entry cap only stops a flood of tiny
/// views (a view weighs about 350 KB per GB of range).
static RANGE_VIEWS: octocode_io::memo::Memo<(std::path::PathBuf, usize, usize), RangeView> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 256,
        max_weight: 4 * 1024 * 1024,
        keep_newest: false,
    });

/// The one byte page `wants` asks for, if that is all it asks for.
fn byte_page(wants: &[Want]) -> Option<Want> {
    match wants {
        [want] if want.byte_page => Some(*want),
        _ => None,
    }
}

/// Lines between two checkpoints of a [`StreamIndex`]: a later page streams
/// at most this many lines before its window.
const CHECKPOINT_LINES: usize = 4_096;

/// Where a line starts in the source, and the private-key block state the
/// lines before it leave.
#[derive(Clone, Copy, Debug)]
struct Checkpoint {
    line: usize,
    offset: u64,
    block: KeyBlockState,
}

/// What one whole pass over a streamed source learned: its digest, line
/// count, whether it holds a NUL byte, and a [`Checkpoint`] at every
/// [`CHECKPOINT_LINES`]-th line from line 1. While the file keeps the
/// settled stamp the pass was made under, a later page of the same version
/// streams from the checkpoint before its window instead of the whole file.
#[derive(Debug)]
struct StreamIndex {
    stamp: octocode_engine::stamp::SourceStamp,
    digest: String,
    total_lines: usize,
    binary: bool,
    checkpoints: Vec<Checkpoint>,
}

/// Indexes of streamed sources by canonical path. The stamp (size, times,
/// inode) recorded only for a file untouched for
/// [`octocode_engine::stamp::SourceStamp::SETTLE`] is the proof that the
/// bytes, and so the digest, are unchanged, as the graph and LSP memos use
/// it. A miss costs a whole-file pass, so the byte weight bounds the memo
/// and the entry cap sits far above a working set of large sources (an
/// index weighs about 80 KB per 10 million lines).
static INDEXES: octocode_io::memo::Memo<std::path::PathBuf, StreamIndex> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 1024,
        max_weight: 8 * 1024 * 1024,
        keep_newest: false,
    });

/// The streamed source as it is copied: every piece is split at its
/// newlines into [`WindowLines`], and a cancel ends the copy.
struct LineSink<'a, C: CancellationCheck> {
    lines: WindowLines,
    /// The 1-based line the next byte belongs to.
    line: usize,
    /// Bytes seen on the current (unterminated) line.
    pending: bool,
    /// Source offset of the next byte.
    offset: u64,
    /// The checkpoints a whole pass records ([`StreamIndex`]).
    checkpoints: Option<Vec<Checkpoint>>,
    /// The view points a byte page's first pass records ([`RangeView`]).
    points: Option<Vec<ViewPoint>>,
    binary: bool,
    cancel: &'a C,
}

impl<C: CancellationCheck> std::io::Write for LineSink<'_, C> {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.binary |= chunk.contains(&0);
        let mut rest = chunk;
        while !rest.is_empty() {
            if !self.pending
                && (self.line - 1).is_multiple_of(CHECKPOINT_LINES)
                && let Some(points) = self.checkpoints.as_mut()
            {
                points.push(Checkpoint {
                    line: self.line,
                    offset: self.offset,
                    block: self.lines.block,
                });
            }
            let (piece, newline) = match rest.iter().position(|byte| *byte == b'\n') {
                Some(index) => (&rest[..=index], true),
                None => (rest, false),
            };
            if let Some(points) = self.points.as_mut() {
                self.lines.mark(points, self.line, self.offset);
            }
            rest = &rest[piece.len()..];
            self.offset += piece.len() as u64;
            self.pending = !newline;
            self.lines.push(piece, self.line);
            if newline {
                self.lines.finish(self.line);
                self.line += 1;
            }
        }
        Ok(chunk.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<C: CancellationCheck> LineSink<'_, C> {
    /// The view of the byte page's range once a pass read all of it.
    fn range_view(&mut self) -> Option<RangeView> {
        let points = self.points.take()?;
        let [held] = self.lines.held.as_slice() else {
            return None;
        };
        Some(RangeView {
            stamp: octocode_engine::stamp::SourceStamp::default(),
            total: held.view_pos,
            keyed: held.inside_lines > 0,
            points,
        })
    }

    /// The held windows once the stream ends, with the source's totals.
    fn windows(self, digest: &str, total_lines: usize, binary: bool) -> Vec<Streamed> {
        self.lines
            .held
            .into_iter()
            .map(|held| Streamed {
                digest: digest.to_owned(),
                total_lines,
                first: held.start.map_or(held.first, |start| start.line),
                last: held.held_last.min(total_lines),
                key_lines: held.key_lines,
                origin: ByteOrigin {
                    base: held.start.map_or(0, |start| start.base),
                    total: held.view_pos,
                },
                window: held.window,
                binary,
            })
            .collect()
    }
}

fn seek(mut file: &std::fs::File, offset: u64) -> Result<(), String> {
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(offset))
        .map(drop)
        .map_err(|error| error.to_string())
}

/// Stream the opened source once from its start: hash all bytes, count
/// lines like `line_count`, and keep lines `first..=last` (1-based) of every
/// requested window, from its byte `from` on. The handle is the one the read opened and stat'ed (never
/// a reopen by path, which a swapped path could turn into a FIFO).
fn stream(
    source: &std::fs::File,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<(Vec<Streamed>, StreamIndex, Option<RangeView>), String> {
    seek(source, 0)?;
    let mut sink = LineSink {
        lines: WindowLines::new(wants, KeyBlockState::default()),
        line: 1,
        pending: false,
        offset: 0,
        checkpoints: Some(Vec::new()),
        points: byte_page(wants).map(|_| Vec::new()),
        binary: false,
        cancel,
    };
    let (_, digest) = octocode_engine::digest::copy_hashed(source, &mut sink)
        .map_err(|error| error.to_string())?;
    if sink.pending {
        sink.lines.finish(sink.line);
    }
    let total_lines = if sink.pending {
        sink.line
    } else {
        sink.line - 1
    };
    let index = StreamIndex {
        stamp: octocode_engine::stamp::SourceStamp::default(),
        digest,
        total_lines,
        binary: sink.binary,
        checkpoints: sink.checkpoints.take().unwrap_or_default(),
    };
    let view = sink.range_view();
    Ok((
        sink.windows(&index.digest, total_lines, index.binary),
        index,
        view,
    ))
}

/// The windows of `wants` from the source `index` describes, with the
/// index's digest and totals (the source is not hashed again): streamed
/// from the checkpoint before the first window through the last, or, for
/// a byte page whose range `view` is known, from the view point before its
/// window, until the window is full when no key block reaches the range.
/// Returns the range's view when this pass learned it.
fn stream_indexed(
    source: &std::fs::File,
    wants: &[Want],
    cancel: &impl CancellationCheck,
    index: &StreamIndex,
    view: Option<&RangeView>,
) -> Result<(Vec<Streamed>, Option<RangeView>), String> {
    let first = wants.iter().map(|want| want.first).min().unwrap_or(1);
    let last = wants.iter().map(|want| want.last).max().unwrap_or(0);
    let page = byte_page(wants);
    let resume = page
        .zip(view)
        .and_then(|(want, view)| view.resume_at(want.from).map(|point| (want, point)));
    let mut sink = if let Some((want, point)) = resume {
        seek(source, point.offset)?;
        LineSink {
            lines: WindowLines::resumed(want, point),
            line: point.line,
            pending: point.view > point.line_view_start,
            offset: point.offset,
            checkpoints: None,
            points: None,
            binary: false,
            cancel,
        }
    } else {
        let at = index
            .checkpoints
            .partition_point(|point| point.line <= first)
            .saturating_sub(1);
        let point = index.checkpoints.get(at).copied().unwrap_or(Checkpoint {
            line: 1,
            offset: 0,
            block: KeyBlockState::default(),
        });
        seek(source, point.offset)?;
        LineSink {
            lines: WindowLines::new(wants, point.block),
            line: point.line,
            pending: false,
            offset: point.offset,
            checkpoints: None,
            points: page.filter(|_| view.is_none()).map(|_| Vec::new()),
            binary: false,
            cancel,
        }
    };
    // Past a full window of a range no key block reaches, no byte changes
    // the window: the view's size is known.
    let settled = view.filter(|view| page.is_some() && !view.keyed);
    let mut block = vec![0_u8; 64 * 1024];
    let mut reader = source;
    while sink.line <= last {
        if let Some(view) = settled
            && sink.lines.full()
        {
            sink.lines.settle(view.total);
            return Ok((
                sink.windows(&index.digest, index.total_lines, index.binary),
                None,
            ));
        }
        let read = match std::io::Read::read(&mut reader, &mut block) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        std::io::Write::write_all(&mut sink, &block[..read]).map_err(|error| error.to_string())?;
    }
    if sink.pending && sink.line <= last {
        sink.lines.finish(sink.line);
    }
    let learned = sink.range_view();
    Ok((
        sink.windows(&index.digest, index.total_lines, index.binary),
        learned,
    ))
}

/// Keep a byte page's range view for later pages of the source version
/// `stamp`.
fn remember_view(
    path: &Path,
    wants: &[Want],
    view: Option<RangeView>,
    stamp: octocode_engine::stamp::SourceStamp,
) {
    let (Some(want), Some(mut view)) = (byte_page(wants), view) else {
        return;
    };
    view.stamp = stamp;
    let weight = view.points.len() * (std::mem::size_of::<ViewPoint>() + MARKER_PREFIX_BYTES);
    RANGE_VIEWS.insert(
        (path.to_path_buf(), want.first, want.last),
        view,
        weight,
        None,
    );
}

/// The windows of `wants` from `source` at `path`: from the source's index
/// while `stamp` (the handle's settled stamp, taken before the read) still
/// matches it, else by one whole pass, which indexes a source whose stamp
/// held through the pass.
fn read_windows(
    source: &std::fs::File,
    path: &Path,
    stamp: Option<octocode_engine::stamp::SourceStamp>,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<Vec<Streamed>, String> {
    let unchanged = |stamp: &octocode_engine::stamp::SourceStamp| {
        source
            .metadata()
            .ok()
            .as_ref()
            .and_then(crate::tools::source::cache_stamp)
            == Some(*stamp)
    };
    if let Some(stamp) = stamp
        && let Some(hit) = INDEXES.get(path)
        && hit.value.stamp == stamp
    {
        let view = byte_page(wants)
            .and_then(|want| RANGE_VIEWS.get(&(path.to_path_buf(), want.first, want.last)))
            .filter(|view| view.value.stamp == stamp);
        let (windows, learned) = stream_indexed(
            source,
            wants,
            cancel,
            &hit.value,
            view.as_ref().map(|view| &*view.value),
        )?;
        if learned.is_some() && unchanged(&stamp) {
            remember_view(path, wants, learned, stamp);
        }
        return Ok(windows);
    }
    let (windows, mut index, view) = stream(source, wants, cancel)?;
    if let Some(stamp) = stamp.filter(|stamp| unchanged(stamp)) {
        index.stamp = stamp;
        let weight =
            index.checkpoints.len() * std::mem::size_of::<Checkpoint>() + index.digest.len();
        INDEXES.insert(path.to_path_buf(), index, weight, None);
        remember_view(path, wants, view, stamp);
    }
    Ok(windows)
}

/// A repair for a request a streamed window cannot serve.
fn unsupported(q: &LocalFetchQuery, len: u64, reason: &str, next: NextCalls) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "largeSourceWindowOnly",
        format!(
            "This source is {}MB, above the {}MB whole-file read limit, so only bounded line windows are served: {reason}",
            len / (1024 * 1024),
            super::acquire::MAX_SOURCE_BYTES / (1024 * 1024)
        ),
    );
    result.source_bytes = usize::try_from(len).ok();
    result.next = Some(next);
    result
}

fn line_chunk_query(q: &LocalFetchQuery, offset: usize) -> LocalFetchQuery {
    let mut query = q.clone();
    query.full_content = None;
    query.minify = None;
    query.match_string = None;
    query.regex = None;
    query.case_mode = None;
    query.context_lines = None;
    local_fetch_query::clear_block_selectors(&mut query);
    query.unit = Some(WindowUnit::Lines);
    query.offset = if offset == 0 {
        None
    } else {
        Some(wire_count(offset))
    };
    query.snapshot = None;
    query
}

/// Serve `q` from a source larger than the whole-file ceiling, streamed
/// from `source`, the read's open handle on `path`.
pub(super) fn fetch_window(
    q: &LocalFetchQuery,
    path: &Path,
    source: &std::fs::File,
    meta: &std::fs::Metadata,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let len = meta.len();
    // Taken before any byte is read: a write during the pass changes it.
    let stamp = crate::tools::source::cache_stamp(meta).filter(|_| {
        octocode_engine::stamp::SourceStamp::settled(meta, octocode_engine::stamp::settle_clock())
            .is_some()
    });
    let source = Opened {
        file: source,
        path,
        stamp,
        len,
    };
    if !local_fetch_query::match_strings(q).is_empty() {
        return match_needs_whole_file(q, len);
    }
    if q.full_content == Some(true)
        || local_fetch_query::minify_mode(q).is_some()
        || (q.unit == Some(WindowUnit::Bytes) && local_fetch_query::start_line(q).is_none())
        || local_fetch_query::block(q)
    {
        return unsupported(
            q,
            len,
            "fullContent, minify, block, and whole-source byte chunks need the whole file. Read line windows, or byte pages within one line range.",
            read_bounded(q),
        );
    }
    // A stream's first page carries its note: the page leaves it room.
    let noted = |noted: bool| {
        let note = crate::response::stream_page::json_text_chars(&streamed_warning(len)) + 4;
        share.map(|share| {
            if noted {
                share.saturating_sub(note).max(1)
            } else {
                share
            }
        })
    };
    if local_fetch_query::has_ranges(q) && local_fetch_query::start_line(q).is_none() {
        return fetch_ranges(q, &source, modified, noted(true), security, cancel);
    }
    let first_page = local_fetch_query::offset(q).unwrap_or(0) == 0;
    let mut result = stream_window(q, &source, modified, noted(first_page), security, cancel);
    // The note is the same on every page: the read's first page states it.
    if result.status != ReadStatus::Error && local_fetch_query::offset(q).unwrap_or(0) == 0 {
        result.warnings.insert(0, streamed_warning(len));
    }
    result
}

/// The note a streamed read's first page carries: the size past the
/// whole-file ceiling. The served lines are the row's own line anchors.
fn streamed_warning(len: u64) -> String {
    format!(
        "Large source ({}MB): served from streamed line windows.",
        len / (1024 * 1024)
    )
}

/// The file lines one streamed window of `q` holds.
#[derive(Clone, Copy)]
struct WindowSpan {
    first: usize,
    /// The window's last line: the request's, capped at [`WINDOW_LINES`]
    /// unless the window is a byte page, which pages the range's bytes.
    last: usize,
    /// The request's last line.
    requested_last: usize,
    /// The line a line `offset` of the read counts from: the range's first
    /// line, or line 1 for a chunk.
    origin: usize,
    /// A line chunk (offset + length, no `ranges`) rather than a range.
    chunked: bool,
    /// The range's bytes the window skips: a byte page's offset less its
    /// lookbehind ([`BYTE_LOOKBEHIND`]).
    from: usize,
    /// A byte page of a range: it reads at its byte offset, however many
    /// lines (or bytes of one line) come before it.
    byte_page: bool,
}

impl WindowSpan {
    fn want(&self) -> Want {
        Want {
            first: self.first,
            last: self.last,
            from: self.from,
            byte_page: self.byte_page,
        }
    }
}

fn window_span(q: &LocalFetchQuery) -> WindowSpan {
    let (first, requested_last, chunked) = match (
        local_fetch_query::start_line(q),
        local_fetch_query::end_line(q),
    ) {
        // A range pages in its own lines: a line offset starts the window
        // there, so every window holds a full page past the offset (a byte
        // offset pages inside the range's first line instead).
        (Some(start), end) => {
            let skip = match q.unit {
                Some(WindowUnit::Bytes) => 0,
                _ => local_fetch_query::offset(q).unwrap_or(0),
            };
            (start + skip, end.unwrap_or(start), false)
        }
        _ => {
            let offset = local_fetch_query::offset(q).unwrap_or(0);
            (
                offset + 1,
                offset
                    + local_fetch_query::window_length(q).unwrap_or(
                        if offset == 0 && q.snapshot.is_none() {
                            STREAMED_HEAD_LINES
                        } else {
                            WINDOW_LINES
                        },
                    ),
                true,
            )
        }
    };
    let byte_page = !chunked && q.unit == Some(WindowUnit::Bytes);
    WindowSpan {
        first,
        last: if byte_page {
            requested_last
        } else {
            requested_last.min(first + WINDOW_LINES - 1)
        },
        from: if byte_page {
            local_fetch_query::offset(q)
                .unwrap_or(0)
                .saturating_sub(BYTE_LOOKBEHIND)
        } else {
            0
        },
        requested_last,
        origin: if chunked {
            1
        } else {
            local_fetch_query::start_line(q).unwrap_or(first)
        },
        chunked,
        byte_page,
    }
}

/// The streamed source a read opened: its handle, canonical path, settled
/// stamp (see [`read_windows`]) and length.
struct Opened<'a> {
    file: &'a std::fs::File,
    path: &'a Path,
    stamp: Option<octocode_engine::stamp::SourceStamp>,
    len: u64,
}

impl Opened<'_> {
    fn windows(
        &self,
        wants: &[Want],
        cancel: &impl CancellationCheck,
    ) -> Result<Vec<Streamed>, String> {
        read_windows(self.file, self.path, self.stamp, wants, cancel)
    }
}

/// Room an omission marker between two joined spans takes.
const OMISSION_MARKER_CHARS: usize = 48;

/// One streamed line window (a range or a chunk).
fn stream_window(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let span = window_span(q);
    // One span streams exactly one window.
    let streamed = source
        .windows(&[span.want()], cancel)
        .and_then(|mut windows| windows.pop().ok_or_else(|| "no window streamed".to_owned()));
    match streamed {
        Ok(streamed) => serve_window(q, streamed, span, source, modified, share, security, cancel),
        Err(error) => LocalFetchResult::error(q.path.to_string(), "fileReadFailed", error),
    }
}

/// The page of one streamed window, mapped back to the whole file.
fn serve_window(
    q: &LocalFetchQuery,
    streamed: Streamed,
    span: WindowSpan,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let (path, len) = (source.path, source.len);
    let WindowSpan {
        first,
        requested_last,
        origin,
        chunked,
        byte_page,
        ..
    } = span;
    if let Some(expected) = q.snapshot.as_deref()
        && **expected != streamed.digest
    {
        return super::executor::stale_snapshot(q);
    }
    if streamed.binary {
        return super::acquire::refuse_binary(q, q.path.as_str());
    }
    let total = streamed.total_lines;
    if !chunked && origin > total {
        // A range starting past the file end, as an in-memory read refuses it.
        return super::prepare::extraction_error(
            q,
            ExtractionError::new(
                ExtractionErrorKind::InvalidInput,
                format!("Requested lines start past the file end ({total} lines)"),
            ),
            total,
        );
    }
    // A line offset past the read's last line (the range's, or the file's
    // for a chunk) is the in-memory read's out-of-range page.
    let read_total = if chunked {
        total
    } else {
        requested_last.min(total) + 1 - origin
    };
    if (!chunked && first > requested_last.min(total))
        || (chunked && (first > total.max(1) || (total == 0 && first > 1)))
    {
        return past_end_row(q, &streamed, WindowUnit::Lines, read_total, len);
    }
    // A byte offset past the range's last byte, which no window holds.
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    if byte_page && offset > 0 && offset >= streamed.origin.total {
        return past_end_row(q, &streamed, WindowUnit::Bytes, streamed.origin.total, len);
    }
    let held_last = streamed.last.max(streamed.first);
    let mut window_query = q.clone();
    // The outer streamed fetch rejects selection and exposes an unfiltered recovery.
    // This internal bounded read addresses only a temporary window digest.
    window_query.clasify_context = None;
    window_query.selection = None;
    local_fetch_query::set_line_span(&mut window_query, 1, held_last + 1 - streamed.first);
    window_query.snapshot = None;
    if chunked {
        // The chunk's file offset selected the window itself.
        window_query.offset = None;
        window_query.length = None;
        window_query.unit = None;
    } else if q.unit != Some(WindowUnit::Bytes) {
        // A range's line offset started the window; a byte offset pages
        // the range's bytes, which the window holds from `origin.base`
        // (`SourceFacts::byte_origin` places the page).
        window_query.offset = None;
    }
    let serve = |window_query: &LocalFetchQuery| {
        let mut result = process_fetched_content(
            window_query,
            &streamed.window,
            path,
            &super::types::SourceFacts {
                modified: modified.clone(),
                window: share,
                beside: 0,
                interior: streamed.first > 1 || streamed.origin.base > 0,
                byte_origin: Some(streamed.origin),
            },
            security,
            cancel,
        );
        if result.status == ReadStatus::Error {
            return result;
        }
        map_to_file(&mut result, &streamed, len);
        if !chunked && requested_last > total {
            // The in-memory read states the same clamp.
            result.warnings.push(format!(
                "Range {first}-{requested_last} adjusted to end at {total} (file end)."
            ));
        }
        // The read's last line: the file's for a chunk, the range's otherwise.
        let last = if chunked {
            total
        } else {
            requested_last.min(total)
        };
        // The lines the page returned, which the window's page budget may have
        // cut short of the window it held.
        let served = result
            .source_line_ranges
            .first()
            .map(|range| range.start)
            .zip(result.source_line_ranges.last().map(|range| range.end))
            .or(result.start_line.zip(result.end_line))
            .unwrap_or((streamed.first, held_last));
        continue_window(
            q,
            &mut result,
            &streamed,
            span,
            (served.1 < last).then_some(served.1 + 1 - origin),
        );
        // A line page pages the read's own lines (the file's for a chunk, the
        // range's otherwise) whatever window held it, as an in-memory read's
        // pagination does.
        if let Some(page) = result.pagination.as_mut()
            && page.unit == WindowUnit::Lines
        {
            let (start, end) = served;
            page.offset = start - origin;
            page.length = end + 1 - start;
            page.total_lines = last + 1 - origin;
            if chunked {
                page.total_bytes = usize_of(len);
            }
            page.has_more = end < last;
            page.next_offset = page.has_more.then_some(end + 1 - origin);
            page.resume_line = None;
        }
        // A byte page pages the bytes of the lines its window was cut from.
        if let Some(page) = result.pagination.as_mut()
            && page.unit == WindowUnit::Bytes
        {
            page.total_lines = span.last.min(total) + 1 - span.first;
        }
        result
    };
    let page = serve(&window_query);
    // A page whose row the file's coordinates grew past its share is cut
    // again from the window already held.
    let Some(lines) = share
        .filter(|_| page.status == ReadStatus::Success)
        .and_then(|share| super::executor::fitted_lines(&page, q, share))
    else {
        return page;
    };
    let mut shorter = window_query;
    shorter.unit = Some(WindowUnit::Lines);
    shorter.length = wire_positive(lines);
    let fitted = serve(&shorter);
    if fitted.status == ReadStatus::Error {
        page
    } else {
        fitted
    }
}

/// Several `ranges`: each (overlaps and neighbours merged, in line order) is
/// one streamed window read, joined with an omission marker between
/// non-adjacent spans, as a whole-file read joins them. The page ends at the
/// first span that does not fit one window page: it holds every span before
/// it and that span's first page, continued by that span's own pages
/// (`continueWindow`) and one read of every requested range after it
/// (`readOtherRanges`), so each requested line is on exactly one page.
fn fetch_ranges(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let len = source.len;
    let merged = crate::tools::line_spans::merge_spans(
        local_fetch_query::line_ranges(q)
            .into_iter()
            .map(|range| (range.start, range.end)),
    );
    let ones = merged
        .iter()
        .map(|&(start, end)| {
            let mut one = q.clone();
            local_fetch_query::set_line_span(&mut one, start, end);
            let span = window_span(&one);
            (one, span)
        })
        .collect::<Vec<_>>();
    // One pass over the file holds every range's window: the digest and
    // line count are the file's, not the range's.
    let wants = ones.iter().map(|(_, span)| span.want()).collect::<Vec<_>>();
    let streamed = match source.windows(&wants, cancel) {
        Ok(streamed) => streamed,
        Err(error) => return LocalFetchResult::error(q.path.to_string(), "fileReadFailed", error),
    };
    let mut parts = Vec::with_capacity(merged.len());
    // The span the page stops inside: its range and the calls that read on.
    let mut cut = None;
    // The row's share is spent by the first span's whole row and each later
    // span's text (with its omission marker); a later span is served with
    // the text room left plus the first row's other chars, which each part's
    // own row carries about as much of.
    let last_line = merged.last().map_or(0, |span| span.1);
    let mut text_left = share.map(|share| {
        share
            .saturating_sub(crate::response::stream_page::json_text_chars(&cut_note(
                last_line, last_line, true,
            )))
            .saturating_sub(4)
            .max(1)
    });
    let mut around = None;
    for (index, ((one, span), streamed)) in ones.into_iter().zip(streamed).enumerate() {
        let (start, end) = (span.first, span.requested_last);
        let part_share = text_left.map(|left| left + around.unwrap_or(0));
        let mut part = serve_window(
            &one,
            streamed,
            span,
            source,
            modified.clone(),
            part_share,
            security,
            cancel,
        );
        if part.status == ReadStatus::Error {
            return part;
        }
        if let Some(left) = text_left {
            let text = part.content.as_deref().map_or(0, |text| {
                crate::response::read_share::text_chars(text, part.total_lines.unwrap_or(0))
            });
            let row = serde_json::to_value(&part)
                .map_or(0, |row| crate::response::read_share::row_chars(&row));
            around.get_or_insert(row.saturating_sub(text));
            let spent = if index == 0 {
                row
            } else {
                text + OMISSION_MARKER_CHARS
            };
            text_left = Some(left.saturating_sub(spent).max(1));
        }
        if let Some(mut next) = part.next.take() {
            let after = &merged[index + 1..];
            next.read_other_ranges = (!after.is_empty()).then(|| {
                let mut rest = q.clone();
                local_fetch_query::set_line_spans(&mut rest, after);
                rest.snapshot = part.source_sha256.as_deref().and_then(|d| d.parse().ok());
                FetchCall { query: rest }
            });
            cut = Some((start, end, next));
            parts.push(part);
            break;
        }
        parts.push(part);
    }
    let mut parts = parts.into_iter();
    let Some(mut result) = parts.next() else {
        return unsupported(q, len, "no line range was requested.", read_bounded(q));
    };
    let served = |result: &LocalFetchResult| {
        result
            .source_line_ranges
            .first()
            .map(|range| range.start)
            .zip(result.source_line_ranges.last().map(|range| range.end))
            .or(result.start_line.zip(result.end_line))
    };
    let mut spans_served = served(&result).into_iter().collect::<Vec<_>>();
    for mut part in parts {
        let span = served(&part);
        let (Some(content), Some(next_content)) = (result.content.as_mut(), part.content.take())
        else {
            continue;
        };
        let previous_end = spans_served.last().map_or(0, |span| span.1);
        if let Some((start, _)) = span
            && start > previous_end + 1
        {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&super::extraction::omission_marker(
                previous_end + 1,
                start - 1,
            ));
        }
        content.push_str(&next_content);
        spans_served.extend(span);
        result.source_line_ranges.extend(part.source_line_ranges);
        for warning in part.warnings {
            if !result.warnings.contains(&warning) {
                result.warnings.push(warning);
            }
        }
        for hint in part.hints {
            if !result.hints.contains(&hint) {
                result.hints.push(hint);
            }
        }
        result.end_line = part.end_line.or(result.end_line);
        result.is_partial =
            (result.is_partial == Some(true) || part.is_partial == Some(true)).then_some(true);
        let add = |a: Option<usize>, b: Option<usize>| match (a, b) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
        result.returned_chars = add(result.returned_chars, part.returned_chars);
        result.returned_bytes = add(result.returned_bytes, part.returned_bytes);
        result.returned_lines = add(result.returned_lines, part.returned_lines);
    }
    // Each window held its lines; one line says it for all.
    result.warnings.insert(0, streamed_warning(len));
    result.pagination = None;
    if let Some((start, end, next)) = cut {
        result
            .warnings
            .push(cut_note(start, end, next.read_other_ranges.is_some()));
        result.is_partial = Some(true);
        result.next = Some(next);
    }
    result
}

/// The note of a ranges page that ends inside lines `start`-`end`, whose
/// later ranges (`after`) are read on their own.
fn cut_note(start: usize, end: usize, after: bool) -> String {
    let after = if after {
        "; the ranges after it are read on their own"
    } else {
        ""
    };
    format!(
        "Lines {start}-{end} need more than one window page: this page ends inside them and their later pages read that range alone{after}."
    )
}

/// A match read needs the whole file: lead to localSearch, which streams
/// it, and to bounded line windows.
fn match_needs_whole_file(q: &LocalFetchQuery, len: u64) -> LocalFetchResult {
    let needles = local_fetch_query::match_strings(q);
    let is_regex = local_fetch_query::is_regex(q);
    let engine = if local_fetch_query::is_pcre2(q) {
        "pcre2"
    } else {
        "rust"
    };
    // A list matches any entry: one alternation for localSearch.
    let (needle, is_regex) = match needles.as_slice() {
        [one] => ((*one).to_owned(), is_regex),
        many => (
            many.iter()
                .map(|n| {
                    if is_regex {
                        (*n).to_owned()
                    } else {
                        ::regex::escape(n)
                    }
                })
                .collect::<Vec<_>>()
                .join("|"),
            true,
        ),
    };
    let mut search = serde_json::json!({
        "path": q.path.to_string(),
        "matchString": needle,
        "regex": if is_regex { engine } else { "literal" },
    });
    if let Some(mode) = q.case_mode {
        search["caseMode"] = serde_json::json!(match mode {
            octocode_contract_types::ReadCaseMode::Sensitive => "sensitive",
            octocode_contract_types::ReadCaseMode::Smart => "smart",
            octocode_contract_types::ReadCaseMode::Insensitive => "insensitive",
        });
    }
    if local_fetch_query::whole_word(q) {
        search["wholeWord"] = serde_json::json!(true);
    }
    let mut next = read_bounded(q);
    next.text_search = Some(
        crate::response::result::Continuation::new(crate::id::ToolId::LocalSearch, search).build(),
    );
    unsupported(q, len, "matchString needs the whole file.", next)
}

/// Bounded line windows from the first line.
fn read_bounded(q: &LocalFetchQuery) -> NextCalls {
    NextCalls {
        read_bounded_lines: Some(FetchCall {
            query: line_chunk_query(q, 0),
        }),
        ..NextCalls::default()
    }
}

/// An offset past the end of the read's `read_total` `unit`s (lines, or the
/// bytes of its lines): nothing returned, in the shape an in-memory read
/// gives the same offset ([`super::executor::past_end`]), with the file's
/// own facts.
fn past_end_row(
    q: &LocalFetchQuery,
    streamed: &Streamed,
    unit: WindowUnit,
    read_total: usize,
    len: u64,
) -> LocalFetchResult {
    let mut warnings = Vec::new();
    let next = super::executor::past_end(q, read_total, unit, &mut warnings);
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    let (total_lines, total_bytes) = match unit {
        WindowUnit::Lines => (read_total, usize_of(len)),
        WindowUnit::Bytes => (streamed.last + 1 - streamed.first, read_total),
    };
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(streamed.digest.clone()),
        content: Some(String::new()),
        warnings,
        total_lines: Some(streamed.total_lines),
        source_bytes: usize::try_from(len).ok(),
        returned_chars: Some(0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        pagination: Some(Pagination {
            unit,
            offset,
            length: 0,
            window: local_fetch_query::window_length(q).unwrap_or(WINDOW_LINES),
            total_lines,
            total_bytes,
            has_more: false,
            next_offset: None,
            resume_line: None,
        }),
        out_of_range: true,
        next: Some(next),
        ..LocalFetchResult::blank(ReadStatus::Success)
    }
}

/// Map a window's positions, totals and digest back to the whole file.
fn map_to_file(result: &mut LocalFetchResult, streamed: &Streamed, len: u64) {
    let shift = streamed.first - 1;
    for range in &mut result.source_line_ranges {
        range.start += shift;
        range.end += shift;
    }
    result.start_line = result.source_line_ranges.first().map(|range| range.start);
    result.end_line = result.source_line_ranges.last().map(|range| range.end);
    let total = streamed.total_lines;
    result.total_lines = Some(total);
    result.source_bytes = usize::try_from(len).ok();
    result.source_chars = None;
    result.source_sha256 = Some(streamed.digest.clone());
    if streamed.key_lines > 0 {
        result.warnings.push(format!(
            "redactedContent: {} line(s) inside private-key blocks were replaced; this content is not verbatim source.",
            streamed.key_lines
        ));
    }
}

/// The read's next page. A line page continues the read itself at
/// `next_offset` (counted in the read's own lines: the file's for a
/// `chunked` read, the range's otherwise), so the next window starts where
/// the page ended and the offset never resets at a window boundary. A byte
/// page continues in the bytes of the lines its window was cut from (its
/// offset counts from their start, as an in-memory read's does).
fn continue_window(
    q: &LocalFetchQuery,
    result: &mut LocalFetchResult,
    streamed: &Streamed,
    span: WindowSpan,
    next_offset: Option<usize>,
) {
    let chunked = span.chunked;
    let snapshot = streamed.digest.parse().ok();
    let inner = result
        .next
        .as_mut()
        .and_then(|next| next.continue_window.as_mut());
    if let Some(inner) = inner
        && inner.query.unit == Some(WindowUnit::Bytes)
    {
        inner.query.path = q.path.clone();
        local_fetch_query::set_line_span(
            &mut inner.query,
            span.first,
            span.last.min(streamed.total_lines),
        );
        inner.query.snapshot = snapshot;
        return;
    }
    let read = next_offset.map(|offset| {
        let mut query = if chunked {
            line_chunk_query(q, offset)
        } else {
            let mut query = q.clone();
            if query.unit == Some(WindowUnit::Bytes) {
                // The long line is done: line pages resume after it.
                query.unit = None;
                query.length = None;
            }
            query.offset = Some(wire_count(offset));
            query
        };
        query.snapshot = snapshot;
        FetchCall { query }
    });
    match (result.next.as_mut(), read) {
        (Some(next), read) => next.continue_window = read,
        (None, Some(read)) => {
            result.next = Some(NextCalls {
                continue_window: Some(read),
                ..NextCalls::default()
            });
        }
        (None, None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use sha2::{Digest, Sha256};

    fn opened(path: &Path) -> std::fs::File {
        std::fs::File::open(path).expect("open")
    }

    /// One stream re-reads its handle from the start, wherever an earlier
    /// read left it.
    #[test]
    fn a_stream_reads_its_handle_from_the_start() {
        let file = temp(b"a\nb\n");
        let source = opened(file.path());
        std::io::Read::read(&mut &source, &mut [0_u8; 3]).expect("sample");
        let (windows, ..) = stream(&source, &[Want::lines(1, 2)], &NeverCancel).expect("stream");
        assert_eq!(windows[0].window, b"a\nb\n");
    }

    fn stream_one(path: &Path, first: usize, last: usize) -> Streamed {
        stream(&opened(path), &[Want::lines(first, last)], &NeverCancel)
            .expect("stream")
            .0
            .pop()
            .expect("window")
    }

    /// One pass holds every window, each with the file's digest and totals,
    /// and key-block lines replaced in each window they reach.
    #[test]
    fn one_stream_holds_every_requested_window() {
        let file = temp(
            b"a\n-----BEGIN RSA PRIVATE KEY-----\nSECRET\n-----END RSA PRIVATE KEY-----\nb\nc\n",
        );
        let (windows, ..) = stream(
            &opened(file.path()),
            &[Want::lines(1, 1), Want::lines(3, 3), Want::lines(5, 6)],
            &NeverCancel,
        )
        .expect("stream");
        let held = windows
            .iter()
            .map(|window| String::from_utf8(window.window.clone()).expect("utf8"))
            .collect::<Vec<_>>();
        assert_eq!(held[0], "a\n");
        assert!(!held[1].contains("SECRET"), "{}", held[1]);
        assert_eq!(held[2], "b\nc\n");
        assert_eq!(windows[1].key_lines, 1);
        assert!(
            windows
                .iter()
                .all(|window| window.total_lines == 6 && window.digest == windows[0].digest)
        );
        assert_eq!(
            windows
                .iter()
                .map(|w| (w.first, w.last))
                .collect::<Vec<_>>(),
            vec![(1, 1), (3, 3), (5, 6)]
        );
    }

    fn temp(content: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp");
        std::io::Write::write_all(&mut file, content).expect("write");
        file
    }

    /// A streamed source shows line 1 without its leading BOM, as an
    /// in-memory read does.
    #[test]
    fn a_streamed_source_hides_its_leading_bom() {
        let mut source = "\u{feff}alpha\nbeta\n".to_owned().into_bytes();
        source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
        let file = temp(&source);
        let query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":file.path(),"ranges":["1-2"]}))
                .expect("query");
        let read = fetch_window(
            &query,
            file.path(),
            &opened(file.path()),
            &std::fs::metadata(file.path()).expect("meta"),
            None,
            None,
            &crate::security::ContentSecurity::new(),
            &NeverCancel,
        );
        assert_eq!(read.content.as_deref(), Some("alpha\nbeta\n"), "{read:?}");
    }

    /// A streamed window that starts after line 1 keeps a U+FEFF its first
    /// line begins with: only the file's first bytes hold its byte-order
    /// mark.
    #[test]
    fn a_streamed_window_past_line_one_keeps_a_leading_feff() {
        let mut source = "alpha\nbeta\n\u{feff}gamma\n".to_owned().into_bytes();
        source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
        let file = temp(&source);
        let query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":file.path(),"ranges":["3-3"]}))
                .expect("query");
        let read = fetch_window(
            &query,
            file.path(),
            &opened(file.path()),
            &std::fs::metadata(file.path()).expect("meta"),
            None,
            None,
            &crate::security::ContentSecurity::new(),
            &NeverCancel,
        );
        assert_eq!(read.content.as_deref(), Some("\u{feff}gamma\n"), "{read:?}");
    }

    #[test]
    fn a_copied_streamed_head_continuation_fills_its_bounded_window() {
        let mut source = "x\n".repeat(1_200).into_bytes();
        source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
        let file = temp(&source);
        let query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":file.path()})).expect("query");
        let read = |query: &LocalFetchQuery| {
            fetch_window(
                query,
                file.path(),
                &opened(file.path()),
                &std::fs::metadata(file.path()).expect("meta"),
                None,
                None,
                &crate::security::ContentSecurity::new(),
                &NeverCancel,
            )
        };
        let first = read(&query);
        assert_eq!(first.content.as_deref(), Some("x\n".repeat(100).as_str()));
        let next = first
            .next
            .and_then(|next| next.continue_window)
            .expect("head continuation");
        crate::contracts::prepare_many_and_validate(
            "localFetch",
            serde_json::json!({"queries":[&next.query]}),
        )
        .expect("canonical continuation");
        let second = read(&next.query);
        assert_eq!(
            second.content.as_deref(),
            Some("x\n".repeat(1_100).as_str())
        );
        assert_eq!(second.start_line, Some(101));
        assert_eq!(second.end_line, Some(1_200));
        let rest = second
            .next
            .and_then(|next| next.continue_window)
            .expect("unread last line");
        assert_eq!(local_fetch_query::offset(&rest.query), Some(1_200));
        crate::contracts::prepare_many_and_validate(
            "localFetch",
            serde_json::json!({"queries":[&rest.query]}),
        )
        .expect("canonical unread boundary");
        let mut explicit = next.query;
        explicit.length = wire_positive(7);
        assert_eq!(read(&explicit).returned_lines, Some(7));
    }

    #[test]
    fn streams_hash_totals_and_window() {
        let file = temp(b"a\nb\nc\nd");
        let streamed = stream_one(file.path(), 2, 3);
        assert_eq!(streamed.total_lines, 4);
        assert_eq!(streamed.window, b"b\nc\n");
        assert_eq!((streamed.first, streamed.last), (2, 3));
        assert_eq!(streamed.digest, hex::encode(Sha256::digest(b"a\nb\nc\nd")));
    }

    /// A cancel ends the hashing copy with the cancel's own reason.
    #[test]
    fn a_cancelled_stream_stops_with_the_cancel_reason() {
        struct Cancelled;
        impl CancellationCheck for Cancelled {
            fn check(&self) -> Result<(), String> {
                Err("deadline reached".into())
            }
        }
        let file = temp(b"a\nb\n");
        assert_eq!(
            stream(&opened(file.path()), &[Want::lines(1, 2)], &Cancelled)
                .map(drop)
                .err()
                .as_deref(),
            Some("deadline reached")
        );
    }

    /// Lines split across copy blocks count once, and an unterminated last
    /// line is a line: the totals agree with the whole-text line count.
    #[test]
    fn lines_across_copy_blocks_count_like_the_whole_text() {
        let mut text = "x".repeat(70 * 1024);
        text.push('\n');
        text.push_str("tail without newline");
        let file = temp(text.as_bytes());
        let streamed = stream_one(file.path(), 2, 2);
        assert_eq!(streamed.total_lines, 2);
        assert_eq!(streamed.window, b"tail without newline");
        assert_eq!(
            streamed.digest,
            hex::encode(Sha256::digest(text.as_bytes()))
        );
    }

    #[test]
    fn key_block_lines_are_replaced_even_when_markers_are_outside_the_window() {
        let file = temp(b"x\n-----BEGIN RSA PRIVATE KEY-----\nSECRET1\nSECRET2\n-----END RSA PRIVATE KEY-----\ny\n");
        let streamed = stream_one(file.path(), 3, 4);
        let window = String::from_utf8(streamed.window).expect("utf8");
        assert!(!window.contains("SECRET"), "{window}");
        assert_eq!(streamed.key_lines, 2);
        let after = stream_one(file.path(), 6, 6);
        assert_eq!(after.window, b"y\n");
    }

    #[test]
    fn unterminated_key_redacts_its_body_run_and_leaves_later_code_readable() {
        let file = temp(
            b"let a = 1;\nconst PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet b = compute(a);\nfn main() {}\n-----BEGIN OPENSSH PRIVATE KEY-----\nSECRET1\nSECRET2\nfn tail() {}\n",
        );
        let streamed = stream_one(file.path(), 1, 8);
        let window = String::from_utf8(streamed.window).expect("utf8");
        assert!(
            !window.contains("SECRET") && !window.contains("BEGIN"),
            "{window}"
        );
        assert!(window.contains("let b = compute(a);"), "{window}");
        assert!(
            window.contains("fn main() {}") && window.contains("fn tail() {}"),
            "{window}"
        );
        assert_eq!(streamed.key_lines, 4);
    }

    /// Counts the copy blocks a stream writes (one check each).
    struct Blocks(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for Blocks {
        fn check(&self) -> Result<(), String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    /// A later page of an indexed source streams from the checkpoint before
    /// its window, not the whole file, and holds the same window: the
    /// digest and totals of the whole pass, and a private-key block open
    /// across the checkpoint still redacted.
    #[test]
    fn an_indexed_source_streams_a_later_window_from_its_checkpoint() {
        let mut text = String::new();
        for line in 1..=40_000 {
            match line {
                4_090 => text.push_str("-----BEGIN RSA PRIVATE KEY-----\n"),
                4_091..=4_110 => text.push_str("SECRETSECRETSECRET\n"),
                4_111 => text.push_str("-----END RSA PRIVATE KEY-----\n"),
                _ => text.push_str(&format!("line {line}\n")),
            }
        }
        let file = temp(text.as_bytes());
        let source = opened(file.path());
        let meta = source.metadata().expect("meta");
        let stamp = crate::tools::source::cache_stamp(&meta);
        assert!(stamp.is_some(), "the platform stamps files");
        let path = file.path().canonicalize().expect("canonical");
        let spans = [Want::lines(4_100, 4_120), Want::lines(39_990, 40_000)];
        let whole = Blocks(Default::default());
        let first = read_windows(&source, &path, stamp, &spans, &whole).expect("first");
        let indexed = Blocks(Default::default());
        let again = read_windows(&source, &path, stamp, &spans, &indexed).expect("again");
        let full = whole.0.into_inner();
        let partial = indexed.0.into_inner();
        assert!(partial < full, "indexed {partial} blocks vs whole {full}");
        for (a, b) in first.iter().zip(&again) {
            assert_eq!(a.window, b.window);
            assert_eq!(
                (a.first, a.last, a.key_lines),
                (b.first, b.last, b.key_lines)
            );
            assert_eq!((&a.digest, a.total_lines), (&b.digest, b.total_lines));
        }
        let held = String::from_utf8(again[0].window.clone()).expect("utf8");
        assert!(!held.contains("SECRET"), "{held}");
        assert!(held.contains("line 4112\n"), "{held}");
        // The body lines in the window and the END marker.
        assert_eq!(again[0].key_lines, 12);
        // Another stamp is another version: the whole file streams again.
        let other = Blocks(Default::default());
        let moved = stamp.map(|stamp| octocode_engine::stamp::SourceStamp {
            size: stamp.size + 1,
            ..stamp
        });
        read_windows(&source, &path, moved, &spans, &other).expect("other");
        assert_eq!(other.0.into_inner(), full);
    }

    /// An offset past the end of a streamed read is the in-memory read's
    /// out-of-range page (nothing returned, a restart), not an error.
    #[test]
    fn a_streamed_offset_past_the_end_is_an_out_of_range_page() {
        let text = "x\n".repeat(50);
        let file = temp(text.as_bytes());
        let read = |query: serde_json::Value| {
            let query: LocalFetchQuery = serde_json::from_value(query).expect("query");
            fetch_window(
                &query,
                file.path(),
                &opened(file.path()),
                &std::fs::metadata(file.path()).expect("meta"),
                None,
                None,
                &crate::security::ContentSecurity::new(),
                &NeverCancel,
            )
        };
        for query in [
            serde_json::json!({"path":file.path(),"offset":80}),
            serde_json::json!({"path":file.path(),"ranges":["10-20"],"offset":30}),
        ] {
            let result = read(query);
            assert_eq!(result.status, ReadStatus::Success, "{result:?}");
            assert!(result.out_of_range, "{result:?}");
            assert_eq!(result.content.as_deref(), Some(""));
            let restart = result.next.and_then(|next| next.restart).expect("restart");
            assert_eq!(restart.query.offset, Some(0));
            assert!(
                result
                    .warnings
                    .iter()
                    .any(|w| w.contains("is past the end"))
            );
        }
        let past = read(serde_json::json!({"path":file.path(),"ranges":["90-95"]}));
        assert_eq!(past.error_code.as_deref(), Some("invalidInput"), "{past:?}");
    }

    /// A streamed page fits the row's share: its window page is cut to it,
    /// and ranges share it, the span that does not fit reading on.
    #[test]
    fn streamed_pages_fit_the_rows_share() {
        let text: String = (1..=2_000)
            .map(|line| format!("const s{line} = \"\\\\\\\\\";\n"))
            .collect();
        let file = temp(text.as_bytes());
        let share = 5_000;
        for (query, ranges) in [
            (serde_json::json!({"path":file.path()}), false),
            (
                serde_json::json!({"path":file.path(),"ranges":["1-300","600-900"]}),
                true,
            ),
        ] {
            let query: LocalFetchQuery = serde_json::from_value(query).expect("query");
            let result = fetch_window(
                &query,
                file.path(),
                &opened(file.path()),
                &std::fs::metadata(file.path()).expect("meta"),
                None,
                Some(share),
                &crate::security::ContentSecurity::new(),
                &NeverCancel,
            );
            assert_eq!(result.status, ReadStatus::Success, "{result:?}");
            let row = serde_json::to_value(&result).expect("row");
            assert_eq!(
                crate::response::read_share::fitted_lines(
                    &row,
                    &crate::response::result::query_value(&query),
                    share
                ),
                None,
                "the page fits its share: {}",
                row.to_string().len()
            );
            let next = result.next.expect("more to read");
            assert!(next.continue_window.is_some());
            assert_eq!(next.read_other_ranges.is_some(), ranges);
        }
    }

    /// A NUL byte past the head sample makes a streamed source binary.
    #[test]
    fn a_streamed_source_with_a_late_nul_is_binary() {
        let mut bytes = "x\n".repeat(20_000).into_bytes();
        bytes.extend_from_slice(b"\0\x01tail\n");
        let file = temp(&bytes);
        let query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":file.path()})).expect("query");
        let result = fetch_window(
            &query,
            file.path(),
            &opened(file.path()),
            &std::fs::metadata(file.path()).expect("meta"),
            None,
            None,
            &crate::security::ContentSecurity::new(),
            &NeverCancel,
        );
        assert_eq!(result.error_code.as_deref(), Some("binaryFileUnsupported"));
    }

    /// The reference for one byte page: a whole pass, no index.
    fn whole_pass(source: &std::fs::File, want: Want) -> Streamed {
        stream(source, &[want], &NeverCancel)
            .expect("stream")
            .0
            .pop()
            .expect("window")
    }

    fn assert_same_window(a: &Streamed, b: &Streamed) {
        assert_eq!(a.window, b.window);
        assert_eq!(
            (a.first, a.last, a.key_lines, a.origin.base, a.origin.total),
            (b.first, b.last, b.key_lines, b.origin.base, b.origin.total)
        );
        assert_eq!((&a.digest, a.total_lines), (&b.digest, b.total_lines));
    }

    /// Byte pages of one giant line, once the source is indexed, read about
    /// one window each wherever their offset sits, and hold what a whole
    /// pass holds: the window, its place in the line and the line's size.
    #[test]
    fn a_late_byte_page_costs_what_an_early_one_does() {
        let line: Vec<u8> = (0..6 * WINDOW_BYTES)
            .map(|at| b'a' + u8::try_from(at % 26).expect("letter"))
            .collect();
        let file = temp(&line);
        let source = opened(file.path());
        let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
        let path = file.path().canonicalize().expect("canonical");
        let page = |from: usize| Want::bytes(1, 1, from);
        let whole = Blocks(Default::default());
        read_windows(&source, &path, stamp, &[page(0)], &whole).expect("index");
        let whole = whole.0.into_inner();
        let mut costs = Vec::new();
        for from in [0, WINDOW_BYTES, 5 * WINDOW_BYTES - 7] {
            let blocks = Blocks(Default::default());
            let paged = read_windows(&source, &path, stamp, &[page(from)], &blocks)
                .expect("page")
                .pop()
                .expect("window");
            assert_same_window(&paged, &whole_pass(&source, page(from)));
            costs.push(blocks.0.into_inner());
        }
        // A page reads its window and at most one view-point gap before it
        // (64 KiB copy blocks, one partial block each), wherever it starts.
        let bound = (WINDOW_BYTES + VIEW_POINT_BYTES) / (64 * 1024) + 2;
        assert!(
            costs.iter().all(|&cost| cost <= bound) && bound * 3 < whole,
            "pages {costs:?}, bound {bound}, whole pass {whole} blocks"
        );
    }

    /// Byte pages of a many-line range no key block reaches match a whole
    /// pass wherever they start: mid-line, on a line start, or past the
    /// range's last byte.
    #[test]
    fn byte_pages_over_many_lines_match_a_whole_pass() {
        let text: String = (0..40)
            .map(|line| format!("{}\n", "q".repeat(150_000 + line * 7_919)))
            .collect();
        let file = temp(text.as_bytes());
        let source = opened(file.path());
        let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
        let path = file.path().canonicalize().expect("canonical");
        let page = |from: usize| Want::bytes(3, 38, from);
        read_windows(&source, &path, stamp, &[page(0)], &NeverCancel).expect("index");
        let line_start = text
            .split_inclusive('\n')
            .take(9)
            .map(str::len)
            .sum::<usize>();
        for from in [0, 1, 999_999, line_start, 2 * WINDOW_BYTES + 3, text.len()] {
            let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
                .expect("page")
                .pop()
                .expect("window");
            assert_same_window(&paged, &whole_pass(&source, page(from)));
        }
    }

    /// A range a private-key block reaches pages exactly as a whole pass
    /// does: the key lines stay replaced and counted, and the view keeps
    /// the placeholder's size.
    #[test]
    fn byte_pages_over_a_key_block_match_a_whole_pass() {
        let mut text = "x".repeat(3 * WINDOW_BYTES);
        text.push('\n');
        text.push_str(
            "-----BEGIN RSA PRIVATE KEY-----\nSECRETSECRETSECRET\n-----END RSA PRIVATE KEY-----\n",
        );
        text.push_str(&"y".repeat(2 * WINDOW_BYTES));
        text.push('\n');
        let file = temp(text.as_bytes());
        let source = opened(file.path());
        let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
        let path = file.path().canonicalize().expect("canonical");
        let page = |from: usize| Want::bytes(1, 5, from);
        read_windows(&source, &path, stamp, &[page(0)], &NeverCancel).expect("index");
        for from in [0, 2 * WINDOW_BYTES, 3 * WINDOW_BYTES, 4 * WINDOW_BYTES + 50] {
            let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
                .expect("page")
                .pop()
                .expect("window");
            assert_same_window(&paged, &whole_pass(&source, page(from)));
            assert!(!String::from_utf8_lossy(&paged.window).contains("SECRET"));
        }
    }

    /// View points that fall inside a key block (a giant body line, then a
    /// line of another shape that ends the block), CRLF line ends and an
    /// unterminated last line page exactly as a whole pass does, up to and
    /// past the range's end.
    #[test]
    fn byte_pages_with_points_inside_a_key_block_match_a_whole_pass() {
        let mut text = "x".repeat(2 * VIEW_POINT_BYTES + 17);
        text.push_str("\r\n-----BEGIN PRIVATE KEY-----\r\n");
        text.push_str(&"A".repeat(2 * VIEW_POINT_BYTES + 5));
        text.push_str("\r\n!!");
        text.push_str(&"B".repeat(VIEW_POINT_BYTES + 3));
        text.push_str("\r\n-----END PRIVATE KEY-----\r\n\r\n");
        text.push_str(&"€".repeat(VIEW_POINT_BYTES));
        let file = temp(text.as_bytes());
        let source = opened(file.path());
        let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
        let path = file.path().canonicalize().expect("canonical");
        let page = |from: usize| Want::bytes(1, 7, from);
        let first = read_windows(&source, &path, stamp, &[page(0)], &NeverCancel)
            .expect("index")
            .pop()
            .expect("window");
        let total = first.origin.total;
        for from in [
            1,
            VIEW_POINT_BYTES + 1,
            2 * VIEW_POINT_BYTES + 18,
            total / 2,
            total - WINDOW_BYTES / 2,
            total - 1,
            total,
            total + 9,
        ] {
            let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
                .expect("page")
                .pop()
                .expect("window");
            let whole = whole_pass(&source, page(from));
            assert_eq!(paged.window, whole.window, "from {from}");
            assert_eq!(
                (paged.first, paged.last, paged.key_lines, paged.origin.total),
                (whole.first, whole.last, whole.key_lines, whole.origin.total),
                "from {from}"
            );
            assert_eq!(paged.origin.base, whole.origin.base, "from {from}");
            assert_eq!(
                (&paged.digest, paged.total_lines),
                (&whole.digest, whole.total_lines)
            );
            // The base64 body stays replaced; the `!!` line is not base64,
            // so it ends the block (`LineShape::Other`) and shows as text.
            assert!(
                !String::from_utf8_lossy(&paged.window).contains("AAAA"),
                "from {from}"
            );
        }
    }

    /// A byte page whose start lies past the view a key block shrank to holds
    /// no byte, so it sits at no offset of the view.
    #[test]
    fn an_empty_window_after_a_key_block_shrank_the_view_has_no_base() {
        let mut text = String::from("-----BEGIN PRIVATE KEY-----\n");
        text.push_str(&"A".repeat(5_000));
        text.push_str("\n-----END PRIVATE KEY-----\n");
        let file = temp(text.as_bytes());
        let streamed = whole_pass(&opened(file.path()), Want::bytes(1, 3, 2_000));
        assert!(streamed.window.is_empty());
        assert!(streamed.origin.total < 2_000);
        assert_eq!(streamed.origin.base, 0);
    }

    #[test]
    fn a_giant_single_line_is_cut_to_a_window_not_buffered() {
        let big = vec![b'z'; WINDOW_BYTES + 10];
        let file = temp(&big);
        let streamed = stream_one(file.path(), 1, 1);
        assert_eq!(streamed.total_lines, 1);
        assert!(streamed.window.len() <= WINDOW_BYTES + 1);
        // The window is a part of the line's bytes: the view is all of them.
        assert_eq!(streamed.origin.base, 0);
        assert_eq!(streamed.origin.total, big.len());
    }

    #[test]
    fn cutting_a_utf8_line_keeps_only_complete_source_characters() {
        let text = format!("€{}", "🙂".repeat(WINDOW_BYTES / 4 + 1));
        let file = temp(text.as_bytes());
        let streamed = stream_one(file.path(), 1, 1);
        let prefix = std::str::from_utf8(&streamed.window).expect("complete UTF-8");
        assert!(text.starts_with(prefix));
        assert_eq!(prefix.len(), WINDOW_BYTES - 1);
        assert_eq!(streamed.origin.total, text.len());
    }
}
