//! `view:"symbols"` for structured data (JSON, YAML, TOML): an outline of
//! the document's keys, as a code outline lists declarations. Each line is
//! the source line that opens a key, with the citation gutter (`N\t`); a key
//! whose value spans lines carries its span (`start-end\t`), so its value is
//! one `ranges` read away. Containers are listed down to [`MAX_DEPTH`] levels
//! of nesting, scalars at the top level only, plus the member that names each
//! object of an array (see `json::identity`).
//!
//! TOML is read with the `toml` parser (its spans place every table; a line
//! it cannot parse is skipped and named, see [`KeyOutline::unparsed`]). JSON and
//! YAML are scanned, never parsed: strings, comments, YAML block scalars and
//! multi-line quoted scalars are skipped, and trailing commas or broken
//! nesting degrade to the nearest container still visible, never to an error.

mod json;
mod toml_tables;
mod yaml;

use json::json_entries;
use toml_tables::toml_entries;
use yaml::yaml_entries;

/// Nesting levels of containers an outline lists.
const MAX_DEPTH: usize = 3;

/// One outlined key: its 1-based line span and nesting depth (1 = top level).
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    start: usize,
    end: usize,
    depth: usize,
    /// Listed even as a one-line value: the member naming an array element.
    names_element: bool,
}

impl Entry {
    fn at(line: usize, depth: usize) -> Self {
        Self {
            start: line,
            end: line,
            depth,
            names_element: false,
        }
    }
}

/// A document's key outline, and the lines its parser skipped.
pub(super) struct KeyOutline {
    pub(super) text: String,
    /// Why the outline may miss keys: the line spans (`"2, 7-9"`) a parser
    /// could not parse and resynchronized after. Only TOML is parsed.
    pub(super) unparsed: Option<String>,
}

/// The key outline of `content`, a document at `path`; `None` for a file
/// that is not JSON, YAML or TOML, or one with no key to list.
pub(super) fn key_outline(content: &str, path: &str) -> Option<KeyOutline> {
    use crate::content::DocumentFormat;
    let (entries, unparsed) = match DocumentFormat::of(path)? {
        DocumentFormat::Json => (json_entries(content), Vec::new()),
        DocumentFormat::Yaml => (yaml_entries(content), Vec::new()),
        DocumentFormat::Toml => toml_entries(content),
        DocumentFormat::Markdown => return None,
    };
    let unparsed = (!unparsed.is_empty()).then(|| {
        unparsed
            .iter()
            .map(|&(start, end)| {
                if start == end {
                    start.to_string()
                } else {
                    format!("{start}-{end}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    });
    Some(KeyOutline {
        text: render(content, entries)?,
        unparsed,
    })
}

/// The outline lines: containers within [`MAX_DEPTH`], top-level scalars and
/// element names, each source line once.
fn render(content: &str, mut entries: Vec<Entry>) -> Option<String> {
    entries.retain(|entry| {
        entry.depth <= MAX_DEPTH
            && (entry.end > entry.start || entry.depth == 1 || entry.names_element)
    });
    entries.sort_by_key(|entry| (entry.start, std::cmp::Reverse(entry.end)));
    entries.dedup_by_key(|entry| entry.start);
    if entries.is_empty() {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let mut out = String::new();
    for entry in entries {
        let Some(text) = lines.get(entry.start - 1) else {
            continue;
        };
        if entry.end > entry.start {
            out.push_str(&format!("{}-{}\t", entry.start, entry.end));
        } else {
            out.push_str(&format!("{}\t", entry.start));
        }
        out.push_str(text);
        out.push('\n');
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests;
