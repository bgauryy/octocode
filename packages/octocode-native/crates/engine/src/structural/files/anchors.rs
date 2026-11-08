//! The literal anchor prefilter.

use std::path::Path;

use super::file_scan::{SourceSkip, read_source};
use crate::structural::query::Prefilter;
use crate::text::{Decoding, admit, has_utf16_bom};

/// The literal prefilter: a file that holds none of the pattern's anchors
/// cannot match, so it is skipped unparsed.
pub(super) struct AnchorFilter {
    /// `None` when an anchor is empty: every readable file holds it.
    automaton: Option<aho_corasick::AhoCorasick>,
}

/// What the prefilter learned about one candidate.
pub(super) enum Anchored {
    /// No anchor, with a digest when the source was read whole.
    Miss(Option<String>),
    /// An anchor is present; the bytes read for the check, kept for the parse.
    Hit(Vec<u8>),
    /// An anchor is present in a file over the parse cap (its length).
    Large(u64),
    /// The file could not be read, so whether it holds an anchor is unknown.
    Unread(SourceSkip),
}

impl AnchorFilter {
    pub(super) fn new(prefilter: &Prefilter) -> Result<Option<Self>, String> {
        let anchors = match prefilter {
            Prefilter::None => return Ok(None),
            Prefilter::Single(anchor) => std::slice::from_ref(anchor),
            Prefilter::Union(anchors) => anchors.as_slice(),
        };
        // An empty anchor matches every file; short-circuit those instead of
        // feeding an empty pattern to Aho-Corasick. Otherwise build one
        // automaton for all anchors and scan each file in a single linear pass.
        let automaton = if anchors.iter().any(String::is_empty) {
            None
        } else {
            Some(aho_corasick::AhoCorasick::new(anchors).map_err(|error| error.to_string())?)
        };
        Ok(Some(Self { automaton }))
    }

    /// Read `path` once. A file within `max_file_bytes` is read whole and its
    /// bytes kept for the parse; a larger one is streamed only until the first
    /// anchor, never held in memory, since the parse skips it anyway.
    pub(super) fn check(&self, path: &Path, max_file_bytes: u64) -> Anchored {
        let bytes = match read_source(path, max_file_bytes) {
            Ok(bytes) => bytes,
            Err(SourceSkip::Large(len)) => return self.check_large(path, len),
            Err(skip) => return Anchored::Unread(skip),
        };
        // Anchors are text: BOM-marked UTF-16 bytes spell them differently
        // from the text the parse sees, so that text is searched (bytes the
        // admission refuses stay a hit, for the read to report).
        let absent = self.automaton.as_ref().is_some_and(|automaton| {
            if has_utf16_bom(&bytes) {
                admit(&bytes, Decoding::Unicode)
                    .is_ok_and(|text| !automaton.is_match(text.text.as_bytes()))
            } else {
                !automaton.is_match(&bytes)
            }
        });
        if absent {
            return Anchored::Miss(Some(crate::digest::sha256(&bytes)));
        }
        Anchored::Hit(bytes)
    }

    /// [`Self::check`] of a file over the parse cap, `len` bytes long: its
    /// stream is searched for the first anchor, and a stream that fails to
    /// open or read leaves it unread.
    fn check_large(&self, path: &Path, len: u64) -> Anchored {
        let Some(automaton) = &self.automaton else {
            return Anchored::Large(len);
        };
        let found = crate::text::open_regular(path).and_then(|(mut file, _)| {
            automaton
                .try_stream_find_iter(&mut file)
                .map_err(std::io::Error::other)?
                .next()
                .transpose()
        });
        match found {
            Ok(Some(_)) => Anchored::Large(len),
            Ok(None) => Anchored::Miss(None),
            Err(error) => Anchored::Unread(SourceSkip::Unreadable(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An over-cap file whose stream cannot be opened or read (gone, or not a
    /// regular file) is unread, never a miss: whether it holds an anchor is
    /// unknown.
    #[test]
    fn a_large_file_whose_stream_fails_is_unread() {
        let filter = AnchorFilter::new(&Prefilter::Single("needle".to_owned()))
            .ok()
            .flatten()
            .unwrap_or_else(|| unreachable!("a non-empty anchor builds a filter"));
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        for path in [dir.path().join("gone.rs"), dir.path().to_path_buf()] {
            assert!(
                matches!(
                    filter.check_large(&path, u64::MAX),
                    Anchored::Unread(SourceSkip::Unreadable(_))
                ),
                "{}",
                path.display()
            );
        }
    }
}
