//! Where a streamed source's redactions are, as its windows need to place them.
//!
//! A source whose structure the walk could keep has one edit per secret value,
//! listed ([`EditMap::Listed`]). A source whose every value is hidden has one per
//! value of the whole file, more than any memory budget may hold, so it is kept
//! as [`Index`] checkpoints instead: the first place at or after each
//! [`CHECKPOINT_BYTES`] where the scan of its values stands as a fresh one does
//! ([`EveryValueScan::quiet`]), with the edits and key lines before it counted.
//! The edits between two checkpoints (a block) come from one read of that block,
//! resumed at its checkpoint, and are held only while a window needs them. Every
//! byte held is charged to [`super::memory`]; a budget with no room refuses the
//! read, never a partial view.

use super::error::ReadError;
use super::masks::PlaceholderEdits;
use super::proven::{CHECKPOINT_BYTES, READ_BYTES};
use super::sealed::{Edit, Shift, keyed_offset, merge, outside_key_lines, shift};
use super::stream::seek;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::security::{EveryValueScan, Named, SecretSpan};
use octocode_io::memo::Reservation;
use std::io::{Read, Seek};
use std::ops::Range;
use std::sync::Arc;

const CHANGED: &str = "the source changed while it was read; read it again";

type KeyLines = [(Range<usize>, usize)];

/// The redactions of one source's sealed view.
#[derive(Clone)]
pub(super) enum EditMap {
    /// Every edit, in order.
    Listed(Arc<[Edit]>),
    /// Checkpoints, from which the edits of a block are read again.
    Blocks(Arc<Index>),
}

/// A quiet place of the source, and what lies before it.
struct Point {
    raw: usize,
    /// The edits that start before `raw`.
    before: Shift,
    /// What the key lines that start before `raw` add to its raw bytes
    /// (their placeholder less their bytes).
    key_delta: isize,
    /// The SHA-256 of the block that starts here.
    digest: [u8; 32],
}

/// The checkpoints of one source, whose blocks are `points[i].raw` up to the
/// next point (the last, up to `end`).
pub(super) struct Index {
    named: Named,
    scanned: Arc<[Edit]>,
    key_lines: Arc<[(Range<usize>, usize)]>,
    points: Vec<Point>,
    end: usize,
    total: Shift,
}

/// The edits of some whole blocks, with the memory they hold.
struct Loaded {
    from: usize,
    to: usize,
    edits: Vec<Edit>,
    _held: Reservation<'static>,
}

fn change(shift: Shift) -> isize {
    isize::try_from(shift.added).unwrap_or(isize::MAX)
        - isize::try_from(shift.removed).unwrap_or(isize::MAX)
}

