//! The text a match of a source past the whole-file ceiling runs over: every
//! line a read serves differently from the source, as the read serves it. A
//! search, or a read's own match scan, then matches what a read shows, never
//! a byte it withholds, without holding the source's text whole: only the
//! changed lines are kept, and the source streams past them.
use super::*;
use std::collections::BTreeMap;
use std::io::{BufRead, Read, Seek, SeekFrom};

/// Each line (1-based) a read serves differently from the source, with the
/// text it serves there on one line.
pub(crate) type ChangedLines = BTreeMap<usize, String>;

/// The changed lines of each source judged, kept by the digest of its bytes,
/// its path and the scanner's limits: a later match of the same bytes takes
/// them without judging the source again. Weighed against the one process
/// budget ([`crate::tools::page_memo::PageMemo`]).
static JUDGED: crate::tools::page_memo::PageMemo<std::sync::Arc<(ChangedLines, Option<String>)>> =
    crate::tools::page_memo::PageMemo::new();

/// One line window to judge: its lines and the source bytes they take.
struct Planned {
    first: usize,
    last: usize,
    bytes: usize,
}

/// The lines of `file`, the source at `path` past the whole-file ceiling, a
/// read serves differently from the source, as it serves them: cut from the
/// one streamed judgment of the whole source ([`served_lines`]) window by
/// window. A line longer than a window is judged on its own, in overlapping
/// windows. With them, the digest of the bytes the judgment read (`None`
/// when no window was served): a match of the source proves it read the same
/// bytes. `None` for a source with no text lines to serve (binary, or
/// UTF-16, which no line of its bytes holds).
pub(crate) fn changed_lines(
    path: &Path,
    file: &std::fs::File,
    meta: &std::fs::Metadata,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Option<(ChangedLines, Option<String>)>, String> {
    let failed = |error: std::io::Error| error.to_string();
    let mut head = Vec::new();
    let mut handle = file;
    handle.seek(SeekFrom::Start(0)).map_err(failed)?;
    handle.take(2).read_to_end(&mut head).map_err(failed)?;
    if octocode_engine::text::has_utf16_bom(&head) {
        return Ok(None);
    }
    let limits = security.limits();
    let room = (limits.window / 2).max(1);
    let (planned, digest) = plan(file, room, cancel).map_err(failed)?;
    let key = format!("{}\n{}", hex::encode(digest), path.to_string_lossy());
    let policy = format!("{limits:?} {}", std::any::type_name_of_val(security));
    if let Some(kept) = JUDGED.get(&key, &policy, std::sync::Arc::clone) {
        return Ok(Some((*kept).clone()));
    }
    // A judgment's windows are served together while they fit what one
    // pass may hold; a later pass reads the first's proof.
    let batch = room.max(octocode_engine::memory::MEMORY.limit() / 4);
    let mut changed = ChangedLines::new();
    let mut known: Option<String> = None;
    let mut offset = 0u64;
    let mut at = 0;
    while at < planned.len() {
        let mut end = at;
        let mut bytes = 0usize;
        while end < planned.len() && (end == at || bytes + planned[end].bytes <= batch) {
            bytes += planned[end].bytes;
            end += 1;
        }
        let group = &planned[at..end];
        at = end;
        // A line longer than a window is no window's whole line: it is
        // judged on its own below, and only the others are served.
        let wants = group
            .iter()
            .filter(|window| window.bytes <= room)
            .map(|window| (window.first, window.last))
            .collect::<Vec<_>>();
        let mut spans = Vec::new();
        if !wants.is_empty() {
            let served =
                match served_lines(path, file, meta, &wants, known.as_deref(), security, cancel) {
                    Ok(served) => served,
                    Err(error) if error == "binary content" => return Ok(None),
                    Err(error) => return Err(error),
                };
            known = Some(served.digest);
            spans = served.spans;
        }
        let mut spans = spans.into_iter();
        // The source's own lines, from where the last group ended.
        let mut handle = file;
        handle.seek(SeekFrom::Start(offset)).map_err(failed)?;
        let mut raw = std::io::BufReader::new(handle);
        let mut line = Vec::new();
        for window in group {
            let span = if window.bytes <= room {
                spans.next().flatten()
            } else {
                None
            };
            for number in window.first..=window.last {
                cancel.check()?;
                line.clear();
                let read = raw.read_until(b'\n', &mut line).map_err(failed)?;
                offset += read as u64;
                let text = octocode_engine::text::Decoded::interior(&line).text;
                let text = text.trim_end_matches(['\n', '\r']);
                let shown = span
                    .as_ref()
                    .and_then(|lines| lines.get(number - window.first))
                    .cloned()
                    .unwrap_or_else(|| judged_alone(text, path, security));
                if shown != text {
                    changed.insert(number, shown.replace('\n', " "));
                }
            }
        }
    }
    let weight = changed.values().map(|line| line.len() + 16).sum::<usize>();
    let judged = (changed, known);
    JUDGED.put(key, policy, weight, std::sync::Arc::new(judged.clone()));
    Ok(Some(judged))
}

/// The line windows covering every line of `file`, in order: consecutive
/// lines while they fit `room` bytes, and a longer line alone; with the
/// digest of every byte read.
fn plan(
    file: &std::fs::File,
    room: usize,
    cancel: &impl CancellationCheck,
) -> std::io::Result<(Vec<Planned>, [u8; 32])> {
    let mut hasher = octocode_engine::digest::HashWriter::default();
    let mut handle = file;
    handle.seek(SeekFrom::Start(0))?;
    let mut reader = std::io::BufReader::new(handle);
    let mut planned: Vec<Planned> = Vec::new();
    let (mut line, mut length, mut open) = (1usize, 0usize, false);
    let push = |planned: &mut Vec<Planned>, number: usize, bytes: usize| match planned.last_mut() {
        Some(window)
            if window.last + 1 == number && bytes <= room && window.bytes + bytes <= room =>
        {
            window.last = number;
            window.bytes += bytes;
        }
        _ => planned.push(Planned {
            first: number,
            last: number,
            bytes,
        }),
    };
    loop {
        let piece = reader.fill_buf()?;
        if piece.is_empty() {
            if open {
                push(&mut planned, line, length);
            }
            break;
        }
        let read = piece.len();
        hasher.update(piece);
        let mut rest = piece;
        while let Some(at) = memchr::memchr(b'\n', rest) {
            push(&mut planned, line, length + at + 1);
            line += 1;
            length = 0;
            rest = &rest[at + 1..];
        }
        length += rest.len();
        open = length > 0;
        reader.consume(read);
    }
    // Planning reads every byte without judging any: the judgment's own pass
    // checks the cancel, so a write during it is seen there.
    let _ = cancel;
    Ok((planned, hasher.finalize()))
}

/// `text`, one line no window of the read holds whole, as a read judges it
/// alone: in windows of the scanner's size, each overlapping the next by a
/// quarter, so a value across a window's edge is whole in one of them. A
/// window the scanner refuses is withheld whole; the windows' redactions
/// merge into their union ([`union`]).
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are char boundaries: `boundary` steps back to one, and each redaction starts and ends where the scanner's own text does"
    )
)]
fn judged_alone(text: &str, path: &Path, security: &impl ContentScan) -> String {
    let size = security.limits().window.max(4);
    let overlap = size / 4;
    let boundary = |mut at: usize| {
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let mut hidden: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut start = 0;
    loop {
        let end = boundary((start + size).min(text.len()));
        let end = if end <= start { text.len() } else { end };
        let piece = &text[start..end];
        match security.sanitize(piece, path) {
            Ok((clean, _)) => hidden.extend(
                redactions(piece, &clean)
                    .into_iter()
                    .map(|(range, shown)| (range.start + start..range.end + start, shown)),
            ),
            Err(_) => hidden.push((start..end, crate::security::cut::REDACTED.to_owned())),
        }
        if end >= text.len() {
            break;
        }
        let next = boundary(end.saturating_sub(overlap));
        start = if next > start { next } else { end };
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, shown) in union(hidden) {
        out.push_str(&text[at..range.start]);
        out.push_str(&shown);
        at = range.end;
    }
    out.push_str(&text[at.min(text.len())..]);
    out
}

/// The redactions of overlapping windows as one sorted, disjoint list over the
/// same bytes: a redaction inside another (the same value both windows saw)
/// goes with it; ones that only overlap hide the union of their bytes, showing
/// each placeholder, or one [`crate::security::cut::REDACTED`] when either
/// shows text of the source too. No byte outside every redaction is dropped,
/// and none inside one shows.
fn union(
    mut hidden: Vec<(std::ops::Range<usize>, String)>,
) -> Vec<(std::ops::Range<usize>, String)> {
    hidden.sort_by_key(|(range, _)| (range.start, std::cmp::Reverse(range.end)));
    let mut out: Vec<(std::ops::Range<usize>, String)> = Vec::with_capacity(hidden.len());
    for (range, shown) in hidden {
        match out.last_mut() {
            Some((last, _))
                if range.end <= last.end && (range.start < last.end || range == *last) => {}
            Some((last, text)) if range.start < last.end => {
                last.end = range.end;
                *text = if !placeholders_only(text) || !placeholders_only(&shown) {
                    crate::security::cut::REDACTED.to_owned()
                } else if text.contains(&shown) {
                    std::mem::take(text)
                } else {
                    format!("{text}{shown}")
                };
            }
            _ => out.push((range, shown)),
        }
    }
    out
}

/// Whether `shown` is placeholders alone, no text of the source beside them.
fn placeholders_only(shown: &str) -> bool {
    octocode_engine::security::placeholder_regex()
        .replace_all(shown, "")
        .replace(crate::security::cut::REDACTED, "")
        .is_empty()
}

/// The bytes of a source with each of its changed lines (1-based) replaced by
/// the line a read serves, its own line break kept: what a match of the
/// source runs over, streamed.
pub(crate) struct Patched<'a, R> {
    raw: R,
    changed: &'a ChangedLines,
    line: usize,
    at_start: bool,
    pending: Vec<u8>,
    taken: usize,
}

