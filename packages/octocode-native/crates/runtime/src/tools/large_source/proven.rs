//! What a match scan proves of the whole source as it passes once: the
//! digest of every byte, line-start checkpoints and the private-key block
//! state at each. The page's windows are then read from the checkpoint
//! nearest before each, not by a second pass from the file's start, so a
//! match page reads its source once plus the bytes around its windows.

mod record;
#[cfg(test)]
mod tests;

use record::split_line;

use super::Streamed;
use super::edit_map::{self, EditMap, make_room};
use super::error::ReadError;
use super::held::{InLine, SealMap, Want, WindowLines};
use super::masks::{Judged, judge};
use super::sealed::{Edit, Recorder, outside_key_lines};
use super::stream::{seek, streamed};
use crate::execution::cancel::CancellationCheck;
use crate::security::KeyBlockState;
use crate::security::scan::ContentScan;
use octocode_engine::security::{Named, SecretSpan, SourceNames};
use octocode_engine::stamp::SourceStamp;
use octocode_io::memo::Reservation;
use std::io::Read;
use std::sync::{Arc, OnceLock};

/// Source bytes between checkpoints: what a window may re-read before its
/// first line. Sizes the work, never the result.
pub(super) const CHECKPOINT_BYTES: u64 = 1024 * 1024;
/// Source bytes a window read takes at once.
pub(super) const READ_BYTES: usize = 64 * 1024;

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
pub(super) struct Proof<'s> {
    /// The read's scanner: the pass records what it redacts ([`Recorder`]).
    scan: &'s dyn ContentScan,
    recorder: Recorder,
    /// Source bytes of each line a private-key block replaces, and the bytes of
    /// its placeholder line.
    key_lines: Vec<(std::ops::Range<usize>, usize)>,
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

impl Pass {
    /// Seal the windows this pass holds: only now, with every byte seen, are
    /// the source's redactions known.
    pub(super) fn seal_windows(
        &mut self,
        file: &std::fs::File,
        cancel: &impl CancellationCheck,
    ) -> Result<(), ReadError> {
        if self.lines.held.is_empty() {
            return Ok(());
        }
        let edits = self.proven.edits(file, cancel)?;
        let judged = self.proven.judged(file, cancel)?;
        self.lines.seal(&edits, file, judged)
    }
}

/// A whole source proved by one pass.
pub(super) struct Proven {
    pub(super) digest: String,
    checkpoints: Vec<Checkpoint>,
    stamp: Stamp,
    pub(super) started: std::time::SystemTime,
    /// What the pass's scanner redacts, off the private-key lines.
    scanned: Arc<[Edit]>,
    /// Each line a private-key block replaces, as [`Proof`] found it.
    pub(super) key_lines: Arc<[(std::ops::Range<usize>, usize)]>,
    /// `scanned` with what the source's structure says is secret.
    sealed: OnceLock<EditMap>,
    named: Named,
    /// What the source's structure says is secret: walked at most once, when a
    /// window is first shown of a source that names something.
    judged: OnceLock<Judged>,
    in_lines: Vec<InLine>,
    long_lines: Vec<LongLineExtent>,
}

/// The placeholder edits of `spans`, each read from `file` through one buffer. A
/// chunk of `n` bytes ends at most `n / 2 + 1` lines, and the span's last line
/// one more: that many edits, with their placeholders, are held through `held`
/// before the chunk is fed, and what the chunk did not use is given back.
fn exact_edits(
    spans: &[SecretSpan],
    file: &std::fs::File,
    cancel: &impl CancellationCheck,
    held: &mut Reservation<'static>,
) -> Result<Vec<Edit>, ReadError> {
    let mut edits: Vec<Edit> = Vec::new();
    let mut buffer = vec![0u8; READ_BYTES];
    for span in spans {
        seek(file, span.range.start as u64)?;
        let text = octocode_engine::security::redaction_placeholder(span.form.pattern()).len();
        let mut lines = super::masks::PlaceholderEdits::new(span);
        let mut left = span.range.len();
        while left > 0 {
            cancel.check()?;
            let chunk = buffer.get_mut(..left.min(READ_BYTES)).unwrap_or_default();
            (&mut &*file)
                .read_exact(chunk)
                .map_err(|error| error.to_string())?;
            super::count_read(chunk.len());
            let most = chunk.len() / 2 + 1;
            let before = edits.len();
            hold_edits(&mut edits, most, text, held)?;
            lines.feed(chunk, &mut edits);
            held.shrink((most - (edits.len() - before)) * text);
            left -= chunk.len();
        }
        let before = edits.len();
        hold_edits(&mut edits, 1, text, held)?;
        lines.finish(&mut edits);
        held.shrink((1 - (edits.len() - before)) * text);
    }
    Ok(edits)
}

