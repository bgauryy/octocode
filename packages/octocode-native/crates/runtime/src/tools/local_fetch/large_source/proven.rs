//! What a match scan proves of the whole source as it passes once: the
//! digest of every byte, line-start checkpoints and the private-key block
//! state at each. The page's windows are then read from the checkpoint
//! nearest before each, not by a second pass from the file's start, so a
//! match page reads its source once plus the bytes around its windows.

use super::Streamed;
use super::held::{InLine, Want, WindowLines};
use super::masks::{Judged, judge};
use super::stream::{seek, streamed};
use crate::execution::cancel::CancellationCheck;
use crate::security::KeyBlockState;
use octocode_engine::security::{Named, SourceNames};
use octocode_engine::stamp::SourceStamp;
use std::io::Read;
use std::sync::OnceLock;

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

/// A handle's length and modification time, and its [`SourceStamp`] when the
/// file has one: what the platform can show of a write that restores the time
/// (on non-Unix, the limit [`octocode_engine::stamp::cache_stamp`] documents).
type Stamp = (u64, Option<std::time::SystemTime>, Option<SourceStamp>);

fn stamp_of(meta: &std::fs::Metadata) -> Stamp {
    (meta.len(), meta.modified().ok(), SourceStamp::of(meta))
}

fn stamp(file: &std::fs::File) -> Result<Stamp, String> {
    file.metadata()
        .map(|meta| stamp_of(&meta))
        .map_err(|error| error.to_string())
}

/// The whole-source facts of one pass, fed every byte in order.
pub(super) struct Proof {
    hasher: octocode_engine::digest::HashWriter,
    lines: WindowLines,
    line: usize,
    /// Bytes seen on the current (unterminated) line.
    pending: bool,
    offset: u64,
    next: u64,
    checkpoints: Vec<Checkpoint>,
    stamp: Stamp,
    /// The settle clock before `stamp`: a write after it cannot be told from
    /// the stamp unless the stamp is older than it by the settle window.
    started: std::time::SystemTime,
    /// What the source spells that only its structure can judge.
    names: SourceNames,
    /// Places inside long lines a later read can start from, and those lines.
    in_lines: Vec<InLine>,
    long_lines: Vec<LongLineExtent>,
    line_at: u64,
    next_in_line: u64,
}

/// A line longer than [`CHECKPOINT_BYTES`] that no key block replaces: its number
/// and its bytes, so a read need not reach its end to know them.
#[derive(Clone, Copy)]
struct LongLineExtent {
    line: usize,
    bytes: usize,
}

/// What one whole pass over a source holds once it ended.
pub(super) struct Pass {
    pub(super) proven: Proven,
    pub(super) lines: WindowLines,
    pub(super) total_lines: usize,
}

impl Proof {
    /// A pass over `file`, the source at `path`, that also holds the windows of
    /// `wants`.
    pub(super) fn new(
        file: &std::fs::File,
        path: &std::path::Path,
        wants: &[Want],
    ) -> Result<Self, String> {
        let started = octocode_engine::stamp::settle_clock();
        Ok(Self {
            hasher: octocode_engine::digest::HashWriter::default(),
            lines: WindowLines::new(wants, KeyBlockState::default()),
            line: 1,
            pending: false,
            offset: 0,
            next: CHECKPOINT_BYTES,
            checkpoints: vec![Checkpoint {
                offset: 0,
                line: 1,
                block: KeyBlockState::default(),
            }],
            stamp: stamp(file)?,
            started,
            names: SourceNames::for_path(&path.to_string_lossy()),
            in_lines: Vec::new(),
            long_lines: Vec::new(),
            line_at: 0,
            next_in_line: 0,
        })
    }

