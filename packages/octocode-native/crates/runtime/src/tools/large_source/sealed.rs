//! The secret edits of a streamed source, recorded in its one hash pass.
//!
//! A source above the whole-file ceiling is never held whole, so its secret scan
//! runs as it streams: the pass judges the source in chunks with the read's own
//! scanner (line-preserving, as an in-memory read redacts its whole source) and
//! records each redaction as an [`Edit`]: the source bytes it hides and the
//! placeholder shown instead. The scanner judges what that read's scanner
//! judges: the text with each private-key line the pass closed shown as its
//! placeholder ([`KeyLines`]), so a block's marker left in the text names no
//! other line a key's. A chunk is committed only up to a clean edge, one
//! no redaction crosses; a redaction that reaches the chunk's end may continue, so
//! the edge waits behind it until it ends, however long it is. Every window of the
//! source is then sealed from those edits ([`apply`]) and its pages are cut from
//! the sealed text, never scanned again, so no secret depends on how far a page
//! looks back.

use super::edit_map::make_room;
use super::error::ReadError;
use crate::security::scan::ContentScan;
use octocode_io::memo::Reservation;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// One redaction: source bytes `range` show as `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Edit {
    pub(super) range: Range<usize>,
    pub(super) text: Box<str>,
}

impl Edit {
    /// About how many bytes this holds.
    pub(super) fn weight(&self) -> usize {
        std::mem::size_of::<Self>() + self.text.len()
    }
}

/// The sizes of one recorder's judgements.
#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    /// New text one judgement takes in. Sizes the work, never the result.
    pub(super) chunk: usize,
    /// Text judged past an edge, and kept before it as context: a bounded
    /// secret starting before an edge ends inside it.
    pub(super) overlap: usize,
}

impl Limits {
    pub(super) const REAL: Self = Self {
        chunk: 1024 * 1024,
        overlap: 256 * 1024,
    };
}

const _: () = assert!(Limits::REAL.overlap >= octocode_engine::security::LONGEST_BOUNDED_MATCH);

/// Why a recording did not end in edits.
#[derive(Debug)]
pub(super) enum RecordError {
    /// The memory budget has no room to hold the edits recorded so far.
    Refused,
    /// The scanner refused the source, as its message.
    Failed(String),
}

impl From<RecordError> for ReadError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Refused => Self::Refused,
            RecordError::Failed(message) => Self::Failed(message),
        }
    }
}

/// The lines of a source a private-key block replaces, as its pass found them
/// so far: each line's source bytes with the bytes of its placeholder line
/// (in order), and the source byte the pass's lines are closed up to.
#[derive(Clone, Copy)]
pub(super) struct KeyLines<'a> {
    pub(super) lines: &'a [(Range<usize>, usize)],
    pub(super) closed: usize,
}

/// Judges a source as it streams and records its edits, each held against the
/// memory budget (through `held`) from the moment it is recorded.
pub(super) struct Recorder {
    limits: Limits,
    path: PathBuf,
    /// Decoded text not yet committed, behind the context kept before it.
    text: String,
    /// Source byte `text` starts at.
    at: usize,
    /// Bytes of `text` before this are context: judged again, never recorded.
    owned: usize,
    /// The end of a character the last piece cut.
    pending: Vec<u8>,
    /// Where decoding replaced invalid bytes: (text offset after the
    /// replacement, the source offset from `at` it stands for).
    shifts: Vec<(usize, usize)>,
    /// The text length a judgement waits for.
    due: usize,
    edits: Vec<Edit>,
    held: Reservation<'static>,
    failed: Option<RecordError>,
}

impl Recorder {
    pub(super) fn new(path: &Path, limits: Limits, held: Reservation<'static>) -> Self {
        Self {
            limits,
            path: path.to_path_buf(),
            text: String::new(),
            at: 0,
            owned: 0,
            pending: Vec::new(),
            shifts: Vec::new(),
            due: limits.chunk + 2 * limits.overlap,
            edits: Vec::new(),
            held,
            failed: None,
        }
    }

    /// Take the source's next `bytes`, judging when a chunk is due.
    pub(super) fn feed(
        &mut self,
        bytes: &[u8],
        scan: &(impl ContentScan + ?Sized),
        keys: KeyLines<'_>,
    ) {
        if self.failed.is_some() {
            return;
        }
        self.decode(bytes);
        if self.text.len() >= self.due {
            self.judge(scan, false, keys);
        }
    }

