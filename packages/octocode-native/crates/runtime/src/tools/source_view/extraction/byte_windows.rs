//! Byte windows around match spans, for sources whose lines are too long
//! to show whole.
use super::OMISSION_LINE;

/// Byte windows of `bytes` around each match span, with `... [N bytes
/// omitted] ...` between them (and, for long lines, before the first and
/// after the last): the text and the source line of every view line.
pub(crate) fn byte_windows(
    content: &str,
    spans: Vec<(usize, usize)>,
    bytes: usize,
) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut last_end = 0;
    let mut selected = Vec::new();
    let starts = octocode_engine::text::line_starts(content);
    for (start, end) in spans {
        let mut a = content.ceil_char_boundary(start.saturating_sub(bytes));
        let b = content.ceil_char_boundary((end + bytes).min(content.len()));
        // Overlapping windows continue from the previous end; re-emitting
        // the overlap would fabricate text that is not in the source.
        if !text.is_empty() {
            a = a.max(last_end)
        }
        if a >= b {
            continue;
        }
        if !text.is_empty() && a > last_end {
            if !text.ends_with('\n') {
                text.push('\n')
            }
            text.push_str(&format!("... [{} bytes omitted] ...\n", a - last_end));
            selected.push(OMISSION_LINE);
        } else if text.is_empty() {
            // The first window starting mid-line says what precedes it.
            let line_start = content[..a].rfind('\n').map_or(0, |at| at + 1);
            if a > line_start {
                text.push_str(&format!("... [{} bytes omitted] ...\n", a - line_start));
                selected.push(OMISSION_LINE);
            }
        }
        let mut source_line = octocode_engine::text::line_at(&starts, a);
        for record in content[a..b].split_inclusive('\n') {
            if text.is_empty() || text.ends_with('\n') {
                selected.push(source_line);
            }
            text.push_str(record);
            source_line += usize::from(record.ends_with('\n'));
        }
        last_end = b
    }
    // The last window ending mid-line says what follows it.
    let line_end = content[last_end..]
        .find('\n')
        .map_or(content.len(), |at| last_end + at);
    if !text.is_empty() && line_end > last_end {
        if !text.ends_with('\n') {
            text.push('\n')
        }
        text.push_str(&format!(
            "... [{} bytes omitted] ...\n",
            line_end - last_end
        ));
        selected.push(OMISSION_LINE);
    }
    (text, selected)
}
