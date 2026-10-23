//! One pass over a streamed source: hashed and split into lines, from its
//! start or from an indexed checkpoint or view point.

use super::Streamed;
use super::held::{Want, WindowLines};
use super::index::{CHECKPOINT_LINES, Checkpoint, RangeView, StreamIndex, ViewPoint, byte_page};
use crate::execution::cancel::CancellationCheck;
use crate::security::KeyBlockState;
use crate::tools::source_view::ByteOrigin;

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
pub(super) fn stream(
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
pub(super) fn stream_indexed(
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