fn signed(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

fn plus(shift: Shift, edit: &Edit) -> Shift {
    Shift {
        removed: shift.removed + edit.range.len(),
        added: shift.added + edit.text.len(),
    }
}

fn minus(a: Shift, b: Shift) -> Shift {
    Shift {
        removed: a.removed.saturating_sub(b.removed),
        added: a.added.saturating_sub(b.added),
    }
}

fn sum(a: Shift, b: Shift) -> Shift {
    Shift {
        removed: a.removed + b.removed,
        added: a.added + b.added,
    }
}

/// What the key lines that start in `from..to` add to their raw bytes.
fn key_delta(key_lines: &KeyLines, from: usize, to: usize) -> isize {
    let first = key_lines.partition_point(|(line, _)| line.start < from);
    key_lines[first..]
        .iter()
        .take_while(|(line, _)| line.start < to)
        .map(|(line, placeholder)| signed(*placeholder) - signed(line.len()))
        .sum()
}

/// Run `read` leaving the handle's position where it was: windows read the
/// file through it in order, and a block read comes in the middle.
fn in_place<T>(
    file: &std::fs::File,
    read: impl FnOnce() -> Result<T, ReadError>,
) -> Result<T, ReadError> {
    let before = (&*file)
        .stream_position()
        .map_err(|error| error.to_string())?;
    let out = read();
    seek(file, before)?;
    out
}

/// Make room for `more` items in `vec`, held as `held` says: the new block is
/// held on top of the old one while it is copied, then the old one's share is
/// given back.
pub(super) fn make_room<T>(
    vec: &mut Vec<T>,
    more: usize,
    held: &mut Reservation<'static>,
) -> Result<(), ReadError> {
    let size = std::mem::size_of::<T>().max(1);
    let (len, capacity) = (vec.len(), vec.capacity());
    let needed = len.saturating_add(more);
    if needed > capacity {
        let new = capacity.saturating_mul(2).max(needed);
        if !held.grow(new.saturating_mul(size)) {
            return Err(ReadError::Refused);
        }
        vec.reserve_exact(new - len);
        held.shrink(capacity.saturating_mul(size));
    }
    Ok(())
}

/// The edits of the block of `bytes`, which starts at source byte `lo` and whose
/// every value `spans` name: the scanner's own edits there, merged with them,
/// off the key lines. Holds what it builds through `held` before it builds it.
fn block_edits(
    scanned: &[Edit],
    key_lines: &KeyLines,
    lo: usize,
    bytes: &[u8],
    mut spans: Vec<SecretSpan>,
    held: &mut Reservation<'static>,
) -> Result<Vec<Edit>, ReadError> {
    spans.sort_by_key(|span| (span.range.start, span.range.end));
    let hi = lo + bytes.len();
    let bodies = |span: &SecretSpan| {
        bytes
            .get(span.range.start.wrapping_sub(lo)..span.range.end.wrapping_sub(lo))
            .ok_or_else(|| super::held::UNPLACED.to_owned())
    };
    // The most edits the spans make, one per line, each with its placeholder; the
    // merge and the key lines each copy them once more.
    let mut worst = 0usize;
    let mut pattern = "";
    let mut room = 0usize;
    for span in &spans {
        if span.form.pattern() != pattern {
            pattern = span.form.pattern();
            room = std::mem::size_of::<Edit>()
                + octocode_engine::security::redaction_placeholder(pattern).len();
        }
        let lines = bodies(span)?.iter().filter(|byte| **byte == b'\n').count() + 1;
        worst = worst.saturating_add(lines.saturating_mul(room));
    }
    if !held.grow(worst.saturating_mul(3)) {
        return Err(ReadError::Refused);
    }
    let mut structural = Vec::new();
    for span in &spans {
        let mut lines = PlaceholderEdits::new(span);
        lines.feed(bodies(span)?, &mut structural);
        lines.finish(&mut structural);
    }
    let first = scanned.partition_point(|edit| edit.range.start < lo);
    let last = scanned.partition_point(|edit| edit.range.start < hi);
    Ok(outside_key_lines(
        merge(scanned[first..last].to_vec(), structural),
        key_lines,
    ))
}

/// Whether a checkpoint at `at` would fall inside a redaction or a key line.
fn inside_one(scanned: &[Edit], key_lines: &KeyLines, at: usize) -> bool {
    let edit = scanned.partition_point(|edit| edit.range.start < at);
    let line = key_lines.partition_point(|(line, _)| line.start < at);
    edit.checked_sub(1)
        .and_then(|index| scanned.get(index))
        .is_some_and(|edit| edit.range.end > at)
        || line
            .checked_sub(1)
            .and_then(|index| key_lines.get(index))
            .is_some_and(|(line, _)| line.end > at)
}

/// One pass over the source behind `file`, whose bytes are `digest`: its
/// checkpoints, each block's edits counted as the pass reaches the block's end.
/// Bytes that are no longer `digest` fail the read.
pub(super) fn build(
    file: &std::fs::File,
    named: Named,
    digest: &str,
    scanned: Arc<[Edit]>,
    key_lines: Arc<[(Range<usize>, usize)]>,
    cancel: &impl CancellationCheck,
) -> Result<Index, ReadError> {
    seek(file, 0)?;
    let budget = super::memory();
    let mut held = budget.reserve(0).ok_or(ReadError::Refused)?;
    let mut whole = octocode_engine::digest::HashWriter::default();
    let mut block_hash = octocode_engine::digest::HashWriter::default();
    let mut scan = EveryValueScan::new(named);
    let (mut bytes, mut spans): (Vec<u8>, Vec<SecretSpan>) = (Vec::new(), Vec::new());
    let mut points = vec![Point {
        raw: 0,
        before: Shift::default(),
        key_delta: 0,
        digest: [0; 32],
    }];
    let (mut lo, mut total, mut keys) = (0usize, Shift::default(), 0isize);
    let mut target = octocode_engine::num::saturating_usize(CHECKPOINT_BYTES);
    let mut buffer = vec![0u8; READ_BYTES];
    // Ends the block that stands at `hi`: its edits are counted, then dropped.
    let mut close = |hi: usize,
                     bytes: &mut Vec<u8>,
                     spans: &mut Vec<SecretSpan>,
                     held: &mut Reservation<'static>,
                     block_hash: &mut octocode_engine::digest::HashWriter|
     -> Result<(), ReadError> {
        // The block's spans go with it: what they held is given back.
        let spans_held = spans.capacity() * std::mem::size_of::<SecretSpan>();
        let mut work = budget.reserve(0).ok_or(ReadError::Refused)?;
        let edits = block_edits(
            &scanned,
            &key_lines,
            lo,
            bytes,
            std::mem::take(spans),
            &mut work,
        )?;
        held.shrink(spans_held);
        total = edits.iter().fold(total, plus);
        keys += key_delta(&key_lines, lo, hi);
        let hash = std::mem::take(block_hash);
        if let Some(point) = points.last_mut() {
            point.digest = hash.finalize();
        }
        points.push(Point {
            raw: hi,
            before: total,
            key_delta: keys,
            digest: [0; 32],
        });
        bytes.clear();
        lo = hi;
        Ok(())
    };
    loop {
        cancel.check()?;
        let read = match (&mut &*file).read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string().into()),
        };
        super::count_read(read);
        let chunk = &buffer[..read];
        whole.update(chunk);
        let mut rest = chunk;
        while !rest.is_empty() {
            let taken = scan.feed_until_quiet(rest, target);
            let piece = &rest[..taken];
            rest = &rest[taken..];
            make_room(&mut bytes, piece.len(), &mut held)?;
            bytes.extend_from_slice(piece);
            block_hash.update(piece);
            let mut found = scan.take();
            make_room(&mut spans, found.len(), &mut held)?;
            spans.append(&mut found);
            if scan.at() >= target && scan.quiet() {
                if inside_one(&scanned, &key_lines, scan.at()) {
                    target = scan.at() + 1;
                } else {
                    close(
                        scan.at(),
                        &mut bytes,
                        &mut spans,
                        &mut held,
                        &mut block_hash,
                    )?;
                    target = scan.at() + octocode_engine::num::saturating_usize(CHECKPOINT_BYTES);
                }
            }
        }
    }
    if hex::encode(whole.finalize()) != digest {
        return Err(CHANGED.into());
    }
    let end = scan.at();
    let mut rest = scan.finish();
    make_room(&mut spans, rest.len(), &mut held)?;
    spans.append(&mut rest);
    // The last block is the one the points before it leave: it ends with the
    // source, and its point is not one to start from.
    close(end, &mut bytes, &mut spans, &mut held, &mut block_hash)?;
    points.pop();
    Ok(Index {
        named,
        scanned,
        key_lines,
        points,
        end,
        total,
    })
}

