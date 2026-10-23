//! TOML tables and top-level keys through the `toml` parser, resynced past
//! the lines it fails on.
use super::Entry;
use crate::tools::line_spans::merge_spans;

/// Parses a malformed TOML source at most this many times: each retry
/// blanks the lines the last parse failed on.
const TOML_RESYNCS: usize = 8;

/// TOML tables (`[a]`, `[[a]]`) through their last value, and the
/// top-level keys before the first table, read with the `toml` parser: text
/// inside a multi-line string or array is never a table. A line the parser
/// fails on is blanked (offsets kept) and the source parsed again, so one
/// broken header does not swallow the tables after it; the second value
/// lists those lines (1-based spans, ascending).
pub(super) fn toml_entries(content: &str) -> (Vec<Entry>, Vec<(usize, usize)>) {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let mut text = std::borrow::Cow::Borrowed(content);
    let mut unparsed: Vec<(usize, usize)> = Vec::new();
    for attempt in 0..=TOML_RESYNCS {
        let (root, errors) = toml::de::DeTable::parse_recoverable(&text);
        let lines = Lines {
            content: &text,
            starts: &starts,
        };
        let failed: Vec<(usize, usize)> = errors
            .iter()
            .filter_map(toml::de::Error::span)
            .map(|span| (lines.line(span.start), lines.last_line(span)))
            .filter(|span| !unparsed.contains(span))
            .collect();
        if failed.is_empty() || attempt == TOML_RESYNCS {
            unparsed.extend(failed);
            return (lines.entries(root.get_ref()), merge_spans(unparsed));
        }
        let mut bytes = text.into_owned().into_bytes();
        for &(first, last) in &failed {
            let from = starts[first - 1];
            let to = starts.get(last).copied().unwrap_or(bytes.len());
            for byte in &mut bytes[from..to] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        unparsed.extend(failed);
        // Whole lines became ASCII spaces: still UTF-8.
        text = std::borrow::Cow::Owned(String::from_utf8(bytes).unwrap_or_default());
    }
    (Vec::new(), merge_spans(unparsed))
}

/// Line numbers of byte offsets in a parsed TOML source.
struct Lines<'a> {
    content: &'a str,
    starts: &'a [usize],
}

type Spanned<'i> = toml::Spanned<toml::de::DeValue<'i>>;

impl Lines<'_> {
    /// The outline entries of a parsed document: its header tables, and
    /// the top-level keys before the first table.
    fn entries(&self, root: &toml::de::DeTable) -> Vec<Entry> {
        let mut entries = Vec::new();
        self.tables(root, &mut entries);
        let first_table = entries.iter().map(|entry| entry.start).min();
        for (key, value) in root {
            let start = self.line(key.span().start);
            if !self.is_header(value) && first_table.is_none_or(|table| start < table) {
                entries.push(Entry {
                    end: self.last_line(value.span()).max(start),
                    ..Entry::at(start, 1)
                });
            }
        }
        entries
    }

    /// The 1-based line holding byte `at`.
    fn line(&self, at: usize) -> usize {
        self.starts.partition_point(|start| *start <= at)
    }

    /// The line of a span's last byte.
    fn last_line(&self, span: std::ops::Range<usize>) -> usize {
        self.line(span.end.saturating_sub(1).max(span.start))
    }

    /// A table opened by a `[header]` (the parser spans such a table by
    /// its header; an implicit or dotted table by its key).
    fn is_header(&self, value: &Spanned) -> bool {
        let header = |value: &Spanned| {
            matches!(value.get_ref(), toml::de::DeValue::Table(_))
                && self.content.as_bytes().get(value.span().start) == Some(&b'[')
        };
        match value.get_ref() {
            toml::de::DeValue::Array(items) => items.first().is_some_and(header),
            _ => header(value),
        }
    }

    /// Every header table under `table`, spanning its header through its
    /// last value.
    fn tables(&self, table: &toml::de::DeTable, entries: &mut Vec<Entry>) {
        for (_, value) in table {
            let items: &[Spanned] = match value.get_ref() {
                toml::de::DeValue::Array(items) => items,
                _ => std::slice::from_ref(value),
            };
            for item in items {
                let toml::de::DeValue::Table(inner) = item.get_ref() else {
                    continue;
                };
                if self.is_header(item) {
                    let start = self.line(item.span().start);
                    entries.push(Entry {
                        end: self.extent(inner).max(start),
                        ..Entry::at(start, 1)
                    });
                }
                self.tables(inner, entries);
            }
        }
    }

    /// The last line of a table's own values (header tables under it are
    /// sections of their own).
    fn extent(&self, table: &toml::de::DeTable) -> usize {
        table
            .iter()
            .filter(|(_, value)| !self.is_header(value))
            .map(|(_, value)| match value.get_ref() {
                toml::de::DeValue::Table(inner) => {
                    self.extent(inner).max(self.last_line(value.span()))
                }
                _ => self.last_line(value.span()),
            })
            .max()
            .unwrap_or(0)
    }
}
