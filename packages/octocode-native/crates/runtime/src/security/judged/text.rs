//! One source judged whole, and every fragment a tool shows of it.
//!
//! The rule: a source's redaction is decided once, over its whole text with
//! its path (the pass localFetch serves:
//! [`super::super::private_key_blocks_hidden`], then
//! [`ContentSecurity::sanitize_text`]). A tool that returns text of that
//! source (a search row, an AST node, an LSP snippet, a graph fact) cuts it
//! from the [`JudgedText`] by position and never judges the fragment alone:
//! alone, a value whose secrecy comes from a key on another line reads as
//! data, and ordinary code reads as configuration. Position decides, never
//! text ([`crate::security::cut`]).
//!
//! A request keeps each source it judged ([`JudgedFiles`]), so many rows of
//! one file cost one scan, and the texts it showed, so the output gate honors
//! them as judged ([`super::JudgedLeaves::shown`]).
use crate::security::ContentSecurity;
use crate::security::cut::{REDACTED, rendered};
use octocode_engine::digest::sha256_digest;
use octocode_engine::security::Edits;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// Characters of a redacted value from which [`JudgedText::scrubbed`]
/// replaces it wherever it occurs: the length the output gate propagates a
/// redacted value from (shorter ones are too generic to be the same secret
/// wherever their letters recur).
const SCRUBBED_ANYWHERE: usize = 8;

/// Byte offsets each line of a text starts at, and the text's length.
struct LineStarts(Vec<usize>, usize);

impl LineStarts {
    fn of(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|at| at + 1));
        if starts.last() == Some(&text.len()) && starts.len() > 1 {
            starts.pop();
        }
        Self(starts, text.len())
    }

    /// Bytes of the 1-based lines `first..first + count`, line breaks between
    /// them included, the last one's left out.
    fn span(&self, text: &str, first: usize, count: usize) -> Option<Range<usize>> {
        let start = *self.0.get(first.checked_sub(1)?)?;
        let last = (first - 1 + count.max(1)).min(self.0.len());
        let end = self.0.get(last).copied().unwrap_or(self.1);
        let body = text.get(start..end)?;
        let body = body.strip_suffix('\n').unwrap_or(body);
        let body = body.strip_suffix('\r').unwrap_or(body);
        Some(start..start + body.len())
    }

    /// The 1-based line holding byte `at`.
    fn line_of(&self, at: usize) -> usize {
        self.0.partition_point(|start| *start <= at).max(1)
    }
}

/// The line starts of `clean`, the text a pass made of `raw`, when it kept the
/// line count: line N of one is then line N of the other.
fn aligned_lines(raw: &str, clean: &str) -> Option<LineStarts> {
    let lines = |text: &str| text.split_inclusive('\n').count();
    (lines(clean) == lines(raw)).then(|| LineStarts::of(clean))
}

/// The byte of `line` its 0-based `column` names, counted in bytes, in
/// characters and in UTF-16 units; `None` for a unit whose count does not end
/// on a character of the line (its end included).
fn column_starts(line: &str, column: usize) -> [Option<usize>; 3] {
    let (mut by_chars, mut by_units) = (None, None);
    let (mut chars, mut units) = (0, 0);
    for (at, c) in line
        .char_indices()
        .map(|(at, c)| (at, Some(c)))
        .chain([(line.len(), None)])
    {
        if chars == column {
            by_chars = Some(at);
        }
        if units == column {
            by_units = Some(at);
        }
        chars += 1;
        units += c.map_or(1, char::len_utf16);
    }
    [
        line.is_char_boundary(column).then_some(column),
        by_chars,
        by_units,
    ]
}

/// A source's decoded text judged whole at its path: what the pass replaced,
/// by position. The only source of fragments a tool shows of that source.
pub(crate) struct JudgedText {
    path: PathBuf,
    raw: String,
    raw_lines: LineStarts,
    /// The text the pass made of `raw` and what it replaced by position;
    /// `None` when it replaced nothing. The line starts of that text when it
    /// keeps the line count (it is then the text localFetch serves, line for
    /// line); `None` for a pass that joined lines, whose lines are shown by
    /// position instead.
    redacted: Option<(String, Edits, Option<LineStarts>)>,
}