    /// The source ended: its edits, in order, with the room they and the shared
    /// copy made of them are held in (given back when it drops), or the
    /// scanner's refusal or the budget's.
    pub(super) fn finish(
        mut self,
        scan: &(impl ContentScan + ?Sized),
        keys: KeyLines<'_>,
    ) -> Result<(Vec<Edit>, Reservation<'static>), RecordError> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.push_lossy(&rest);
        }
        if self.failed.is_none() {
            self.judge(scan, true, keys);
        }
        if let Some(error) = self.failed {
            return Err(error);
        }
        // The shared list is a copy beside these edits while it is built.
        let copy: usize = self.edits.iter().map(Edit::weight).sum();
        if !self.held.grow(copy) {
            return Err(RecordError::Refused);
        }
        Ok((self.edits, self.held))
    }

    fn push_lossy(&mut self, bad: &[u8]) {
        let source = self.source_at(self.text.len()) - self.at + bad.len();
        self.text.push('\u{fffd}');
        self.shifts.push((self.text.len(), source));
    }

    fn decode(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    self.text.push_str(text);
                    self.pending.clear();
                    return;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    let (good, rest) = self.pending.split_at(valid);
                    self.text
                        .push_str(std::str::from_utf8(good).unwrap_or_default());
                    let Some(bad) = error.error_len() else {
                        self.pending = rest.to_vec();
                        return;
                    };
                    let bad_bytes = rest[..bad].to_vec();
                    self.pending = rest[bad..].to_vec();
                    self.push_lossy(&bad_bytes);
                }
            }
        }
    }

    /// The source byte text offset `at` stands for.
    fn source_at(&self, at: usize) -> usize {
        let index = self.shifts.partition_point(|(from, _)| *from <= at);
        match index.checked_sub(1) {
            Some(index) => {
                let (from, source) = self.shifts[index];
                self.at + source + (at - from)
            }
            None => self.at + at,
        }
    }

    /// The text offset source byte `source` is read at: the inverse of
    /// [`Self::source_at`], within the text.
    fn text_at(&self, source: usize) -> usize {
        let from = source.saturating_sub(self.at);
        let index = self.shifts.partition_point(|(_, at)| *at <= from);
        let at = match index.checked_sub(1) {
            Some(index) => {
                let (text, at) = self.shifts[index];
                text.saturating_add(from - at)
            }
            None => from,
        };
        self.boundary(at)
    }

    /// The text up to `end` as the scanner judges it, with where it differs
    /// from the text: each stretch of a private-key line of `keys` shown as
    /// that line's placeholder, behind the line's own lead, its line break
    /// kept. Each entry is the stretch's text bytes and where its
    /// replacement starts in the judged text.
    fn keyed(&self, end: usize, keys: KeyLines<'_>) -> (std::borrow::Cow<'_, str>, Vec<Keyed>) {
        let text = self.text.get(..end).unwrap_or_default();
        let (from, to) = (self.at, self.source_at(end));
        let first = keys.lines.partition_point(|(line, _)| line.end <= from);
        let mut judged = String::new();
        let mut keyed: Vec<Keyed> = Vec::new();
        let mut copied = 0;
        let placeholder = crate::security::key_fragment_placeholder();
        for (line, shown) in keys.lines[first..]
            .iter()
            .take_while(|(line, _)| line.start < to)
        {
            let start = self.text_at(line.start.max(from));
            let stop = self.text_at(line.end).min(end);
            if stop <= start {
                continue;
            }
            // The line's bytes before its key stay the source's own: the
            // placeholder line is that lead, the placeholder and a line break.
            let lead = shown.saturating_sub(placeholder.len() + 1);
            let whole = line.start >= from && text.is_char_boundary(start + lead);
            let broken = usize::from(
                text.as_bytes()
                    .get(start..stop)
                    .is_some_and(|line| line.ends_with(b"\n")),
            );
            let lead = if whole {
                lead.min(stop - start - broken)
            } else {
                0
            };
            judged.push_str(text.get(copied..start + lead).unwrap_or_default());
            keyed.push(Keyed {
                raw: start + lead..stop - broken,
                at: judged.len(),
                len: placeholder.len(),
            });
            judged.push_str(&placeholder);
            copied = stop - broken;
        }
        if keyed.is_empty() {
            return (text.into(), keyed);
        }
        judged.push_str(text.get(copied..).unwrap_or_default());
        (judged.into(), keyed)
    }

    fn boundary(&self, mut at: usize) -> usize {
        at = at.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// A line start within an overlap before `at`, else `at`.
    fn line_edge(&self, at: usize) -> usize {
        let from = self.boundary(at.saturating_sub(self.limits.overlap));
        self.text.as_bytes()[from..at]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(at, |newline| from + newline + 1)
    }

    fn judge(&mut self, scan: &(impl ContentScan + ?Sized), last: bool, keys: KeyLines<'_>) {
        // A line still open is judged with the lines closed before it once
        // it closes, when the pass knows whether a key block replaces it;
        // one longer than an overlap is judged as it passes.
        let closed = self.text_at(keys.closed);
        let len = if last || self.text.len() - closed > self.limits.overlap {
            self.text.len()
        } else {
            closed
        };
        let (judged, keyed) = self.keyed(len, keys);
        let found = match scan.redact_source_lines(&judged, &self.path) {
            Ok((clean, true)) => align(&judged, &clean)
                .into_iter()
                .filter_map(|(range, text)| Some((unkeyed(&keyed, range)?, text)))
                .collect(),
            Ok((_, false)) => Vec::new(),
            Err(error) => {
                self.failed = Some(RecordError::Failed(error.1));
                return;
            }
        };
        let mut edge = if last {
            len
        } else {
            self.line_edge(self.boundary(len.saturating_sub(self.limits.overlap)))
        };
        // No edit is cut: an edge inside one moves before it.
        for (range, _) in found.iter().rev() {
            if range.start < edge && range.end > edge {
                edge = range.start;
            }
        }
        if edge <= self.owned {
            // A redaction from the owned text's start reaches past the overlap:
            // it may go on, so the judgement waits for more of it.
            self.due = self.text.len() + self.limits.chunk;
            return;
        }
        let mut fresh = Vec::new();
        for (range, text) in found {
            if range.end <= self.owned || range.start >= edge {
                continue;
            }
            // A match that starts in the context was not whole in the judgement
            // that owned its start: its owned part is hidden.
            let start = range.start.max(self.owned);
            fresh.push(Edit {
                range: self.source_at(start)..self.source_at(range.end),
                text: text.into(),
            });
        }
        // Held before they are kept: the list's room, then each placeholder.
        let text: usize = fresh.iter().map(|edit| edit.text.len()).sum();
        if make_room(&mut self.edits, fresh.len(), &mut self.held).is_err() || !self.held.grow(text)
        {
            self.failed = Some(RecordError::Refused);
            return;
        }
        self.edits.extend(fresh);
        if last {
            return;
        }
        let keep = self.line_edge(edge);
        let keep = if keep == edge {
            self.boundary(edge.saturating_sub(self.limits.overlap))
        } else {
            keep
        };
        let source = self.source_at(keep);
        self.text.drain(..keep);
        // Re-base: the kept text starts at source byte `source`.
        let dropped = source - self.at;
        self.shifts = self
            .shifts
            .iter()
            .filter(|(from, _)| *from > keep)
            .map(|(from, at)| (from - keep, at - dropped))
            .collect();
        self.at = source;
        self.owned = edge - keep;
        self.due = self.text.len().max(self.owned) + self.limits.chunk;
    }
}

