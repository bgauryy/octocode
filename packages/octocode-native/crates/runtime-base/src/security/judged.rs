//! Leaves a tool judged against their whole source.
//!
//! The output gate judges each string leaf alone ([`super::ContentSecurity::sanitize_leaf`]).
//! A leaf cut from a source that the whole-source pass already judged
//! (`ContentScan::redact_source_lines`) is judged there with the syntax
//! around it; alone, a fragment that fails to parse reads as data. Such a
//! leaf is marked: a JSON pointer and the digest of its text, held beside the
//! result value and never inside it, so no input, key or serialized field
//! can set one. The gate honors a mark only where the leaf at that pointer
//! still has that digest; a leaf changed after it was judged (a page cut, a
//! hint, a transformed view) is judged by the full gate again. A marked leaf
//! still passes the patterns the whole pass did not apply
//! ([`octocode_engine::security::sanitizer::sanitize_judged_leaf`]).
//!
//! A tool that shows many fragments of the sources it judged
//! ([`JudgedText`]) marks them by text instead: the digests of the texts it
//! showed, held beside its row and scoped to that row's data
//! ([`JudgedLeaves::shown`]). A leaf there with one of those digests is that
//! judged text, wherever the row puts it.
use octocode_engine::digest::sha256_digest;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Text the whole-source pass returned for the file at `path`: the only
/// source of [`JudgedLeaf`]s.
pub struct JudgedSource<'a> {
    text: &'a str,
    path: &'a Path,
}

impl<'a> JudgedSource<'a> {
    /// `text` as `redact_source_lines` (or the large-source sealing built on
    /// it) returned it for the file at `path`, the path it scanned with, or
    /// that text with only the whitespace and comments between its tokens
    /// dropped and its lines numbered (a compact view): the tokens such a
    /// layout shows are the judged ones, in their order.
    pub fn new(text: &'a str, path: &'a Path) -> Self {
        Self { text, path }
    }

    /// `leaf` judged with this source: `None` unless it is a non-empty piece
    /// of the judged text, or such pieces with lines of the runtime's own
    /// between them, the ones `seam` names (the marker a read writes between
    /// two windows of a file). Every run of lines between two seams must be
    /// a piece of the judged text, so a transformed view never passes as
    /// judged, and a read of several ranges is judged as the whole read of
    /// the same lines is.
    pub fn leaf(&self, leaf: &str, seam: impl Fn(&str) -> bool) -> Option<JudgedLeaf> {
        let held = |piece: &str| {
            piece.is_empty()
                || memchr::memmem::find(self.text.as_bytes(), piece.as_bytes()).is_some()
        };
        // The run of lines since the last seam, and whether any line is no seam.
        let (mut run, mut any) = (0..0, false);
        for record in leaf.split_inclusive('\n') {
            let end = run.end + record.len();
            if seam(record) {
                if !held(leaf.get(run.clone())?) {
                    return None;
                }
                run = end..end;
            } else {
                (run.end, any) = (end, true);
            }
        }
        (any && held(leaf.get(run)?)).then(|| JudgedLeaf {
            digest: digest(leaf),
            source: self.path.to_string_lossy().into_owned(),
        })
    }
}

/// The digest of a judged leaf's text and the path its source was judged at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JudgedLeaf {
    digest: [u8; 32],
    source: String,
}

/// The marked leaves of one value: by JSON pointer, and by text under a
/// pointer (each set of shown texts with the pointer of the data it marks).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JudgedLeaves(Vec<(String, JudgedLeaf)>, Vec<(String, Shown)>);

/// Texts a row showed of its judged sources, by digest, with the path each
/// source was judged at.
type Shown = std::sync::Arc<HashMap<[u8; 32], String>>;

impl JudgedLeaves {
    /// Marks every string leaf of the value whose text is one `shown` holds
    /// ([`JudgedFiles::leaves`]).
    pub fn shown(shown: HashMap<[u8; 32], String>) -> Self {
        if shown.is_empty() {
            return Self::default();
        }
        Self(
            Vec::new(),
            vec![(String::new(), std::sync::Arc::new(shown))],
        )
    }

    /// Marks the string leaf at `pointer` (RFC 6901) as `leaf`.
    pub fn mark(&mut self, pointer: impl Into<String>, leaf: JudgedLeaf) {
        self.0.push((pointer.into(), leaf));
    }

    /// These marks for the value placed at `prefix` of an enclosing value.
    pub fn under(self, prefix: &str) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|(pointer, leaf)| (format!("{prefix}{pointer}"), leaf))
                .collect(),
            self.1
                .into_iter()
                .map(|(pointer, shown)| (format!("{prefix}{pointer}"), shown))
                .collect(),
        )
    }

    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
        self.1.extend(other.1);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty()
    }

    /// The marked string leaves of `value` whose text still has the marked
    /// digest, by the address of their text, with the path their source was
    /// judged at. An address names a live leaf only while `value` is not
    /// rewritten; the walker takes each one once, as it reaches the leaf.
    pub(super) fn resolve(&self, value: &Value) -> HashMap<usize, &str> {
        self.verified(value)
            .map(|(text, source)| (text.as_ptr() as usize, source))
            .collect()
    }

    /// Each marked leaf of `value` whose text has the marked digest.
    pub(super) fn verified<'s, 'v>(
        &'s self,
        value: &'v Value,
    ) -> impl Iterator<Item = (&'v str, &'s str)> + use<'s, 'v> {
        let pointed = self.0.iter().filter_map(|(pointer, leaf)| {
            let text = value.pointer(pointer)?.as_str()?;
            (!text.is_empty() && digest(text) == leaf.digest)
                .then_some((text, leaf.source.as_str()))
        });
        let shown = self.1.iter().flat_map(|(pointer, shown)| {
            let mut leaves = Vec::new();
            if let Some(data) = value.pointer(pointer) {
                string_leaves(data, &mut leaves);
            }
            leaves.into_iter().filter_map(|text| {
                let source = shown.get(&digest(text))?;
                Some((text, source.as_str()))
            })
        });
        pointed.chain(shown)
    }
}

/// Every non-empty string leaf of `value`.
fn string_leaves<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::String(text) if !text.is_empty() => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| string_leaves(item, out)),
        Value::Object(fields) => fields.values().for_each(|field| string_leaves(field, out)),
        _ => {}
    }
}

fn digest(text: &str) -> [u8; 32] {
    sha256_digest(text.as_bytes())
}

mod text;
pub use text::WholePass;
#[cfg(any(test, feature = "test-support"))]
pub use text::scanned_on_this_thread;
pub use text::{JudgedFiles, JudgedText, meets_written, searched_text};

#[cfg(test)]
mod tests;
