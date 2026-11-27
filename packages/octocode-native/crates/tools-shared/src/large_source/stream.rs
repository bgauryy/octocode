//! One pass over a streamed source: hashed and split into lines, from its
//! start. Every request proves its digest from the bytes it reads.

use super::Streamed;
use super::error::ReadError;
use super::held::{Want, WindowLines};
use super::proven::{Pass, Proof};
use crate::source_view::ByteOrigin;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use std::io::Read;
#[cfg(test)]
use std::path::Path;

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
                lead: held.start.map_or(0, |start| start.lead),
                total: held.view_pos,
                redacted: held.redacted.into(),
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

/// One whole pass, its sealed windows, and whether the source holds a NUL byte.
pub(super) struct Streaming {
    pub(super) proven: super::proven::Proven,
    pub(super) total_lines: usize,
    pub(super) windows: Vec<Streamed>,
    pub(super) binary: bool,
}

/// Stream the opened source once from its start: hash all bytes, count
/// lines like `line_count`, and keep lines `first..=last` (1-based) of every
/// requested window, from its byte `from` on. The handle is the one the read opened and stat'ed (never
/// a reopen by path, which a swapped path could turn into a FIFO).
pub(super) fn stream_pass(
    source: &std::fs::File,
    path: &std::path::Path,
    wants: &[Want],
    scan: &dyn octocode_runtime_base::security::scan::ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Streaming, ReadError> {
    seek(source, 0)?;
    // A window from its lines' first byte is held as the pass streams; one at a
    // sealed offset is placed once the pass recorded the redactions before it.
    let held = wants.iter().all(|want| want.from == 0);
    let mut proof = Proof::new(source, path, if held { wants } else { &[] }, scan)?;
    let mut buffer = vec![0u8; READ_BYTES];
    let mut binary = false;
    loop {
        cancel.check()?;
        let read = match (&mut &*source).read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string().into()),
        };
        super::count_read(read);
        let chunk = &buffer[..read];
        binary |= chunk.contains(&0);
        proof.feed(chunk);
    }
    let mut pass: Pass = proof.finish()?;
    // A binary source is refused: nothing of it is shown.
    let windows = if binary {
        streamed(pass.lines, &pass.proven.digest, pass.total_lines, binary)
    } else if held {
        pass.seal_windows(source)?;
        streamed(pass.lines, &pass.proven.digest, pass.total_lines, binary)
    } else {
        pass.proven
            .windows(source, wants, pass.total_lines, cancel)?
    };
    Ok(Streaming {
        proven: pass.proven,
        total_lines: pass.total_lines,
        windows,
        binary,
    })
}

/// The windows of [`stream_pass`].
#[cfg(test)]
pub(super) fn stream(
    source: &std::fs::File,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<Vec<Streamed>, ReadError> {
    Ok(stream_pass(
        source,
        Path::new(""),
        wants,
        &octocode_runtime_base::security::ContentSecurity::new(),
        cancel,
    )?
    .windows)
}
