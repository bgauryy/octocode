//! Check cuts of source text against what sanitizing the source replaced. A cut
//! taken before sanitizing can split a secret so its prefix or key no longer
//! matches a redaction pattern; it shows part of that secret when its bytes
//! overlap a replaced range ([`exposes`]). Position decides, never text: a
//! one-character cut of a secret is part of it even when that character also
//! occurs elsewhere.

use octocode_engine::security::Edits;
use std::ops::Range;

/// A shown line without the clip markers at its ends: a leading `…`, a
/// trailing `…` or `...` truncation suffix.
pub fn strip_clip_markers(line: &str) -> &str {
    let line = line.strip_prefix('…').unwrap_or(line);
    let line = line.strip_suffix("...").unwrap_or(line);
    line.strip_suffix('…').unwrap_or(line)
}

/// What a row shows of the lines it was cut from.
pub enum Cut<'a> {
    /// `text`, starting `offset` bytes into its first line.
    Span { offset: usize, text: &'a str },
    /// This many whole lines, from its first.
    Lines(usize),
}

/// The bytes of `lines` joined by `\n` that `cut`, a row on line `at` (an index
/// of `lines`), shows. A span that is not those lines' bytes at its place is
/// placed on every line it would reach, so it is judged by them whole.
pub fn placed(lines: &[&str], at: usize, cut: &Cut<'_>) -> Range<usize> {
    let start_of = |line: usize| -> usize { lines[..line].iter().map(|text| text.len() + 1).sum() };
    let whole = |count: usize| {
        let last = (at + count.max(1)).min(lines.len()).max(at + 1) - 1;
        start_of(at)..start_of(last) + lines[last].len()
    };
    match *cut {
        Cut::Lines(count) => whole(count),
        Cut::Span { offset, text } => {
            let mut reached = lines[at..].iter().copied();
            let mut joined = String::new();
            if let Some(first) = reached.next() {
                joined.push_str(first);
            }
            for line in reached.take(text.matches('\n').count()) {
                joined.push('\n');
                joined.push_str(line);
            }
            if joined.get(offset..offset + text.len()) == Some(text) {
                let start = start_of(at) + offset;
                start..start + text.len()
            } else {
                whole(text.lines().count())
            }
        }
    }
}

/// Whether a cut of the source at bytes `cut` shows what sanitizing the source
/// replaced (`redacted`): any of its bytes is one a redaction took.
pub fn exposes(redacted: &Edits, cut: &Range<usize>) -> bool {
    redacted.overlaps(cut)
}

/// What a row shows of source bytes `at`, its `text`, judged by the scan that
/// decided it exposed: `text` with each run of `runs` it reaches (source ranges,
/// in `at`'s coordinates, with what replaced them) replaced once: by what the
/// pass wrote for a run inside `bound`, the row's own lines, and by the
/// placeholders that text holds ([`stated`]) for a run that reaches past them,
/// since the text between them is other lines'. Either way a value shows
/// under the placeholder a read of its lines shows, and [`REDACTED`] stands
/// only for a run whose replacement states none. Each byte left shown is
/// checked against `source` (the source's bytes of an unreplaced range), so a
/// row that is not the source's bytes at its place goes whole.
pub fn rendered<'a, 'b>(
    runs: impl IntoIterator<Item = (Range<usize>, &'a str)>,
    text: &str,
    at: &Range<usize>,
    bound: &Range<usize>,
    source: impl Fn(Range<usize>) -> Option<&'b str>,
) -> String {
    if at.len() != text.len() {
        return REDACTED.to_owned();
    }
    // `text`'s bytes `seg`, when they are the source's at their place.
    let kept = |seg: Range<usize>| {
        let shown = text.get(seg.clone())?;
        (seg.is_empty() || source(at.start + seg.start..at.start + seg.end) == Some(shown))
            .then_some(shown)
    };
    let mut out = String::with_capacity(text.len());
    // Bytes of `text` shown or replaced so far.
    let mut from = 0;
    let mut replaced = false;
    for (range, placeholder) in runs {
        if range.end <= at.start || at.end <= range.start {
            continue;
        }
        // Widened to whole characters: a cut can only over-mask.
        let lo = text.floor_char_boundary(range.start.max(at.start) - at.start);
        let hi = text.ceil_char_boundary(range.end.min(at.end) - at.start);
        if replaced && lo < from {
            from = from.max(hi);
            continue;
        }
        let Some(shown) = kept(from..lo) else {
            return REDACTED.to_owned();
        };
        out.push_str(shown);
        let within = bound.start <= range.start && range.end <= bound.end;
        if within && !placeholder.trim().is_empty() {
            out.push_str(placeholder);
        } else {
            stated(placeholder, &mut out);
        }
        (from, replaced) = (hi, true);
    }
    let Some(shown) = kept(from..text.len()) else {
        return REDACTED.to_owned();
    };
    out.push_str(shown);
    out
}

/// Appends the redaction placeholders `replacement` holds, each once and in
/// order, without the text between them: the kinds the pass named its values
/// by. [`REDACTED`] when it holds none.
fn stated(replacement: &str, out: &mut String) {
    let mut kinds: Vec<&str> = Vec::new();
    for found in octocode_engine::security::placeholder_regex().find_iter(replacement) {
        if !kinds.contains(&found.as_str()) {
            kinds.push(found.as_str());
            out.push_str(found.as_str());
        }
    }
    if kinds.is_empty() {
        out.push_str(REDACTED);
    }
}

/// What a span shows for a run whose replacement states no placeholder.
pub const REDACTED: &str = "[REDACTED]";

#[cfg(test)]
mod tests;
