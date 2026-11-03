//! One pass over a streamed source: hashed and split into lines, from its
//! start. Every request proves its digest from the bytes it reads.

use super::Streamed;
use super::held::{Want, WindowLines};
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
    binary: bool,
    cancel: &'a C,
}

impl<C: CancellationCheck> std::io::Write for LineSink<'_, C> {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        super::count_read(chunk.len());
        self.binary |= chunk.contains(&0);
        let mut rest = chunk;
        while !rest.is_empty() {
            let (piece, newline) = match rest.iter().position(|byte| *byte == b'\n') {
                Some(index) => (&rest[..=index], true),
                None => (rest, false),
            };
            rest = &rest[piece.len()..];
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

/// The windows `lines` held once a stream ends, with the source's totals.
pub(super) fn streamed(
    lines: WindowLines,
    digest: &str,
    total_lines: usize,
    binary: bool,
) -> Vec<Streamed> {
    lines
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

pub(super) fn seek(mut file: &std::fs::File, offset: u64) -> Result<(), String> {
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
) -> Result<Vec<Streamed>, String> {
    seek(source, 0)?;
    let mut sink = LineSink {
        lines: WindowLines::new(wants, KeyBlockState::default()),
        line: 1,
        pending: false,
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
    let binary = sink.binary;
    Ok(streamed(sink.lines, &digest, total_lines, binary))
}