impl JudgedText {
    /// `raw` (a source's decoded text, the one its tool's positions count)
    /// judged whole as the file at `path`. A pass that changed the text
    /// without saying where withholds every line, as a read does
    /// ([`crate::security::scan::redact_lines`]).
    pub(crate) fn judge(raw: &str, path: &Path, security: &ContentSecurity) -> Self {
        #[cfg(test)]
        JUDGED_BYTES.with(|judged| judged.set(judged.get() + raw.len()));
        let (keyed, keys) = super::super::private_key_blocks_hidden(raw);
        let scanned = security.sanitize_text(&keyed, Some(path));
        let changed = !keys.is_empty() || scanned.has_secrets || scanned.content != keyed;
        let redacted = changed.then(|| {
            let edits = keys.then(&scanned.edits);
            if edits.is_empty() {
                let withheld = crate::security::scan::withheld_lines(raw);
                let edits = Edits::of_pass([(0..raw.len(), withheld.len())]);
                let starts = LineStarts::of(&withheld);
                return (withheld, edits, Some(starts));
            }
            let clean = scanned.content.into_owned();
            let starts = aligned_lines(raw, &clean);
            (clean, edits, starts)
        });
        Self {
            path: path.to_path_buf(),
            raw_lines: LineStarts::of(raw),
            raw: raw.to_owned(),
            redacted,
        }
    }

    /// What the pass made of the source: the text a read serves and what it
    /// replaced by position; `None` when it replaced nothing. A stored scan
    /// keeps it for its later pages ([`Self::judged_as`]).
    pub(crate) fn redaction(&self) -> Option<(&str, &Edits)> {
        let (clean, edits, _) = self.redacted.as_ref()?;
        Some((clean, edits))
    }

    /// `raw` as the file at `path`, judged by a pass a caller kept the
    /// outcome of ([`Self::redaction`]) for this exact text and path: no scan
    /// runs again.
    pub(crate) fn judged_as(raw: &str, path: &Path, redaction: Option<(&str, &Edits)>) -> Self {
        Self {
            path: path.to_path_buf(),
            raw_lines: LineStarts::of(raw),
            raw: raw.to_owned(),
            redacted: redaction
                .map(|(clean, edits)| (clean.to_owned(), edits.clone(), aligned_lines(raw, clean))),
        }
    }

    /// Bytes this judgment holds, for a store that keeps it.
    pub(crate) fn weight(&self) -> usize {
        let starts = |lines: &LineStarts| lines.0.len() * std::mem::size_of::<usize>();
        let redacted = self.redacted.as_ref().map_or(0, |(clean, edits, lines)| {
            clean.len() + edits.heap_bytes() + lines.as_ref().map_or(0, starts)
        });
        std::mem::size_of::<Self>()
            + self.path.as_os_str().len()
            + self.raw.len()
            + starts(&self.raw_lines)
            + redacted
    }

    /// The path this source was judged at.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The decoded text positions count.
    pub(crate) fn raw(&self) -> &str {
        &self.raw
    }

    /// Whether the pass replaced anything of the source.
    pub(crate) fn is_redacted(&self) -> bool {
        self.redacted.is_some()
    }

    /// The source bytes of the 1-based lines `first..first + count`, without
    /// the last one's line break; `None` past the end.
    pub(crate) fn line_span(&self, first: usize, count: usize) -> Option<Range<usize>> {
        self.raw_lines.span(&self.raw, first, count)
    }