/// Hold room through `held` for `more` edits of `text` placeholder bytes each.
fn hold_edits(
    edits: &mut Vec<Edit>,
    more: usize,
    text: usize,
    held: &mut Reservation<'static>,
) -> Result<(), ReadError> {
    make_room(edits, more, held)?;
    if held.grow(more.saturating_mul(text)) {
        Ok(())
    } else {
        Err(ReadError::Refused)
    }
}

impl Proven {
    pub(super) fn judged(
        &self,
        file: &std::fs::File,
        cancel: &impl CancellationCheck,
    ) -> Result<&Judged, String> {
        if let Some(judged) = self.judged.get() {
            return Ok(judged);
        }
        let judged = judge(file, self.named, &self.digest, super::walk_budget(), cancel)?;
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
            + self.scanned.iter().map(Edit::weight).sum::<usize>()
            + self.key_lines.len() * std::mem::size_of::<(std::ops::Range<usize>, usize)>()
            + self.sealed.get().map_or(0, EditMap::weight)
    }

    /// Every edit a window of this source shows: the scanner's, and the values
    /// the source's structure says are secret.
    pub(super) fn edits(
        &self,
        file: &std::fs::File,
        cancel: &impl CancellationCheck,
    ) -> Result<EditMap, ReadError> {
        if let Some(edits) = self.sealed.get() {
            return Ok(edits.clone());
        }
        // `held` stays held until the map is built.
        let (structural, _held) = match self.judged(file, cancel)? {
            Judged::EveryValue => {
                let index = edit_map::build(
                    file,
                    self.named,
                    &self.digest,
                    self.scanned.clone(),
                    self.key_lines.clone(),
                    cancel,
                )?;
                let map = EditMap::Blocks(Arc::new(index));
                return Ok(self.sealed.get_or_init(|| map).clone());
            }
            Judged::Exact(spans) => {
                // Every edit this builds is held against the budget before it
                // exists, and given back when the map is built.
                let mut held = super::memory().reserve(0).ok_or(ReadError::Refused)?;
                let structural = exact_edits(spans, file, cancel, &mut held)?;
                if stamp(file)? != self.stamp {
                    return Err("the source changed while it was read; read it again".into());
                }
                if !(structural.is_empty() && self.scanned.is_empty()) {
                    // The merge copies the scanner's edits beside them and builds
                    // the merged list while both live; the key lines and the
                    // shared list each copy that list once more (a source whose
                    // structure names nothing copies the scanner's edits alone).
                    let copies = self.scanned.len().saturating_add(structural.len());
                    let scanned_text: usize = self.scanned.iter().map(|edit| edit.text.len()).sum();
                    let room = copies
                        .saturating_mul(2)
                        .saturating_mul(std::mem::size_of::<Edit>())
                        .saturating_add(scanned_text);
                    if !held.grow(room) {
                        return Err(ReadError::Refused);
                    }
                }
                (structural, held)
            }
        };
        let edits = outside_key_lines(
            super::sealed::merge(self.scanned.to_vec(), structural),
            &self.key_lines,
        );
        Ok(self
            .sealed
            .get_or_init(|| EditMap::Listed(edits.into()))
            .clone())
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

    /// The last place inside `want`'s line whose bytes before it the window
    /// skips: its sealed offset placed on the line's bytes by `map`.
    fn in_line_before(&self, want: &Want, map: &SealMap) -> Result<Option<&InLine>, ReadError> {
        let Some(line_at) = self
            .in_lines
            .iter()
            .find(|point| point.line == want.first)
            .map(|point| point.line_at)
        else {
            return Ok(None);
        };
        let Some(from) = map.keyed(line_at, want.from)? else {
            return Ok(None);
        };
        let at = self.in_lines.partition_point(|point| {
            point.line < want.first
                || (point.line == want.first && point.offset - point.line_at <= from)
        });
        Ok(at
            .checked_sub(1)
            .and_then(|index| self.in_lines.get(index))
            .filter(|point| point.line == want.first))
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
    ) -> Result<Vec<Streamed>, ReadError> {
        let judged = self.judged(file, cancel)?;
        let edits = self.edits(file, cancel)?;
        let map = SealMap {
            edits: edits.clone(),
            key_lines: self.key_lines.clone(),
            file: Arc::new(file.try_clone().map_err(|error| error.to_string())?),
        };
        let last = wants.iter().map(|want| want.last).max().unwrap_or(0);
        let mut lines = WindowLines::new(wants, KeyBlockState::default()).sealed_by(map.clone());
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
            && let Some(point) = self.in_line_before(want, &map)?
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
            return Err("the source changed while it was read; read it again".into());
        }
        lines.seal(&edits, file, judged)?;
        Ok(streamed(lines, &self.digest, total_lines, false))
    }
}
