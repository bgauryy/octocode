//! One line longer than a window, judged and served from the source in pieces:
//! no buffer holds the line, only a window of its text and the redactions.
use super::{Shown, judge_windows};
use octocode_engine::text::Charset;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;

/// Source bytes one read of a long line takes: what one piece of its text
/// decodes from.
const PIECE: usize = 256 * 1024;

/// A line too long for a window, as a read serves it: its text, decoded as
/// the whole line decodes ([`octocode_engine::text::Decoded::interior`]),
/// with each redaction's text bytes replaced by what it shows. Only the
/// redactions are kept: the line's own bytes stream from the source again
/// when it is served ([`Splice`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LongLine {
    charset: Charset,
    /// Source bytes of the line's text: the line without the run of line
    /// break bytes it ends with.
    text_bytes: usize,
    /// Source bytes of the whole line, its break included.
    bytes: usize,
    /// Sorted, disjoint byte ranges of the decoded text, with what each shows.
    spans: Vec<(Range<usize>, String)>,
}

impl LongLine {
    /// The text each redaction shows, in order.
    pub(super) fn shown(&self) -> impl Iterator<Item = &str> {
        self.spans.iter().map(|(_, shown)| shown.as_str())
    }

    pub(super) fn weight(&self) -> usize {
        self.spans
            .iter()
            .map(|(_, shown)| shown.len() + 32)
            .sum::<usize>()
            + 48
    }
}

/// How the `bytes` source bytes of a line from `offset` decode as one text,
/// with the source bytes of that text (its break run left out).
fn classify(
    file: &std::fs::File,
    offset: u64,
    bytes: usize,
    cancel: &impl CancellationCheck,
) -> Result<(Charset, usize), String> {
    let failed = |error: std::io::Error| error.to_string();
    let mut handle = file;
    handle.seek(SeekFrom::Start(offset)).map_err(failed)?;
    let mut raw = std::io::BufReader::with_capacity(PIECE, handle.take(bytes as u64));
    let (mut utf8, mut multibyte) = (true, false);
    let mut carry: Vec<u8> = Vec::new();
    let (mut at, mut text_bytes) = (0usize, 0usize);
    loop {
        cancel.check()?;
        let piece = raw.fill_buf().map_err(failed)?;
        let read = piece.len();
        let last = read == 0 || at + read >= bytes;
        if let Some(end) = piece
            .iter()
            .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
        {
            text_bytes = at + end + 1;
        }
        let joined;
        let piece = if carry.is_empty() {
            piece
        } else {
            carry.extend_from_slice(piece);
            joined = std::mem::take(&mut carry);
            &joined[..]
        };
        // Valid UTF-8 is checked whole, the common case; only a line that is
        // not is walked sequence by sequence, and only until it is known to
        // hold a multibyte one (it then decodes lossy, whatever follows).
        let fast = utf8.then(|| std::str::from_utf8(piece));
        match fast {
            Some(Ok(text)) => multibyte |= !text.is_ascii(),
            Some(Err(error)) if error.error_len().is_none() && !last => {
                let (valid, rest) = piece.split_at(error.valid_up_to());
                multibyte |= !valid.is_ascii();
                carry = rest.to_vec();
            }
            _ if !utf8 && multibyte => {}
            _ => {
                let mut chunks = piece.utf8_chunks().peekable();
                while let Some(chunk) = chunks.next() {
                    multibyte |= !chunk.valid().is_ascii();
                    let invalid = chunk.invalid();
                    if invalid.is_empty() {
                        continue;
                    }
                    if chunks.peek().is_none() && !last && incomplete(invalid) {
                        carry = invalid.to_vec();
                    } else {
                        utf8 = false;
                    }
                }
            }
        }
        raw.consume(read);
        at += read;
        if last {
            break;
        }
    }
    let charset = if utf8 {
        Charset::Utf8
    } else if multibyte {
        Charset::Lossy
    } else {
        Charset::Latin1
    };
    Ok((charset, text_bytes))
}

/// Whether `invalid`, a chunk's invalid bytes, is a UTF-8 sequence cut
/// short: the bytes after it may complete it.
fn incomplete(invalid: &[u8]) -> bool {
    std::str::from_utf8(invalid).is_err_and(|error| error.error_len().is_none())
}

/// Decodes a line's bytes piece by piece as `charset` decodes the line
/// whole: a sequence cut by a piece's end waits for the next piece.
struct Decoder {
    charset: Charset,
    carry: Vec<u8>,
}

impl Decoder {
    fn new(charset: Charset) -> Self {
        Self {
            charset,
            carry: Vec::new(),
        }
    }

    /// Append the text of `bytes`, the line's next ones (`last`: its last).
    fn push(&mut self, bytes: &[u8], last: bool, out: &mut String) {
        if self.charset == Charset::Latin1 {
            out.extend(bytes.iter().map(|&byte| char::from(byte)));
            return;
        }
        let joined;
        let bytes = if self.carry.is_empty() {
            bytes
        } else {
            self.carry.extend_from_slice(bytes);
            joined = std::mem::take(&mut self.carry);
            &joined[..]
        };
        // Valid text, the common case, is copied as it is; a sequence cut by
        // the piece's end waits for the next piece.
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                out.push_str(text);
                return;
            }
            Err(error) if error.error_len().is_none() && !last => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                self.carry = rest.to_vec();
                return;
            }
            Err(_) => {}
        }
        let mut chunks = bytes.utf8_chunks().peekable();
        while let Some(chunk) = chunks.next() {
            out.push_str(chunk.valid());
            let invalid = chunk.invalid();
            if invalid.is_empty() {
                continue;
            }
            if chunks.peek().is_none() && !last && incomplete(invalid) {
                self.carry = invalid.to_vec();
            } else {
                out.push(char::REPLACEMENT_CHARACTER);
            }
        }
    }
}