impl Index {
    fn block_of(&self, raw: usize) -> usize {
        self.points
            .partition_point(|point| point.raw <= raw)
            .saturating_sub(1)
    }

    /// The source bytes block `index` covers.
    fn bounds(&self, index: usize) -> Range<usize> {
        let lo = self.points.get(index).map_or(self.end, |point| point.raw);
        let hi = self
            .points
            .get(index + 1)
            .map_or(self.end, |point| point.raw);
        lo..hi
    }

    /// The edits that start before the block `index` starts.
    fn before(&self, index: usize) -> Shift {
        self.points
            .get(index)
            .map_or(self.total, |point| point.before)
    }

    /// The edits of block `index`, from its own bytes read again.
    fn rebuild(
        &self,
        file: &std::fs::File,
        index: usize,
        held: &mut Reservation<'static>,
    ) -> Result<Vec<Edit>, ReadError> {
        let Range { start: lo, end: hi } = self.bounds(index);
        let last = index + 1 >= self.points.len();
        let mut scan = EveryValueScan::resume(self.named, lo);
        let mut hash = octocode_engine::digest::HashWriter::default();
        let (mut bytes, mut spans): (Vec<u8>, Vec<SecretSpan>) = (Vec::new(), Vec::new());
        let mut buffer = vec![0u8; READ_BYTES];
        in_place(file, || {
            seek(file, lo as u64)?;
            let mut left = hi - lo;
            while left > 0 {
                let chunk = buffer.get_mut(..left.min(READ_BYTES)).unwrap_or_default();
                (&mut &*file)
                    .read_exact(chunk)
                    .map_err(|error| error.to_string())?;
                super::count_read(chunk.len());
                left -= chunk.len();
                hash.update(&*chunk);
                make_room(&mut bytes, chunk.len(), held)?;
                bytes.extend_from_slice(chunk);
                scan.feed(chunk);
                let mut found = scan.take();
                make_room(&mut spans, found.len(), held)?;
                spans.append(&mut found);
            }
            Ok(())
        })?;
        if self.points.get(index).map(|point| point.digest) != Some(hash.finalize()) {
            return Err(CHANGED.into());
        }
        if last {
            let mut rest = scan.finish();
            make_room(&mut spans, rest.len(), held)?;
            spans.append(&mut rest);
        }
        block_edits(&self.scanned, &self.key_lines, lo, &bytes, spans, held)
    }