    /// Take the source's next `bytes`.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
        self.names.feed(bytes);
        let mut rest = bytes;
        while !rest.is_empty() {
            let (piece, newline) = split_line(rest);
            rest = &rest[piece.len()..];
            self.offset += piece.len() as u64;
            self.pending = !newline;
            self.lines.push(piece, self.line);
            if !newline
                && self.offset - self.line_at >= CHECKPOINT_BYTES
                && self.offset >= self.next_in_line
            {
                self.in_lines.push(self.lines.in_line(self.line));
                self.next_in_line = self.offset + CHECKPOINT_BYTES;
            }
            if newline {
                self.close_line();
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

    fn close_line(&mut self) {
        let hidden = self.lines.finish(self.line);
        let bytes = self.offset - self.line_at;
        if bytes > CHECKPOINT_BYTES && !hidden {
            self.long_lines.push(LongLineExtent {
                line: self.line,
                bytes: octocode_engine::num::saturating_usize(bytes),
            });
        }
        self.line_at = self.offset;
    }

    /// The pass once the source ended.
    pub(super) fn finish(mut self) -> Pass {
        if self.pending {
            self.close_line();
        }
        Pass {
            proven: Proven {
                digest: hex::encode(self.hasher.finalize()),
                checkpoints: self.checkpoints,
                stamp: self.stamp,
                started: self.started,
                named: self.names.finish(),
                judged: OnceLock::new(),
                in_lines: self.in_lines,
                long_lines: self.long_lines,
            },
            lines: self.lines,
            total_lines: if self.pending {
                self.line
            } else {
                self.line - 1
            },
        }
    }
}

impl Pass {
    /// Hide what the source's structure says is secret in the windows this pass
    /// holds: only now, with every byte seen, is that known.
    pub(super) fn mask_windows(
        &mut self,
        file: &std::fs::File,
        cancel: &impl CancellationCheck,
    ) -> Result<(), String> {
        let judged = self.proven.judged(file, cancel)?;
        self.lines.mask(judged);
        Ok(())
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
    pub(super) started: std::time::SystemTime,
    named: Named,
    /// What the source's structure says is secret: walked at most once, when a
    /// window is first shown of a source that names something.
    judged: OnceLock<Judged>,
    in_lines: Vec<InLine>,
    long_lines: Vec<LongLineExtent>,
}

impl Proven {
    fn judged(
        &self,
        file: &std::fs::File,
        cancel: &impl CancellationCheck,
    ) -> Result<&Judged, String> {
        if let Some(judged) = self.judged.get() {
            return Ok(judged);
        }
        let budget = octocode_engine::memory::MEMORY.limit();
        let judged = judge(file, self.named, &self.digest, budget, cancel)?;
        Ok(self.judged.get_or_init(|| judged))
    }

    /// Whether `file` still has the stamp this pass started from, and `meta`
    /// saw that same state.
    pub(super) fn unchanged(&self, file: &std::fs::File, meta: &std::fs::Metadata) -> bool {
        stamp(file).is_ok_and(|now| now == self.stamp) && stamp_of(meta) == self.stamp
    }

    /// About how many bytes this proof holds.
    pub(super) fn weight(&self) -> usize {
        self.digest.len()
            + self.checkpoints.len() * std::mem::size_of::<Checkpoint>()
            + self.judged.get().map_or(0, Judged::weight)
            + self.in_lines.iter().map(InLine::weight).sum::<usize>()
            + self.long_lines.len() * std::mem::size_of::<LongLineExtent>()
    }

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

    /// The last place inside `want`'s line whose bytes before it the window skips.
    fn in_line_before(&self, want: &Want) -> Option<&InLine> {
        let at = self.in_lines.partition_point(|point| {
            point.line < want.first
                || (point.line == want.first && point.offset - point.line_at <= want.from)
        });
        self.in_lines
            .get(at.checked_sub(1)?)
            .filter(|point| point.line == want.first)
    }

    /// The windows of `wants` from the proved source `file` with
    /// `total_lines` lines: each run of wanted lines is read from the
    /// checkpoint before it. The handle must still have the stamp it had when
    /// proved, so the windows hold the bytes the digest proves.
    pub(super) fn windows(
        &self,
        file: &std::fs::File,
        wants: &[Want],
        total_lines: usize,
        cancel: &impl CancellationCheck,
    ) -> Result<Vec<Streamed>, String> {
        let judged = self.judged(file, cancel)?;
        let last = wants.iter().map(|want| want.last).max().unwrap_or(0);
        let mut lines = WindowLines::new(wants, KeyBlockState::default());
        let (mut line, mut at, mut pending) = (1usize, 0u64, false);
        let mut buffer = vec![0u8; READ_BYTES];
        seek(file, 0)?;
        // One window of one long line: from the nearest place before its bytes, and
        // no further than it holds.
        let long_line = match wants {
            [want] if want.first == want.last => self
                .long_lines
                .iter()
                .find(|long| long.line == want.first)
                .copied(),
            _ => None,
        };
        if let (Some(_), [want]) = (long_line, wants)
            && let Some(point) = self.in_line_before(want)
        {
            seek(file, point.offset as u64)?;
            lines.resume(point);
            (line, at, pending) = (point.line, point.offset as u64, true);
        }
        'read: while line <= last {
            if !pending && let Some(point) = self.ahead(wants, line).filter(|p| p.offset > at) {
                seek(file, point.offset)?;
                (line, at) = (point.line, point.offset);
                lines.restart(point.block, point.offset);
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
                if let Some(long) = long_line.filter(|_| lines.windows_full()) {
                    lines.close_known(long.line, long.bytes);
                    pending = false;
                    break 'read;
                }
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
        lines.mask(judged);
        Ok(streamed(lines, &self.digest, total_lines, false))
    }
}