/// A stretch of a recorder's text the scanner judged as a key line's
/// placeholder ([`Recorder::keyed`]).
struct Keyed {
    /// The text bytes the placeholder stands for.
    raw: Range<usize>,
    /// Where the placeholder starts in the judged text, and its length.
    at: usize,
    len: usize,
}

/// The text bytes `range` of the judged text stands for: an end inside a
/// placeholder is that end of the stretch it replaced. `None` for a range
/// wholly inside one, which a key line's own placeholder already shows.
fn unkeyed(keyed: &[Keyed], range: Range<usize>) -> Option<Range<usize>> {
    let place = |at: usize, end: bool| {
        let index = keyed.partition_point(|stretch| stretch.at + stretch.len <= at);
        match keyed.get(index) {
            Some(stretch) if stretch.at < at || (stretch.at == at && !end) => {
                if end {
                    stretch.raw.end
                } else {
                    stretch.raw.start
                }
            }
            // Past every stretch before `index`: as far past the last one's
            // bytes as past its placeholder.
            _ => match index.checked_sub(1).and_then(|before| keyed.get(before)) {
                Some(stretch) => stretch.raw.end + (at - (stretch.at + stretch.len)),
                None => at,
            },
        }
    };
    let within = keyed
        .iter()
        .any(|stretch| stretch.at <= range.start && range.end <= stretch.at + stretch.len);
    let (start, end) = (place(range.start, false), place(range.end, true));
    (!within && start <= end).then_some(start..end)
}

