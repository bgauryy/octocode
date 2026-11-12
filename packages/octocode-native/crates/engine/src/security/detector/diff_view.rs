//! A unified diff judged as the texts it changes. Each line of a hunk starts with
//! its marker column (`+`, `-` or a blank), so a rule that reads indentation (a
//! YAML block scalar's body, the mapping a Kubernetes Secret's `data` is in) or a
//! document's start (`---`) misreads the line by one column, and `+---` starts no
//! document. A hunk also holds two texts: the old one (its `-` and context lines)
//! and the new one (its `+` and context lines), so a fact of one (a manifest's
//! `kind: Secret`) decides nothing on the other. Each side is judged as its own
//! text with the marker column taken out ([`Side`]), and each value found is
//! replaced in the diff's own line: the marker stays, and so does every byte
//! around the value.
use std::ops::Range;

use super::DetectResult;

/// One of the two texts a diff's hunks hold.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Side {
    /// The text before the change: the hunks' `-` and context lines.
    Old,
    /// The text after the change: the hunks' `+` and context lines.
    New,
}

impl Side {
    /// Whether a hunk line with `marker` is a line of this side's text.
    fn holds(self, marker: u8) -> bool {
        match marker {
            b' ' => true,
            b'+' => self == Self::New,
            b'-' => self == Self::Old,
            _ => false,
        }
    }
}

/// Where the view leaves diff bytes out: at view byte `at`, a line start, the
/// diff bytes `skipped` are not in the view. They are the other side's lines
/// before the view's line, then that line's marker.
struct Cut {
    at: usize,
    skipped: Range<usize>,
    /// The diff bytes left out up to and including this cut.
    through: usize,
}

/// One side of a diff: the diff's text with the other side's hunk lines and the
/// marker column of this side's hunk lines taken out.
pub(super) struct DiffView {
    text: String,
    /// Where diff bytes are left out, ascending by view position.
    cuts: Vec<Cut>,
}

/// Whether `line` is a hunk header (`@@ -1,3 +1,4 @@`).
fn is_hunk_header(line: &str) -> bool {
    line.strip_prefix("@@ -")
        .and_then(after_range)
        .and_then(|rest| rest.strip_prefix(" +"))
        .and_then(after_range)
        .is_some_and(|rest| rest.starts_with(" @@"))
}

/// `text` past the hunk range it starts with (`12`, `12,3`).
fn after_range(text: &str) -> Option<&str> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let rest = text.get(digits..).filter(|_| digits > 0)?;
    let Some(count) = rest.strip_prefix(',') else {
        return Some(rest);
    };
    let more = count.bytes().take_while(u8::is_ascii_digit).count();
    count.get(more..).filter(|_| more > 0)
}

/// Whether `line`, in a hunk, is one of its lines: it starts with a marker
/// (`+`, `-`, a blank), is the note that a side ends without a line break
/// (`\ No newline at end of file`), or is empty (a blank context line whose
/// trailing blank a tool trimmed).
fn is_hunk_line(line: &str) -> bool {
    line.is_empty() || line.starts_with(['+', '-', ' ', '\\'])
}