impl<'a, R: BufRead> Patched<'a, R> {
    pub(crate) fn new(raw: R, changed: &'a ChangedLines) -> Self {
        Self {
            raw,
            changed,
            line: 1,
            at_start: true,
            pending: Vec::new(),
            taken: 0,
        }
    }
}

impl<R: BufRead> Read for Patched<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.taken == self.pending.len() {
            self.pending.clear();
            self.taken = 0;
            if self.at_start
                && let Some(shown) = self.changed.get(&self.line)
            {
                let mut skipped = Vec::new();
                self.raw.read_until(b'\n', &mut skipped)?;
                if skipped.is_empty() {
                    return Ok(0);
                }
                self.pending.extend_from_slice(shown.as_bytes());
                if skipped.ends_with(b"\r\n") {
                    self.pending.extend_from_slice(b"\r\n");
                } else if skipped.ends_with(b"\n") {
                    self.pending.push(b'\n');
                }
                self.line += 1;
            } else {
                let piece = self.raw.fill_buf()?;
                if piece.is_empty() {
                    return Ok(0);
                }
                let take = memchr::memchr(b'\n', piece).map_or(piece.len(), |at| at + 1);
                self.pending.extend_from_slice(&piece[..take]);
                self.raw.consume(take);
                self.at_start = self.pending.last() == Some(&b'\n');
                if self.at_start {
                    self.line += 1;
                }
            }
        }
        let n = buf.len().min(self.pending.len() - self.taken);
        buf[..n].copy_from_slice(&self.pending[self.taken..self.taken + n]);
        self.taken += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