    /// The 1-based lines `first..first + count` as a read of the source
    /// serves them (the last one's line break left out); `None` past the end.
    /// Where the pass joined lines, each replaced run those lines hold is
    /// shown by position ([`Self::cut`]).
    pub(crate) fn lines(&self, first: usize, count: usize) -> Option<Cow<'_, str>> {
        let span = self.line_span(first, count)?;
        match &self.redacted {
            None => self.raw.get(span).map(Cow::Borrowed),
            Some((clean, _, Some(starts))) => clean
                .get(starts.span(clean, first, count)?)
                .map(Cow::Borrowed),
            Some((clean, edits, None)) => {
                let text = self.raw.get(span.clone())?;
                if !edits.overlaps(&span) {
                    return Some(Cow::Borrowed(text));
                }
                Some(Cow::Owned(rendered(
                    edits.runs(clean),
                    text,
                    &span,
                    &span,
                    |at| self.raw.get(at),
                )))
            }
        }
    }

    /// Whether source bytes `range` hold any byte the pass replaced.
    pub(crate) fn exposes(&self, range: &Range<usize>) -> bool {
        self.redacted
            .as_ref()
            .is_some_and(|(_, edits, _)| edits.overlaps(range))
    }

    /// What a tool shows of source bytes `range`: the bytes themselves where
    /// the pass replaced none of them; whole lines as a read serves them; any
    /// other cut with each replaced run it reaches shown as its placeholder.
    /// A range that is not the source's bytes shows nothing of it.
    pub(crate) fn cut(&self, range: Range<usize>) -> Cow<'_, str> {
        let Some(text) = self.raw.get(range.clone()) else {
            return Cow::Borrowed(REDACTED);
        };
        let Some((clean, edits, _)) = &self.redacted else {
            return Cow::Borrowed(text);
        };
        if !edits.overlaps(&range) {
            return Cow::Borrowed(text);
        }
        let first = self.raw_lines.line_of(range.start);
        let last = self
            .raw_lines
            .line_of(range.end.saturating_sub(1).max(range.start));
        let bound = self
            .line_span(first, last - first + 1)
            .unwrap_or_else(|| range.clone());
        if bound == range
            && let Some(lines) = self.lines(first, last - first + 1)
        {
            return lines;
        }
        Cow::Owned(rendered(edits.runs(clean), text, &range, &bound, |at| {
            self.raw.get(at)
        }))
    }

    /// `text`, a fact a tool derived from this source with no place of its
    /// own (a name stored at build time, a string a server computed), with
    /// every value the pass replaced shown as its placeholder; `None` when
    /// it holds none. A value of [`SCRUBBED_ANYWHERE`] characters or more is
    /// replaced wherever it occurs; a shorter one only where it stands as a
    /// whole token, so a short secret does not blank the identifiers that
    /// contain its letters.
    pub(crate) fn scrubbed(&self, text: &str) -> Option<String> {
        let (clean, edits, _) = self.redacted.as_ref()?;
        let mut values = edits
            .runs(clean)
            .filter_map(|(at, placeholder)| {
                let value = self.raw.get(at)?.trim();
                let placeholder = if placeholder.trim().is_empty() {
                    REDACTED
                } else {
                    placeholder
                };
                value
                    .chars()
                    .any(char::is_alphanumeric)
                    .then_some((value, placeholder))
            })
            .collect::<Vec<_>>();
        // Longest first, so a value never leaves the tail of a longer one.
        values.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
        values.dedup_by(|a, b| a.0 == b.0);
        let mut out: Option<String> = None;
        for (value, placeholder) in values {
            let current = out.as_deref().unwrap_or(text);
            if !current.contains(value) {
                continue;
            }
            let anywhere = value.chars().count() >= SCRUBBED_ANYWHERE;
            let mut next = String::with_capacity(current.len());
            let mut rest = current;
            let mut changed = false;
            while let Some(at) = rest.find(value) {
                let (before, found) = rest.split_at(at);
                let after = found.get(value.len()..).unwrap_or_default();
                let word = |c: char| c.is_alphanumeric() || c == '_';
                let token = !before.chars().next_back().is_some_and(word)
                    && !after.chars().next().is_some_and(word);
                next.push_str(before);
                if anywhere || token {
                    next.push_str(placeholder);
                    changed = true;
                } else {
                    next.push_str(value);
                }
                rest = after;
            }
            next.push_str(rest);
            if changed {
                out = Some(next);
            }
        }
        out
    }

    /// Where `text`, a fragment whose first byte is on the 1-based `line`,
    /// sits in the source. `column` is its 0-based column there in the
    /// tool's own unit: the place is the bytes at that column, counted in
    /// bytes, characters or UTF-16 units (whichever holds the text there),
    /// and never another occurrence of the text, so a name is not mistaken
    /// for the same letters inside a value. Only a fragment that states no
    /// column (a value a tool shows without its place) is searched for on
    /// the line: an occurrence the pass replaced part of decides before one
    /// it left whole, so the fragment never shows a value the pass hid.
    pub(crate) fn place(
        &self,
        line: usize,
        column: Option<usize>,
        text: &str,
    ) -> Option<Range<usize>> {
        let on_line = self.line_span(line, 1)?;
        let head = self.raw.get(on_line.clone())?;
        if let Some(column) = column {
            let held = |start: usize| {
                let at = on_line.start + start..on_line.start + start + text.len();
                (self.raw.get(at.clone()) == Some(text)).then_some(at)
            };
            return column_starts(head, column)
                .into_iter()
                .flatten()
                .find_map(held);
        }
        let reach = (on_line.end + text.len()).min(self.raw.len());
        let body = self.raw.as_bytes().get(on_line.start..reach)?;
        let mut found = memchr::memmem::find_iter(body, text.as_bytes())
            .filter(|at| *at <= on_line.len())
            .map(|at| on_line.start + at..on_line.start + at + text.len())
            .filter(|at| self.raw.get(at.clone()).is_some())
            .collect::<Vec<_>>();
        let exposed = found.iter().position(|at| self.exposes(at)).unwrap_or(0);
        (!found.is_empty()).then(|| found.swap_remove(exposed))
    }

    /// What a tool shows of `text`, a fragment it holds of this source that
    /// starts on the 1-based `line` (see [`Self::place`]): the [`Self::cut`]
    /// of its bytes, and `true`. A fragment that is not the source's bytes
    /// there (the file changed, or the text is a view of it) is judged by
    /// every line it would reach: those lines as a read serves them where the
    /// pass replaced anything on them, else the text as it is and `false`,
    /// for the output gate to judge.
    pub(crate) fn fragment<'a>(
        &'a self,
        line: usize,
        column: Option<usize>,
        text: &'a str,
    ) -> (Cow<'a, str>, bool) {
        if let Some(at) = self.place(line, column, text) {
            return (self.cut(at), true);
        }
        let reached = text.split('\n').count();
        match self.line_span(line, reached) {
            Some(lines) if !self.exposes(&lines) => (Cow::Borrowed(text), false),
            _ => (
                self.lines(line, reached).unwrap_or(Cow::Borrowed(REDACTED)),
                true,
            ),
        }
    }

    /// What a tool shows of `text`, a fragment it holds from somewhere in
    /// source bytes `within` with no place of its own (an unranged capture):
    /// the cut of an occurrence there, one the pass replaced part of first.
    /// `None` when `within` does not hold it.
    pub(crate) fn fragment_within(
        &self,
        within: &Range<usize>,
        text: &str,
    ) -> Option<Cow<'_, str>> {
        let body = self.raw.as_bytes().get(within.clone())?;
        let mut found = memchr::memmem::find_iter(body, text.as_bytes())
            .map(|at| within.start + at..within.start + at + text.len())
            .filter(|at| self.raw.get(at.clone()).is_some());
        let first = found.next()?;
        let at = if self.exposes(&first) {
            first
        } else {
            found.find(|at| self.exposes(at)).unwrap_or(first)
        };
        Some(self.cut(at))
    }
}