    /// The edits of blocks `first..=last`.
    fn load(&self, file: &std::fs::File, first: usize, last: usize) -> Result<Loaded, ReadError> {
        let mut held = super::memory().reserve(0).ok_or(ReadError::Refused)?;
        let mut edits = Vec::new();
        for index in first..=last {
            edits.append(&mut self.rebuild(file, index, &mut held)?);
        }
        Ok(Loaded {
            from: self.bounds(first).start,
            to: self.bounds(last).end,
            edits,
            _held: held,
        })
    }

    fn shift(&self, file: &std::fs::File, span: Range<usize>) -> Result<Shift, ReadError> {
        if span.end <= span.start {
            return Ok(Shift::default());
        }
        let (first, last) = (self.block_of(span.start), self.block_of(span.end));
        let inside = |edits: &[Edit], keep: &dyn Fn(&Edit) -> bool| {
            edits
                .iter()
                .filter(|edit| keep(edit))
                .fold(Shift::default(), plus)
        };
        if first == last {
            let block = self.load(file, first, first)?;
            return Ok(inside(&block.edits, &|edit| {
                edit.range.start >= span.start && edit.range.end <= span.end
            }));
        }
        let head = if span.start == self.bounds(first).start {
            minus(self.before(first + 1), self.before(first))
        } else {
            let block = self.load(file, first, first)?;
            inside(&block.edits, &|edit| edit.range.start >= span.start)
        };
        let middle = minus(self.before(last), self.before(first + 1));
        let bounds = self.bounds(last);
        let tail = if span.end >= bounds.end {
            minus(self.before(last + 1), self.before(last))
        } else if span.end == bounds.start {
            Shift::default()
        } else {
            let block = self.load(file, last, last)?;
            inside(&block.edits, &|edit| edit.range.end <= span.end)
        };
        Ok(sum(sum(head, middle), tail))
    }

    /// [`keyed_offset`] over blocks: whole blocks before the one the sealed
    /// offset falls in are stepped over by their counted edits and key lines.
    fn keyed(
        &self,
        file: &std::fs::File,
        key_lines: &KeyLines,
        range_start: usize,
        sealed: usize,
    ) -> Result<Option<usize>, ReadError> {
        let first = self.block_of(range_start);
        let bounds = self.bounds(first);
        let block = self.load(file, first, first)?;
        // What the walk from `range_start` counts before it: the edits and key
        // lines that start before it are not its own.
        let own_edits = block
            .edits
            .iter()
            .take_while(|edit| edit.range.start < range_start)
            .fold(Shift::default(), plus);
        let edits_at = change(self.before(first)) + change(own_edits);
        let keys_at = self.points.get(first).map_or(0, |point| point.key_delta)
            + key_delta(key_lines, bounds.start, range_start);
        // The keyed and sealed offsets of a checkpoint, from `range_start`.
        let from_start = |point: &Point| {
            let keyed = signed(point.raw) - signed(range_start) + point.key_delta - keys_at;
            (keyed, keyed + change(point.before) - edits_at)
        };
        let after =
            self.points[first + 1..].partition_point(|point| from_start(point).1 <= signed(sealed));
        if after == 0 {
            return Ok(keyed_offset(&block.edits, key_lines, range_start, sealed));
        }
        let at = first + after;
        let (keyed, shown) = from_start(&self.points[at]);
        let (Ok(keyed), Ok(shown)) = (usize::try_from(keyed), usize::try_from(shown)) else {
            return Ok(None);
        };
        let block = self.load(file, at, at)?;
        Ok(keyed_offset(
            &block.edits,
            key_lines,
            self.bounds(at).start,
            sealed - shown,
        )
        .map(|inner| keyed + inner))
    }

