use super::{GAP, SEPARATOR, omission_span, push_numbered};

/// Source lines searched ahead of the last aligned one for a view line.
const VIEW_ALIGN_WINDOW: usize = 1_000;

/// Number a transformed view (`minify:"standard"`) with the source line each
/// of its lines came from, so every kept line can be cited. `source` is the
/// text the view was made from; `source_lines` gives each of its lines' 1-based
/// source number (`0` marks a line-omission marker), or `None` when `source`
/// starts at line 1. Lines align in order on their non-whitespace text: a view
/// line is the start of a source line's remaining text, else inside it. A
/// view line that joins several source lines, or aligns with none, has no
/// single citable number: `None`, and the caller keeps lines whole
/// ([`line_faithful_view`]). Omission markers stay unnumbered.
#[must_use]
pub(crate) fn number_view(
    source: &str,
    source_lines: Option<&[usize]>,
    view: &str,
) -> Option<String> {
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let lines: Vec<(u64, String)> = source
        .lines()
        .enumerate()
        .filter_map(|(index, text)| {
            let line = match source_lines {
                Some(numbers) => *numbers.get(index)?,
                None => index + 1,
            };
            (line != 0).then(|| (line as u64, squash(text)))
        })
        .collect();
    if lines.is_empty() {
        return Some(view.to_owned());
    }
    let mut aligner = ViewAligner {
        lines: &lines,
        cur: 0,
        pos: 0,
        faithful: true,
    };
    let mut output = String::with_capacity(view.len() + view.len() / 8);
    for record in view.split_inclusive('\n') {
        if omission_span(record).is_some() {
            output.push_str(GAP);
            continue;
        }
        let number = aligner.align(&squash(record));
        if !aligner.faithful {
            return None;
        }
        push_numbered(&mut output, number, record);
    }
    Some(output)
}

/// The compact view when the transformed one is not line-faithful: every
/// source line whole under its own number, trailing whitespace trimmed and
/// blank lines left out (the gutter's jump shows them). Omission markers
/// stay unnumbered.
#[must_use]
pub(crate) fn line_faithful_view(source: &str, source_lines: Option<&[usize]>) -> String {
    let mut output = String::with_capacity(source.len());
    for (index, record) in source.lines().enumerate() {
        let line = match source_lines {
            Some(numbers) => numbers.get(index).copied().unwrap_or(0),
            None => index + 1,
        };
        if line == 0 {
            output.push_str(GAP);
            continue;
        }
        let text = record.trim_end();
        if text.trim().is_empty() {
            continue;
        }
        push_numbered(&mut output, line, text);
        output.push('\n');
    }
    output
}

/// In-order alignment state over squashed source lines: the current line and
/// the byte offset already matched in it.
struct ViewAligner<'a> {
    lines: &'a [(u64, String)],
    cur: usize,
    pos: usize,
    /// Every view line so far is one source line's text (or part of it).
    faithful: bool,
}

impl ViewAligner<'_> {
    /// The unmatched rest of line `index`.
    fn rest(&self, index: usize) -> (&str, usize) {
        let text = self.lines[index].1.as_str();
        if index == self.cur {
            (&text[self.pos.min(text.len())..], self.pos.min(text.len()))
        } else {
            (text, 0)
        }
    }

    fn current(&self) -> u64 {
        self.lines[self.cur.min(self.lines.len() - 1)].0
    }

    fn align(&mut self, key: &str) -> u64 {
        let end = self
            .cur
            .saturating_add(VIEW_ALIGN_WINDOW)
            .min(self.lines.len());
        if key.is_empty() {
            // A blank view line: the next blank source line.
            let start = if self.pos > 0 { self.cur + 1 } else { self.cur };
            if let Some(index) = (start..end).find(|&index| self.lines[index].1.is_empty()) {
                self.cur = index + 1;
                self.pos = 0;
                return self.lines[index].0;
            }
            return self.current();
        }
        for index in self.cur..end {
            let (rest, base) = self.rest(index);
            if rest.starts_with(key) {
                return self.take(index, base + key.len());
            }
        }
        for index in self.cur..end {
            let (rest, base) = self.rest(index);
            if let Some(at) = rest.find(key) {
                return self.take(index, base + at + key.len());
            }
        }
        for index in self.cur..end {
            let (rest, _) = self.rest(index);
            if rest.is_empty() || !key.starts_with(rest) {
                continue;
            }
            let mut matched = rest.len();
            for next in index + 1..self.lines.len() {
                let need = &key[matched..];
                let text = self.lines[next].1.as_str();
                if need.starts_with(text) {
                    matched += text.len();
                    if matched == key.len() {
                        let line = self.lines[index].0;
                        self.take(next, text.len());
                        self.faithful = false;
                        return line;
                    }
                } else if text.starts_with(need) {
                    let line = self.lines[index].0;
                    self.take(next, need.len());
                    self.faithful = false;
                    return line;
                } else {
                    break;
                }
            }
        }
        // No source line holds this text: its number would be a guess.
        self.faithful = false;
        self.current()
    }

    /// Consume line `index` up to byte `pos`; its number cites the view line.
    fn take(&mut self, index: usize, pos: usize) -> u64 {
        self.cur = index;
        self.pos = pos;
        self.lines[index].0
    }
}

/// One gutter for every outline: each `N<space>text`, `N\ttext` or `N}` line
/// becomes `N\ttext`. Lines without a leading number stay as they are.
#[must_use]
pub(crate) fn tab_gutter(outline: &str) -> String {
    let mut output = String::with_capacity(outline.len());
    for record in outline.split_inclusive('\n') {
        let digits = record.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || record.starts_with('0') || digits == record.trim_end().len() {
            output.push_str(record);
            continue;
        }
        let (number, rest) = record.split_at(digits);
        output.push_str(number);
        output.push(SEPARATOR);
        output.push_str(rest.strip_prefix([' ', SEPARATOR]).unwrap_or(rest));
    }
    output
}
