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
pub(crate) fn is_keyed_path(path: &Path) -> bool {
    kind(path).is_some()
}

/// The key path of each of `lines` (1-based, ascending) in `text`, a
/// document at `path`; empty for any other file. Lines at the document root
/// are left out.
pub(crate) fn line_key_paths(path: &Path, text: &str, lines: &[usize]) -> Vec<(usize, String)> {
    match kind(path) {
        Some(Kind::Json) => json::line_key_paths(text, lines),
        Some(Kind::Toml) => toml::line_key_paths(text, lines),
        None => Vec::new(),
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

/// `serialize_with` form of [`MatchKeys`].
pub(crate) fn serialize<S: serde::Serializer>(
    keys: &[(usize, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    MatchKeys(keys).serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_json_and_toml_extensions_count() {
        for (path, keyed) in [
            ("package.json", true),
            ("tsconfig.JSONC", true),
            ("a.json5", true),
            ("Cargo.toml", true),
            ("pyproject.TOML", true),
            ("a.jsonl", false),
            ("a.js", false),
            ("Cargo.lock", false),
        ] {
            assert_eq!(is_keyed_path(Path::new(path)), keyed, "{path}");
        }
    }

    #[test]
    fn other_files_have_no_key_paths() {
        assert!(line_key_paths(Path::new("a.rs"), "a = 1\n", &[1]).is_empty());
        assert_eq!(
            line_key_paths(Path::new("a.toml"), "[t]\na = 1\n", &[2]),
            vec![(2, "t.a".to_owned())]
        );
    }
}