/// A kept judgment: the digest of the text judged, and the judgment.
type KeptSource = ([u8; 32], Arc<JudgedText>);

/// What one request judged whole and showed: the sources its rows cut their
/// text from, each judged once, and the texts it showed of them. A source is
/// kept under its path and the digest of the text judged, so a file that
/// changed between two rows is judged again.
#[derive(Default)]
pub(crate) struct JudgedFiles {
    files: Mutex<HashMap<PathBuf, KeptSource>>,
    /// Paths judged whole without being kept ([`Self::judged_whole`]).
    whole: Mutex<std::collections::HashSet<PathBuf>>,
    shown: Mutex<HashMap<[u8; 32], String>>,
}

impl JudgedFiles {
    /// `raw`, the decoded text of the file at `path`, judged whole: the
    /// judgment this request already made of the same text, else a new one,
    /// kept for the request's other rows of that file.
    pub(crate) fn judge(
        &self,
        raw: &str,
        path: &Path,
        security: &ContentSecurity,
    ) -> Arc<JudgedText> {
        let digest = sha256_digest(raw.as_bytes());
        let mut files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((known, judged)) = files.get(path)
            && *known == digest
        {
            return Arc::clone(judged);
        }
        let judged = Arc::new(JudgedText::judge(raw, path, security));
        files.insert(path.to_path_buf(), (digest, Arc::clone(&judged)));
        judged
    }

    /// Notes that this request judged the file at `path` whole and cut its
    /// rows from that judgment, without keeping the source: a tool that
    /// settles all of a file's rows in one pass holds nothing of it after.
    pub(crate) fn judged_whole(&self, path: &Path) {
        self.whole
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_path_buf());
    }

    /// Whether this request judged the file at `path` whole.
    pub(crate) fn is_judged(&self, path: &Path) -> bool {
        let whole = self.whole.lock().unwrap_or_else(PoisonError::into_inner);
        whole.contains(path)
            || self
                .files
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(path)
    }

    /// Records `text` as a leaf this request shows of the source judged at
    /// `path`: built only from [`JudgedText::cut`]s and [`JudgedText::lines`]
    /// of it and what the runtime writes around them (line numbers, clip
    /// markers, joins). The output gate judges such a leaf as a piece of its
    /// judged source.
    pub(crate) fn show(&self, path: &Path, text: &str) {
        if text.is_empty() {
            return;
        }
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                sha256_digest(text.as_bytes()),
                path.to_string_lossy().into_owned(),
            );
    }

    /// The texts this request showed, for the output gate.
    pub(crate) fn leaves(&self) -> super::JudgedLeaves {
        let shown = std::mem::take(&mut *self.shown.lock().unwrap_or_else(PoisonError::into_inner));
        super::JudgedLeaves::shown(shown)
    }
}

impl std::fmt::Debug for JudgedFiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JudgedFiles")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
thread_local! {
    /// Source bytes this thread judged whole, for the tests that count scans.
    pub(crate) static JUDGED_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
