//! What a match scan proves of the whole source as it passes once: the
//! digest of every byte, line-start checkpoints and the private-key block
//! state at each. The page's windows are then read from the checkpoint
//! nearest before each, not by a second pass from the file's start, so a
//! match page reads its source once plus the bytes around its windows.

use super::Streamed;
use super::held::{Want, WindowLines};
use super::stream::{seek, streamed};
use crate::execution::cancel::CancellationCheck;
use crate::security::KeyBlockState;
use std::io::Read;

/// Source bytes between checkpoints: what a window may re-read before its
/// first line. Sizes the work, never the result.
const CHECKPOINT_BYTES: u64 = 1024 * 1024;
/// Source bytes a window read takes at once.
const READ_BYTES: usize = 64 * 1024;

/// A line start a later read of the same source may begin at.
#[derive(Clone, Copy)]
struct Checkpoint {
    offset: u64,
    /// The 1-based line starting at `offset`.
    line: usize,
    /// The private-key block state over the lines before it.
    block: KeyBlockState,
}

/// The handle's length and modification time.
type Stamp = (u64, Option<std::time::SystemTime>);

fn stamp(file: &std::fs::File) -> Result<Stamp, String> {
    let meta = file.metadata().map_err(|error| error.to_string())?;
    Ok((meta.len(), meta.modified().ok()))
}

/// The whole-source facts of one pass, fed every byte in order.
pub(super) struct Proof {
    hasher: octocode_engine::digest::HashWriter,
    lines: WindowLines,
    line: usize,
    offset: u64,
    next: u64,
    checkpoints: Vec<Checkpoint>,
    stamp: Stamp,
}

impl Proof {
    pub(super) fn new(file: &std::fs::File) -> Result<Self, String> {
        Ok(Self {
            hasher: octocode_engine::digest::HashWriter::default(),
            lines: WindowLines::new(&[], KeyBlockState::default()),
            line: 1,
            offset: 0,
            next: CHECKPOINT_BYTES,
            checkpoints: vec![Checkpoint {
                offset: 0,
                line: 1,
                block: KeyBlockState::default(),
            }],
            stamp: stamp(file)?,
        })
    }

    /// Take the source's next `bytes`.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
        let mut rest = bytes;
        while !rest.is_empty() {
            let (piece, newline) = split_line(rest);
            rest = &rest[piece.len()..];
            self.offset += piece.len() as u64;
            self.lines.push(piece, self.line);
            if newline {
                self.lines.finish(self.line);
                self.line += 1;
                if self.offset >= self.next {
                    self.checkpoints.push(Checkpoint {
                        offset: self.offset,
                        line: self.line,
                        block: self.lines.block,
                    });
                    self.next = self.offset + CHECKPOINT_BYTES;
                }
            }
        }
    }

    /// The proof once the source ended.
    pub(super) fn finish(self) -> Proven {
        Proven {
            digest: hex::encode(self.hasher.finalize()),
            checkpoints: self.checkpoints,
            stamp: self.stamp,
        }
    }
}

/// The bytes of `rest` up to and including its first newline, and whether
/// there was one.
fn split_line(rest: &[u8]) -> (&[u8], bool) {
    match rest.iter().position(|byte| *byte == b'\n') {
        Some(index) => (&rest[..=index], true),
        None => (rest, false),
    }
}

/// A whole source proved by one pass.
pub(super) struct Proven {
    pub(super) digest: String,
    checkpoints: Vec<Checkpoint>,
    stamp: Stamp,
}

impl Proven {
    /// The checkpoint to skip ahead to from `line` (a line start): the
    /// nearest before the next line a window still wants, when that is past
    /// `line`; `None` while `line` is wanted.
    fn ahead(&self, wants: &[Want], line: usize) -> Option<Checkpoint> {
        let needed = wants
            .iter()
            .filter(|want| want.last >= line)
            .map(|want| want.first)
            .min()?;
        if needed <= line {
            return None;
        }
        let at = self
            .checkpoints
            .partition_point(|point| point.line <= needed);
        self.checkpoints.get(at.checked_sub(1)?).copied()
    }

    /// The windows of `wants` from the proved source `file` with
    /// `total_lines` lines: each run of wanted lines is read from the
    /// checkpoint before it. The handle must still have the length and time
    /// it had when proved, so the windows hold the bytes the digest proves.
    pub(super) fn windows(
        &self,
        file: &std::fs::File,
        wants: &[Want],
        total_lines: usize,
        cancel: &impl CancellationCheck,
    ) -> Result<Vec<Streamed>, String> {
        let last = wants.iter().map(|want| want.last).max().unwrap_or(0);
        let mut lines = WindowLines::new(wants, KeyBlockState::default());
        let (mut line, mut at, mut pending) = (1usize, 0u64, false);
        let mut buffer = vec![0u8; READ_BYTES];
        seek(file, 0)?;
        'read: while line <= last {
            if !pending && let Some(point) = self.ahead(wants, line).filter(|p| p.offset > at) {
                seek(file, point.offset)?;
                (line, at) = (point.line, point.offset);
                lines.restart(point.block);
            }
            cancel.check()?;
            let read = (&mut &*file)
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            super::count_read(read);
            let mut rest = &buffer[..read];
            while !rest.is_empty() {
                let (piece, newline) = split_line(rest);
                rest = &rest[piece.len()..];
                at += piece.len() as u64;
                pending = !newline;
                lines.push(piece, line);
                if newline {
                    lines.finish(line);
                    line += 1;
                    let skip = self.ahead(wants, line).is_some_and(|p| p.offset > at);
                    if line > last || skip {
                        seek(file, at)?;
                        continue 'read;
                    }
                }
            }
        }
        if pending {
            lines.finish(line);
        }
        if stamp(file)? != self.stamp {
            return Err("the source changed while it was read; read it again".to_owned());
        }
        Ok(streamed(lines, &self.digest, total_lines, false))
    }
}