/// The line of `bytes` source bytes at `offset` of `file`, judged alone
/// ([`judge_windows`]) from its text streamed in pieces: `None` when a read
/// serves it as it is.
pub(super) fn judged_long(
    file: &std::fs::File,
    offset: u64,
    bytes: usize,
    path: &Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Option<Shown>, String> {
    let failed = |error: std::io::Error| error.to_string();
    let (charset, text_bytes) = classify(file, offset, bytes, cancel)?;
    let mut handle = file;
    handle.seek(SeekFrom::Start(offset)).map_err(failed)?;
    // Pieces no larger than a window: the text held is at most two windows.
    let piece = PIECE.min(security.limits().window.max(4));
    let mut raw = std::io::BufReader::with_capacity(piece, handle.take(text_bytes as u64));
    let mut decoder = Decoder::new(charset);
    let mut left = text_bytes;
    let spans = judge_windows(
        |text: &mut String| {
            let piece = raw.fill_buf().map_err(failed)?;
            let read = piece.len().min(left);
            left -= read;
            let last = read == 0 || left == 0;
            decoder.push(&piece[..read], last, text);
            raw.consume(read);
            Ok(!last)
        },
        path,
        security,
        cancel,
    )?;
    Ok((!spans.is_empty()).then(|| {
        Shown::Long(LongLine {
            charset,
            text_bytes,
            bytes,
            spans: spans
                .into_iter()
                .map(|(range, shown)| (range, shown.replace('\n', " ")))
                .collect(),
        })
    }))
}

/// A long line being served ([`super::Patched`]): its text decoded from the
/// source piece by piece, each redaction's bytes replaced by what it shows,
/// then its own line break.
pub(super) struct Splice<'a> {
    line: &'a LongLine,
    decoder: Decoder,
    /// Text source bytes not read yet, then break bytes not read yet.
    text_left: usize,
    break_left: usize,
    /// The decoded text's offset the next piece starts at.
    at: usize,
    /// The next redaction, and whether its text was shown.
    span: usize,
    shown: bool,
    /// The line's last two bytes: its break as the source wrote it.
    tail: [u8; 2],
    piece: String,
}

impl<'a> Splice<'a> {
    pub(super) fn new(line: &'a LongLine) -> Self {
        Self {
            line,
            decoder: Decoder::new(line.charset),
            text_left: line.text_bytes,
            break_left: line.bytes.saturating_sub(line.text_bytes),
            at: 0,
            span: 0,
            shown: false,
            tail: [0; 2],
            piece: String::new(),
        }
    }

    /// Serve the line's next bytes into `out`; false once it ended (its
    /// break was written).
    pub(super) fn step(
        &mut self,
        raw: &mut impl BufRead,
        out: &mut Vec<u8>,
    ) -> std::io::Result<bool> {
        if self.text_left > 0 {
            let piece = raw.fill_buf()?;
            let read = piece.len().min(self.text_left);
            if read == 0 {
                // The source ended early: what was decoded is the line.
                self.text_left = 0;
                self.break_left = 0;
            } else {
                self.text_left -= read;
                self.remember(&piece[..read]);
            }
            let mut text = std::mem::take(&mut self.piece);
            text.clear();
            self.decoder
                .push(&piece[..read], self.text_left == 0, &mut text);
            raw.consume(read);
            self.splice(text.as_bytes(), out);
            self.piece = text;
            return Ok(true);
        }
        if self.break_left > 0 {
            let piece = raw.fill_buf()?;
            let read = piece.len().min(self.break_left);
            self.break_left = if read == 0 { 0 } else { self.break_left - read };
            self.remember(&piece[..read]);
            raw.consume(read);
            return Ok(true);
        }
        // Redactions past the text's end (it ended early) still show.
        for (index, (_, shown)) in self.line.spans.iter().enumerate().skip(self.span) {
            if index > self.span || !self.shown {
                out.extend_from_slice(shown.as_bytes());
            }
        }
        self.span = self.line.spans.len();
        if self.tail == *b"\r\n" {
            out.extend_from_slice(b"\r\n");
        } else if self.tail[1] == b'\n' {
            out.push(b'\n');
        }
        Ok(false)
    }

    fn remember(&mut self, bytes: &[u8]) {
        match bytes {
            [] => {}
            [only] => self.tail = [self.tail[1], *only],
            [.., before, last] => self.tail = [*before, *last],
        }
    }

    /// Write `text`, the decoded bytes from `self.at`, with each redaction's
    /// bytes replaced by its text, written once where it starts.
    fn splice(&mut self, text: &[u8], out: &mut Vec<u8>) {
        let (from, to) = (self.at, self.at + text.len());
        let mut at = from;
        while at < to {
            match self.line.spans.get(self.span) {
                Some((range, shown)) if range.start <= at => {
                    if !self.shown {
                        out.extend_from_slice(shown.as_bytes());
                        self.shown = true;
                    }
                    if range.end <= at {
                        self.span += 1;
                        self.shown = false;
                    } else {
                        at = range.end.min(to);
                    }
                }
                Some((range, _)) => {
                    let stop = range.start.min(to);
                    out.extend_from_slice(&text[at - from..stop - from]);
                    at = stop;
                }
                None => {
                    out.extend_from_slice(&text[at - from..]);
                    at = to;
                }
            }
        }
        // A redaction ending where this piece ends is done.
        while let Some((range, _)) = self.line.spans.get(self.span) {
            if range.start < to && range.end <= to && self.shown {
                self.span += 1;
                self.shown = false;
            } else {
                break;
            }
        }
        self.at = to;
    }
}