/// What the edits inside a span of source bytes do to its length: the source
/// bytes they replace and the placeholder bytes they show. A placeholder can be
/// shorter or longer than its source (a structural value such as `"a":"b"`
/// becomes a longer marker), so neither is netted against the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Shift {
    pub(super) removed: usize,
    pub(super) added: usize,
}

impl Shift {
    /// The sealed length of `len` bytes holding these edits' sources; `None`
    /// when they do not (an edit outside the span it was counted in).
    pub(super) fn apply(self, len: usize) -> Option<usize> {
        len.checked_add(self.added)?.checked_sub(self.removed)
    }
}

/// The [`Shift`] of the edits inside source bytes `span`.
pub(super) fn shift(edits: &[Edit], span: Range<usize>) -> Shift {
    let first = edits.partition_point(|edit| edit.range.start < span.start);
    edits
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take_while(|edit| edit.range.end <= span.end)
        .fold(Shift::default(), |shift, edit| Shift {
            removed: shift.removed + edit.range.len(),
            added: shift.added + edit.text.len(),
        })
}

/// The keyed offset (counted from source byte `range_start`, a line start, with
/// each private-key line as its placeholder) of the sealed offset `sealed`. An
/// offset inside a placeholder is its edit's start. `None` when the edits and
/// key lines are not in order without overlap, as the pass records them.
pub(super) fn keyed_offset(
    edits: &[Edit],
    key_lines: &[(Range<usize>, usize)],
    range_start: usize,
    sealed: usize,
) -> Option<usize> {
    let (mut raw, mut keyed, mut shown) = (range_start, 0usize, 0usize);
    let mut edits = edits[edits.partition_point(|edit| edit.range.start < range_start)..]
        .iter()
        .peekable();
    let mut keys = key_lines[key_lines.partition_point(|(line, _)| line.start < range_start)..]
        .iter()
        .peekable();
    loop {
        let next_edit = edits.peek().map(|edit| edit.range.start);
        let next_key = keys.peek().map(|(line, _)| line.start);
        let next = match (next_edit, next_key) {
            (Some(edit), Some(key)) => edit.min(key),
            (Some(at), None) | (None, Some(at)) => at,
            (None, None) => return Some(keyed + (sealed - shown)),
        };
        let plain = next.checked_sub(raw)?;
        if sealed <= shown + plain {
            return Some(keyed + (sealed - shown));
        }
        (keyed, shown, raw) = (keyed + plain, shown + plain, next.max(raw));
        if next_key == Some(next) && next_edit != Some(next) {
            let Some((line, placeholder)) = keys.next() else {
                continue;
            };
            if sealed < shown + placeholder {
                return Some(keyed + (sealed - shown));
            }
            (keyed, shown, raw) = (keyed + placeholder, shown + placeholder, line.end);
        } else if let Some(edit) = edits.next() {
            if sealed < shown + edit.text.len() {
                return Some(keyed);
            }
            (keyed, shown, raw) = (
                keyed + edit.range.len(),
                shown + edit.text.len(),
                edit.range.end.max(raw),
            );
        }
    }
}

/// The redactions turning `raw` into `clean`, a line-preserving redaction of it:
/// each placeholder with the bytes it stands for. A line whose placeholders cannot
/// be placed on its bytes is one edit from its first changed byte to its last.
pub(super) fn align(raw: &str, clean: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    for (raw_line, clean_line) in raw.split_inclusive('\n').zip(clean.split_inclusive('\n')) {
        if raw_line != clean_line {
            line_edits(raw_line, clean_line, at, &mut out);
        }
        at += raw_line.len();
    }
    out
}

