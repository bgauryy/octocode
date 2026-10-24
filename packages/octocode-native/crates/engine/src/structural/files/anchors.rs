//! The literal anchor prefilter.

use std::path::Path;

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
        let bytes = match crate::text::read_source(path, max_file_bytes) {
            Ok(source) => source.bytes,
            Err(crate::text::ReadError::TooLarge(len)) => {
                let hit = match &self.automaton {
                    None => true,
                    Some(automaton) => crate::text::open_regular(path)
                        .ok()
                        .and_then(|(mut file, _)| {
                            automaton.try_stream_find_iter(&mut file).ok()?.next()
                        })
                        .is_some_and(|found| found.is_ok()),
                };
                return if hit {
                    Anchored::Large(len)
                } else {
                    Anchored::Miss(None)
                };
            }
            Err(_) => return Anchored::Miss(None),
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
}