    /// About how many bytes this holds.
    fn weight(&self) -> usize {
        std::mem::size_of::<Self>() + self.points.capacity() * std::mem::size_of::<Point>()
    }
}

impl EditMap {
    /// The edits that start in `span` and end in it.
    pub(super) fn shift(
        &self,
        file: &std::fs::File,
        span: Range<usize>,
    ) -> Result<Shift, ReadError> {
        match self {
            Self::Listed(edits) => Ok(shift(edits, span)),
            Self::Blocks(index) => index.shift(file, span),
        }
    }

    /// [`keyed_offset`] of the source's edits and `key_lines`.
    pub(super) fn keyed(
        &self,
        file: &std::fs::File,
        key_lines: &KeyLines,
        range_start: usize,
        sealed: usize,
    ) -> Result<Option<usize>, ReadError> {
        match self {
            Self::Listed(edits) => Ok(keyed_offset(edits, key_lines, range_start, sealed)),
            Self::Blocks(index) => index.keyed(file, key_lines, range_start, sealed),
        }
    }

    /// The edits near source bytes, as a window's runs ask for them.
    pub(super) fn view<'a>(&'a self, file: &'a std::fs::File) -> EditView<'a> {
        EditView {
            map: self,
            file,
            loaded: None,
        }
    }

    /// About how many bytes this holds.
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Listed(edits) => edits.iter().map(Edit::weight).sum(),
            Self::Blocks(index) => index.weight(),
        }
    }

    /// How many checkpoints this keeps; `None` for a listed map.
    #[cfg(test)]
    pub(super) fn checkpoints(&self) -> Option<usize> {
        match self {
            Self::Listed(_) => None,
            Self::Blocks(index) => Some(index.points.len()),
        }
    }

    /// Every edit, read from the source for a block map.
    #[cfg(test)]
    pub(super) fn all(&self, file: &std::fs::File) -> Result<Vec<Edit>, ReadError> {
        match self {
            Self::Listed(edits) => Ok(edits.to_vec()),
            Self::Blocks(index) => Ok(index.load(file, 0, index.points.len() - 1)?.edits.clone()),
        }
    }
}

/// A reader of a map's edits that keeps the blocks it last read.
pub(super) struct EditView<'a> {
    map: &'a EditMap,
    file: &'a std::fs::File,
    loaded: Option<Loaded>,
}

impl EditView<'_> {
    /// The edits that end after `span` starts, in order: the first of them are
    /// those inside it, and the edits past its end are the caller's to stop at.
    pub(super) fn over(&mut self, span: Range<usize>) -> Result<&[Edit], ReadError> {
        let edits = match self.map {
            EditMap::Listed(edits) => &edits[..],
            EditMap::Blocks(index) => {
                let held = self.loaded.as_ref().is_some_and(|loaded| {
                    loaded.from <= span.start && (span.end <= loaded.to || loaded.to >= index.end)
                });
                if !held {
                    self.loaded = None;
                    let first = index.block_of(span.start);
                    let last = index.block_of(span.end.saturating_sub(1).max(span.start));
                    self.loaded = Some(index.load(self.file, first, last)?);
                }
                self.loaded
                    .as_ref()
                    .map_or(&[][..], |loaded| &loaded.edits[..])
            }
        };
        let first = edits.partition_point(|edit| edit.range.end <= span.start);
        Ok(&edits[first..])
    }
}