fn line_edits(raw: &str, clean: &str, at: usize, out: &mut Vec<(Range<usize>, String)>) {
    let prefix = raw
        .char_indices()
        .zip(clean.chars())
        .find(|((_, a), b)| a != b)
        .map_or(raw.len().min(clean.len()), |((index, _), _)| index);
    let prefix = clean.floor_char_boundary(prefix);
    let room = raw.len().min(clean.len()) - prefix;
    let suffix = raw
        .bytes()
        .rev()
        .zip(clean.bytes().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let mut suffix = suffix;
    while !raw.is_char_boundary(raw.len() - suffix) || !clean.is_char_boundary(clean.len() - suffix)
    {
        suffix -= 1;
    }
    let (Some(raw_mid), Some(clean_mid)) = (
        raw.get(prefix..raw.len() - suffix),
        clean.get(prefix..clean.len() - suffix),
    ) else {
        // Edges off a character boundary: the whole line is one edit.
        out.push((at..at + raw.len(), clean.to_owned()));
        return;
    };
    let base = at + prefix;
    match place(raw_mid, clean_mid) {
        Some(edits) => out.extend(
            edits
                .into_iter()
                .map(|(range, text)| (base + range.start..base + range.end, text)),
        ),
        None => out.push((base..base + raw_mid.len(), clean_mid.to_owned())),
    }
}

/// The placeholders of `clean` on the bytes of `raw` they stand for, the text
/// between them matched in order; `None` when it does not match.
fn place(raw: &str, clean: &str) -> Option<Vec<(Range<usize>, String)>> {
    let marks: Vec<Range<usize>> = octocode_engine::security::placeholder_regex()
        .find_iter(clean)
        .map(|found| found.range())
        .collect();
    if marks.is_empty() {
        return None;
    }
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let lead = clean.get(..marks.first()?.start)?;
    let mut pos = raw.strip_prefix(lead).map(|_| lead.len())?;
    let mut join = false;
    for (index, mark) in marks.iter().enumerate() {
        let tail_end = marks.get(index + 1).map_or(clean.len(), |next| next.start);
        let between = clean.get(mark.end..tail_end)?;
        let placeholder = clean.get(mark.clone())?;
        let end = if index + 1 == marks.len() {
            let end = raw.len().checked_sub(between.len())?;
            (raw.get(end..)? == between && end >= pos).then_some(end)?
        } else if between.is_empty() {
            // Adjacent placeholders: one edit shows both.
            pos
        } else {
            let from = raw.ceil_char_boundary(pos + 1);
            from + raw.get(from..)?.find(between)?
        };
        match edits.last_mut() {
            Some((range, text)) if join => {
                range.end = end;
                text.push_str(placeholder);
            }
            _ => edits.push((pos..end, placeholder.to_owned())),
        }
        join = between.is_empty() && index + 1 < marks.len();
        pos = end + between.len();
    }
    Some(edits)
}

/// Merge `more` edits (each a placeholder for its bytes; the structure's, which
/// a source held whole puts first) into the sorted `edits`: an edit inside
/// another goes with it, as a whole read hides a span inside another, so the
/// same bytes show one placeholder, the structure's on a tie; edits that only
/// overlap become one edit over both, showing each placeholder.
pub(super) fn merge(edits: Vec<Edit>, more: Vec<Edit>) -> Vec<Edit> {
    let mut all: Vec<Edit> = more.into_iter().chain(edits).collect();
    // Stable: on the same bytes, `more` stays first.
    all.sort_by_key(|edit| (edit.range.start, std::cmp::Reverse(edit.range.end)));
    let mut out: Vec<Edit> = Vec::with_capacity(all.len());
    for edit in all {
        match out.last_mut() {
            Some(last) if edit.range.end <= last.range.end && edit.range.start < last.range.end => {
            }
            Some(last) if edit.range.start < last.range.end => {
                last.range.end = last.range.end.max(edit.range.end);
                if !last.text.contains(&*edit.text) {
                    last.text = format!("{}{}", last.text, edit.text).into();
                }
            }
            _ => out.push(edit),
        }
    }
    out
}

/// `edits` less their bytes on lines a private-key block replaces: such a line
/// shows only its placeholder. An edit cut by one keeps its placeholder.
pub(super) fn outside_key_lines(
    edits: Vec<Edit>,
    key_lines: &[(Range<usize>, usize)],
) -> Vec<Edit> {
    if key_lines.is_empty() {
        return edits;
    }
    let mut out = Vec::with_capacity(edits.len());
    for edit in edits {
        let mut range = edit.range.clone();
        let first = key_lines.partition_point(|(line, _)| line.end <= range.start);
        for (line, _) in key_lines[first..]
            .iter()
            .take_while(|(line, _)| line.start < edit.range.end)
        {
            if line.start <= range.start {
                range.start = range.start.max(line.end);
            } else {
                // The bytes past this key line stay hidden by the same edit.
                if range.start < line.start {
                    out.push(Edit {
                        range: range.start..line.start,
                        text: edit.text.clone(),
                    });
                }
                range.start = line.end;
            }
        }
        if range.start < range.end {
            out.push(Edit {
                range,
                text: edit.text,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
