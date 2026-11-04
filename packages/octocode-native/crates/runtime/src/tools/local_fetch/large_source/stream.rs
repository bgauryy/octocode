//! One pass over a streamed source: hashed and split into lines, from its
//! start. Every request proves its digest from the bytes it reads.

use super::Streamed;
use super::held::{Want, WindowLines};
use super::proven::{Pass, Proof};
use crate::execution::cancel::CancellationCheck;
use crate::tools::source_view::ByteOrigin;
use std::io::Read;

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

/// Source bytes one pass reads at once.
const READ_BYTES: usize = 64 * 1024;

/// One whole pass, and whether the source holds a NUL byte.
pub(super) struct Streaming {
    pub(super) pass: Pass,
    pub(super) binary: bool,
}

/// Stream the opened source once from its start: hash all bytes, count
/// lines like `line_count`, and keep lines `first..=last` (1-based) of every
/// requested window, from its byte `from` on. The handle is the one the read opened and stat'ed (never
/// a reopen by path, which a swapped path could turn into a FIFO).
pub(super) fn stream_pass(
    source: &std::fs::File,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<Streaming, String> {
    seek(source, 0)?;
    let mut proof = Proof::new(source, wants)?;
    let mut buffer = vec![0u8; READ_BYTES];
    let mut binary = false;
    loop {
        cancel.check()?;
        let read = match (&mut &*source).read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        super::count_read(read);
        let chunk = &buffer[..read];
        binary |= chunk.contains(&0);
        proof.feed(chunk);
    }
    Ok(Streaming {
        pass: proof.finish(),
        binary,
    })
}

/// The windows of [`stream_pass`].
#[cfg(test)]
pub(super) fn stream(
    source: &std::fs::File,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<Vec<Streamed>, String> {
    let Streaming { pass, binary } = stream_pass(source, wants, cancel)?;
    Ok(streamed(
        pass.lines,
        &pass.proven.digest,
        pass.total_lines,
        binary,
    ))
}