impl DiffView {
    /// The `side` view of `diff`, or `None` when it holds no hunk line. A hunk
    /// runs from its header to the first line that is none of its lines, or
    /// that starts the next file (`--- a/…` with `+++ b/…` after it) or the
    /// next hunk. Every line outside the hunks is in both views.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "offsets are line starts after an ASCII `\\n` and the ASCII marker byte after them"
        )
    )]
    pub(super) fn of(diff: &str, side: Side) -> Option<Self> {
        if !diff.starts_with("@@ -") && !diff.contains("\n@@ -") {
            return None;
        }
        let mut text = String::with_capacity(diff.len());
        let mut cuts: Vec<Cut> = Vec::new();
        let mut hunk_lines = false;
        let mut in_hunk = false;
        let mut start = 0;
        while start < diff.len() {
            let next = diff[start..]
                .find('\n')
                .map_or(diff.len(), |at| start + at + 1);
            let line = &diff[start..next];
            let body = line.trim_end_matches(['\n', '\r']);
            let file_header = body.starts_with("--- ") && diff[next..].starts_with("+++ ");
            // The diff bytes of this line the view leaves out: the whole line
            // of the other side, the marker of this side's line.
            let mut skip = 0;
            if is_hunk_header(body) {
                in_hunk = true;
            } else if in_hunk && is_hunk_line(body) && !file_header {
                if let Some(&marker) = body.as_bytes().first().filter(|m| b"+- ".contains(m)) {
                    hunk_lines = true;
                    skip = if side.holds(marker) { 1 } else { line.len() };
                }
            } else {
                in_hunk = false;
            }
            if skip > 0 {
                let through = cuts.last().map_or(0, |cut| cut.through) + skip;
                match cuts.last_mut() {
                    // The other side's lines, then this line's marker: one cut.
                    Some(cut) if cut.at == text.len() && cut.skipped.end == start => {
                        cut.skipped.end = start + skip;
                        cut.through = through;
                    }
                    _ => cuts.push(Cut {
                        at: text.len(),
                        skipped: start..start + skip,
                        through,
                    }),
                }
            }
            text.push_str(&line[skip..]);
            start = next;
        }
        hunk_lines.then_some(Self { text, cuts })
    }

    /// The text this side's lines are judged as.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The diff bytes left out before view byte `at`; at `at` too when
    /// `inclusive`.
    fn left_out(&self, at: usize, inclusive: bool) -> usize {
        let after = self
            .cuts
            .partition_point(|cut| cut.at < at || (inclusive && cut.at == at));
        after
            .checked_sub(1)
            .and_then(|last| self.cuts.get(last))
            .map_or(0, |cut| cut.through)
    }

    /// Where the view's byte `at` is in the diff, when a range starts there: past
    /// what is left out before a line that starts at `at`.
    fn start_in_diff(&self, at: usize) -> usize {
        at + self.left_out(at, true)
    }

    /// Where a range of the view that ends at `at` ends in the diff: before what
    /// is left out before a line that starts at `at`.
    fn end_in_diff(&self, at: usize) -> usize {
        at + self.left_out(at, false)
    }

    /// `judged`, the redaction of [`DiffView::text`], as the redaction of `diff`:
    /// each replaced range of the view is replaced in the diff, and what the view
    /// left out inside it (the other side's lines, a line's marker), after one of
    /// its line breaks, is written again after that line break of the
    /// replacement, so every line keeps its marker and the other side's lines
    /// stay as written.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "offsets are a redaction's bounds mapped past whole lines and ASCII marker bytes"
        )
    )]
    pub(super) fn restore(&self, diff: &str, judged: DetectResult) -> DetectResult {
        let mut sanitized = String::with_capacity(diff.len());
        let mut replaced: Vec<(Range<usize>, usize)> = Vec::new();
        let mut from = 0;
        for (range, replacement) in judged.edits.runs(&judged.sanitized) {
            let start = self.start_in_diff(range.start).max(from);
            let end = self.end_in_diff(range.end).max(start);
            sanitized.push_str(&diff[from..start]);
            let shown_from = sanitized.len();
            // What the view left out after the hidden text's line breaks, in order.
            let mut kept = self.text[range.clone()].match_indices('\n').map(|(at, _)| {
                let line = range.start + at + 1;
                self.cuts
                    .binary_search_by_key(&line, |cut| cut.at)
                    .ok()
                    .filter(|_| line < range.end)
                    .and_then(|cut| self.cuts.get(cut))
                    .map(|cut| &diff[cut.skipped.clone()])
            });
            for c in replacement.chars() {
                sanitized.push(c);
                if c == '\n'
                    && let Some(Some(left_out)) = kept.next()
                {
                    sanitized.push_str(left_out);
                }
            }
            replaced.push((start..end, sanitized.len() - shown_from));
            from = end;
        }
        sanitized.push_str(&diff[from..]);
        DetectResult {
            sanitized,
            secrets_detected: judged.secrets_detected,
            edits: crate::security::Edits::of_pass(replaced),
        }
    }
}

/// `diff` judged side by side, `new` its new side's view: the new side first,
/// then the old side over what the new side left, each with `judge`; `None`
/// when neither hides anything. A context line is in both sides, so it goes
/// when either hides it.
pub(super) fn redact_sides(
    diff: &str,
    new: &DiffView,
    judge: impl Fn(&str) -> Option<DetectResult>,
) -> Option<DetectResult> {
    let new = judge(new.text()).map(|judged| new.restore(diff, judged));
    let after = new.as_ref().map_or(diff, |new| new.sanitized.as_str());
    let old = DiffView::of(after, Side::Old)
        .and_then(|view| judge(view.text()).map(|judged| view.restore(after, judged)));
    match (new, old) {
        (new, None) => new,
        (None, old) => old,
        (Some(new), Some(old)) => {
            let mut secrets_detected = new.secrets_detected;
            for name in old.secrets_detected {
                if !secrets_detected.contains(&name) {
                    secrets_detected.push(name);
                }
            }
            Some(DetectResult {
                sanitized: old.sanitized,
                secrets_detected,
                edits: new.edits.then(&old.edits),
            })
        }
    }
}
