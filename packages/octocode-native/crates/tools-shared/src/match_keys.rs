//! Key paths of matched lines in JSON and TOML documents (`matchKeys`).
//!
//! A hit in a config file shows the line, not the section it sits in:
//! `"vite": "^7"` reads the same under `dependencies` and `devDependencies`,
//! and `serde = "1"` the same under `[dependencies]` and
//! `[dev-dependencies]`. Each scanner walks the source once, line by line,
//! and names each requested line's key path; neither parses, so broken
//! documents degrade to the nearest container still visible.
use serde::Serialize;
use std::path::Path;

mod json;
mod toml;

/// Extensions read as JSON documents.
const JSON_EXTENSIONS: [&str; 3] = ["json", "jsonc", "json5"];

/// The document kind of `path`, by extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Json,
    Toml,
}

fn kind(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?;
    if JSON_EXTENSIONS
        .iter()
        .any(|json| extension.eq_ignore_ascii_case(json))
    {
        Some(Kind::Json)
    } else if extension.eq_ignore_ascii_case("toml") {
        Some(Kind::Toml)
    } else {
        None
    }
}

/// Whether `path` names a document whose hit lines get key paths.
pub fn is_keyed_path(path: &Path) -> bool {
    kind(path).is_some()
}

/// The key path of each of `lines` (1-based, ascending) in `text`, a
/// document at `path`; empty for any other file. Lines at the document root
/// are left out.
pub fn line_key_paths(path: &Path, text: &str, lines: &[usize]) -> Vec<(usize, String)> {
    match kind(path) {
        Some(Kind::Json) => json::line_key_paths(text, lines),
        Some(Kind::Toml) => toml::line_key_paths(text, lines),
        None => Vec::new(),
    }
}

/// What a scan of a document tells about each requested line, in order.
pub trait LineVisitor {
    /// Line `line` (1-based) begins inside the container at `container`
    /// (empty: the document root).
    fn line(&mut self, line: usize, container: &str);
    /// A member or element at `path` starts on the current line; `false`
    /// when no later start on it is wanted.
    fn start(&mut self, path: &str) -> bool;
}

/// Tells `visitor`, for each of `lines` (1-based, ascending) of `text`, a
/// document at `path`, the container the line begins in and each member or
/// element that starts on it, in order; nothing for any other file.
pub fn visit_lines(path: &Path, text: &str, lines: &[usize], visitor: &mut dyn LineVisitor) {
    match kind(path) {
        Some(Kind::Json) => json::visit_lines(text, lines, visitor),
        Some(Kind::Toml) => toml::visit_lines(text, lines, visitor),
        None => {}
    }
}

/// Each line's key path: the member or element that starts first on it,
/// else the container it sits in; root lines left out.
#[derive(Default)]
struct FirstKey {
    paths: Vec<(usize, String)>,
    open: Option<(usize, String)>,
}

impl FirstKey {
    fn flush(&mut self) {
        if let Some((line, path)) = self.open.take()
            && !path.is_empty()
        {
            self.paths.push((line, path));
        }
    }

    fn into_paths(mut self) -> Vec<(usize, String)> {
        self.flush();
        self.paths
    }
}

impl LineVisitor for FirstKey {
    fn line(&mut self, line: usize, container: &str) {
        self.flush();
        self.open = Some((line, container.to_owned()));
    }

    fn start(&mut self, path: &str) -> bool {
        if let Some((_, open)) = self.open.as_mut() {
            path.clone_into(open);
        }
        false
    }
}

/// Appends one key to a dotted path: plain keys join with `.`, a key that a
/// dot, bracket, quote or space would misread is quoted as `["…"]`.
fn push_key(out: &mut String, key: &str) {
    let plain = !key.is_empty()
        && !key
            .chars()
            .any(|c| matches!(c, '.' | '[' | ']' | '"') || c.is_whitespace());
    if plain {
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(key);
    } else {
        out.push('[');
        out.push_str(&serde_json::to_string(key).unwrap_or_default());
        out.push(']');
    }
}

/// `matchKeys` on the wire: `{"<line>": "<key path>"}` in line order.
pub(crate) struct MatchKeys<'a>(pub(crate) &'a [(usize, String)]);

impl Serialize for MatchKeys<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (line, key) in self.0 {
            map.serialize_entry(&line.to_string(), key)?;
        }
        map.end()
    }
}

/// localSearch's `matchKeys` on the wire: `[{"line": <line>, "keyPath": "<key path>"}]` in line order.
pub(crate) struct MatchKeyRows<'a>(pub(crate) &'a [(usize, String)]);

impl Serialize for MatchKeyRows<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut rows = serializer.serialize_seq(Some(self.0.len()))?;
        for (line, key) in self.0 {
            rows.serialize_element(&MatchKeyRow {
                line: *line,
                key_path: key,
            })?;
        }
        rows.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchKeyRow<'a> {
    line: usize,
    key_path: &'a str,
}

/// `serialize_with` form of [`MatchKeyRows`].
pub fn serialize_rows<S: serde::Serializer>(
    keys: &[(usize, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    MatchKeyRows(keys).serialize(serializer)
}

#[cfg(test)]
mod tests;
