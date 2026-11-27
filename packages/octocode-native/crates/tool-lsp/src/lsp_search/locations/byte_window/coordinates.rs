//! Source line, UTF-16 column, and redacted-view byte coordinates.

/// Byte range of zero-based lines `first..=last` of `text` (each with its
/// `\n`), as localFetch's line ranges select them.
pub(super) fn line_span(text: &str, first: usize, last: usize) -> Option<std::ops::Range<usize>> {
    if first > last {
        return None;
    }
    let after_line = |line: usize| -> Option<usize> {
        let mut at = 0;
        for _ in 0..=line {
            if at >= text.len() {
                return None;
            }
            at = text
                .get(at..)?
                .find('\n')
                .map_or(text.len(), |end| at + end + 1);
        }
        Some(at)
    };
    let start = match first {
        0 => 0,
        line => after_line(line - 1)?,
    };
    (start < text.len()).then_some(start..after_line(last)?)
}

/// Byte of zero-based UTF-16 `column` on the first line of `text`, clamped
/// to that line.
pub(super) fn raw_byte_of_column(text: &str, column: usize) -> usize {
    let mut units = 0;
    for (at, ch) in text.char_indices() {
        if units >= column || ch == '\n' || ch == '\r' {
            return at;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Where raw byte `at` stands in `view`, the same lines with secrets
/// replaced by redaction markers. The two texts agree byte for byte between
/// markers, so `at` maps exactly there; raw text a marker replaced maps to
/// the marker's start, so the window shows the marker. After each marker the
/// raw text resumes where the view's next stretch (up to the next marker)
/// starts again; a change that is no marker, or a stretch that cannot be
/// found, maps to the first changed character. A non-boundary or out-of-range
/// input is refused; every result is a character boundary of `view`.
pub(super) fn view_offset(raw: &str, view: &str, at: usize) -> Option<usize> {
    use octocode_engine::security::{CONTENT_REDACTION_PREFIX, REDACTION_PREFIX};
    raw.get(..at)?;
    let next_marker = |from: usize| {
        let remaining = view.get(from..)?;
        Some(
            [REDACTION_PREFIX, CONTENT_REDACTION_PREFIX]
                .iter()
                .filter_map(|prefix| remaining.find(prefix).map(|found| from + found))
                .min()
                .unwrap_or(view.len()),
        )
    };
    let (mut i, mut j) = (0, 0);
    loop {
        // Equal byte prefixes can stop inside different multibyte characters.
        let same = raw
            .get(i..)?
            .chars()
            .zip(view.get(j..)?.chars())
            .take_while(|(a, b)| a == b)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        if at <= i + same {
            return Some(j + (at - i));
        }
        (i, j) = (i + same, j + same);
        if j >= view.len() {
            return Some(view.len());
        }
        // The marker covering the first changed byte: it may share leading
        // bytes with the raw text it replaced.
        let Some(open) = marker_at(view, j) else {
            return Some(j);
        };
        let close = view
            .get(open..)?
            .find(']')
            .map_or(view.len(), |end| open + end + 1);
        let stretch = view.get(close..next_marker(close)?)?;
        let resume = if stretch.is_empty() {
            (close == view.len()).then_some(raw.len())
        } else {
            raw.get(i..)?.find(stretch).map(|found| i + found)
        };
        match resume {
            Some(resume) if at >= resume => (i, j) = (resume, close),
            _ => return Some(open),
        }
    }
}

/// Start of the redaction marker that byte `at` of `view` opens or falls
/// inside, looking back no further than its line.
fn marker_at(view: &str, at: usize) -> Option<usize> {
    use octocode_engine::security::{CONTENT_REDACTION_PREFIX, REDACTION_PREFIX};
    let bytes = view.as_bytes();
    let open = (0..=at.min(bytes.len().saturating_sub(1)))
        .rev()
        .find(|&k| matches!(bytes[k], b'[' | b']' | b'\n'))?;
    let remaining = view.get(open..)?;
    (bytes[open] == b'['
        && (remaining.starts_with(REDACTION_PREFIX)
            || remaining.starts_with(CONTENT_REDACTION_PREFIX)))
    .then_some(open)
}
