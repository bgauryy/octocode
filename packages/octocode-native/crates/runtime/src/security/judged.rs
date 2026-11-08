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
use octocode_engine::digest::sha256_digest;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Text the whole-source pass returned for the file at `path`: the only
/// source of [`JudgedLeaf`]s.
pub(crate) struct JudgedSource<'a> {
    text: &'a str,
    path: &'a Path,
}

impl<'a> JudgedSource<'a> {
    /// `text` as `redact_source_lines` (or the large-source sealing built on
    /// it) returned it for the file at `path`, the path it scanned with.
    pub(crate) fn new(text: &'a str, path: &'a Path) -> Self {
        Self { text, path }
    }

    /// `leaf` judged with this source: `None` unless it is a non-empty piece
    /// of the judged text, so a transformed view never passes as judged.
    pub(crate) fn leaf(&self, leaf: &str) -> Option<JudgedLeaf> {
        (!leaf.is_empty() && memchr::memmem::find(self.text.as_bytes(), leaf.as_bytes()).is_some())
            .then(|| JudgedLeaf {
                digest: digest(leaf),
                source: self.path.to_string_lossy().into_owned(),
            })
    }
}

/// The digest of a judged leaf's text and the path its source was judged at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JudgedLeaf {
    digest: [u8; 32],
    source: String,
}

/// The marked leaves of one value, by JSON pointer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JudgedLeaves(Vec<(String, JudgedLeaf)>);

impl JudgedLeaves {
    /// Marks the string leaf at `pointer` (RFC 6901) as `leaf`.
    pub(crate) fn mark(&mut self, pointer: impl Into<String>, leaf: JudgedLeaf) {
        self.0.push((pointer.into(), leaf));
    }

    /// These marks for the value placed at `prefix` of an enclosing value.
    pub(crate) fn under(self, prefix: &str) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|(pointer, leaf)| (format!("{prefix}{pointer}"), leaf))
                .collect(),
        )
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
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
        self.0.iter().filter_map(|(pointer, leaf)| {
            let text = value.pointer(pointer)?.as_str()?;
            (!text.is_empty() && digest(text) == leaf.digest)
                .then_some((text, leaf.source.as_str()))
        })
    }
}

fn digest(text: &str) -> [u8; 32] {
    sha256_digest(text.as_bytes())
}

#[cfg(test)]
mod tests;
