//! The literal anchor prefilter.

use std::path::Path;

use super::file_scan::{SourceSkip, read_source};
use crate::structural::query::Prefilter;
use crate::text::{Decoding, admit, has_utf16_bom};

/// The literal prefilter: a file that lacks a literal of any of the query's
/// groups (an AND of ORs, [`Prefilter`]) cannot match, so it is skipped
/// unparsed. One automaton holds every literal, and one pass over the file
/// marks the groups it hits.
pub(in crate::structural) struct AnchorFilter {
    /// `None` when every group holds an empty anchor: every readable file
    /// holds those.
    automaton: Option<aho_corasick::AhoCorasick>,
    /// The groups each automaton pattern belongs to, by pattern index.
    groups_of: Vec<Vec<usize>>,
    /// How many groups a file must hit.
    groups: usize,
    /// The longest anchor's byte length: what a streamed chunk keeps of the
    /// previous one, so an anchor across the boundary is found.
    longest: usize,
}

/// What the prefilter learned about one candidate.
pub(super) enum Anchored {
    /// An anchor group without a hit, with a digest when the source was read
    /// whole.
    Miss(Option<String>),
    /// Every group is hit; the bytes read for the check, kept for the parse.
    Hit(Vec<u8>),
    /// Every group is hit in a file over the parse cap (its length).
    Large(u64),
    /// The file could not be read, so whether it holds the anchors is unknown.
    Unread(SourceSkip),
}

/// Which groups a pass has hit so far.
struct Hits {
    hit: Vec<bool>,
    missing: usize,
}

impl Hits {
    /// Marks the groups of every anchor in `haystack`; whether every group
    /// is hit now.
    fn scan(
        &mut self,
        automaton: &aho_corasick::AhoCorasick,
        groups_of: &[Vec<usize>],
        haystack: &[u8],
    ) -> bool {
        for found in automaton.find_overlapping_iter(haystack) {
            for &group in &groups_of[found.pattern().as_usize()] {
                if !self.hit[group] {
                    self.hit[group] = true;
                    self.missing -= 1;
                }
            }
            if self.missing == 0 {
                return true;
            }
        }
        false
    }
}

impl AnchorFilter {
    pub(in crate::structural) fn new(prefilter: &Prefilter) -> Result<Option<Self>, String> {
        let groups: Vec<&[String]> = match prefilter {
            Prefilter::None => return Ok(None),
            Prefilter::Single(anchor) => vec![std::slice::from_ref(anchor)],
            Prefilter::Union(anchors) => vec![anchors.as_slice()],
            Prefilter::All(groups) => groups.iter().map(Vec::as_slice).collect(),
        };
        // An empty anchor matches every file, so its group is always hit;
        // the other groups go to one automaton, with each distinct anchor
        // once.
        let groups: Vec<&[String]> = groups
            .into_iter()
            .filter(|group| !group.iter().any(String::is_empty))
            .collect();
        let mut anchors: Vec<&str> = Vec::new();
        let mut groups_of: Vec<Vec<usize>> = Vec::new();
        for (group, members) in groups.iter().enumerate() {
            for anchor in *members {
                let index = anchors
                    .iter()
                    .position(|known| *known == anchor.as_str())
                    .unwrap_or_else(|| {
                        anchors.push(anchor);
                        groups_of.push(Vec::new());
                        anchors.len() - 1
                    });
                if !groups_of[index].contains(&group) {
                    groups_of[index].push(group);
                }
            }
        }
        let automaton = if groups.is_empty() {
            None
        } else {
            Some(aho_corasick::AhoCorasick::new(&anchors).map_err(|error| error.to_string())?)
        };
        Ok(Some(Self {
            automaton,
            groups_of,
            groups: groups.len(),
            longest: anchors.iter().map(|anchor| anchor.len()).max().unwrap_or(0),
        }))
    }

    /// Whether `haystack` holds a literal of every group.
    pub(in crate::structural) fn admits(&self, haystack: &[u8]) -> bool {
        let Some(automaton) = &self.automaton else {
            return true;
        };
        if self.groups == 1 {
            return automaton.is_match(haystack);
        }
        self.hits().scan(automaton, &self.groups_of, haystack)
    }

    fn hits(&self) -> Hits {
        Hits {
            hit: vec![false; self.groups],
            missing: self.groups,
        }
    }

    /// Read `path` once. A file within `max_file_bytes` is read whole and its
    /// bytes kept for the parse; a larger one is streamed only until every
    /// group is hit, never held in memory, since the parse skips it anyway.
    pub(super) fn check(&self, path: &Path, max_file_bytes: u64) -> Anchored {
        let bytes = match read_source(path, max_file_bytes) {
            Ok(bytes) => bytes,
            Err(SourceSkip::Large(len)) => return self.check_large(path, len),
            Err(skip) => return Anchored::Unread(skip),
        };
        let absent = self.automaton.is_some() && self.absent(&bytes);
        if absent {
            return Anchored::Miss(Some(crate::digest::sha256(&bytes)));
        }
        Anchored::Hit(bytes)
    }

    /// Whether no anchor group is in `bytes`. Anchors are text: BOM-marked
    /// UTF-16 and legacy 8-bit bytes spell them differently from the text the
    /// parse sees, so that text is searched when the bytes do not hold them
    /// (bytes the admission refuses stay a hit, for the read to report).
    fn absent(&self, bytes: &[u8]) -> bool {
        if !has_utf16_bom(bytes) {
            if self.admits(bytes) {
                return false;
            }
            if std::str::from_utf8(bytes).is_ok() {
                return true;
            }
        }
        admit(bytes, Decoding::Lenient).is_ok_and(|text| !self.admits(text.text.as_bytes()))
    }

    /// [`Self::check`] of a file over the parse cap, `len` bytes long: its
    /// stream is searched until every group is hit or it ends, and a stream
    /// that fails to open or read leaves it unread. A BOM-marked UTF-16
    /// stream stays large: its bytes do not spell the text anchors.
    fn check_large(&self, path: &Path, len: u64) -> Anchored {
        let Some(automaton) = &self.automaton else {
            return Anchored::Large(len);
        };
        let found = crate::text::open_regular(path).and_then(|(mut file, _)| {
            // BOM-marked UTF-16 spells the anchors in other bytes than the
            // text the parse would see: the stream cannot prove them absent,
            // so the file stays large (unread), never anchor-free.
            let mut head = [0; 2];
            let mut filled = 0;
            while filled < head.len() {
                match std::io::Read::read(&mut file, &mut head[filled..]) {
                    Ok(0) => break,
                    Ok(read) => filled += read,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error),
                }
            }
            if has_utf16_bom(&head[..filled]) {
                return Ok(true);
            }
            self.stream_admits(automaton, &mut std::io::Read::chain(&head[..filled], file))
        });
        match found {
            Ok(true) => Anchored::Large(len),
            Ok(false) => Anchored::Miss(None),
            Err(error) => Anchored::Unread(SourceSkip::Unreadable(error)),
        }
    }

    /// Whether the stream holds a literal of every group. Each chunk is
    /// searched after the last `longest - 1` bytes of the one before, so an
    /// anchor that spans two reads is found.
    fn stream_admits(
        &self,
        automaton: &aho_corasick::AhoCorasick,
        reader: &mut impl std::io::Read,
    ) -> std::io::Result<bool> {
        const CHUNK: usize = 64 * 1024;
        let mut hits = self.hits();
        let mut window: Vec<u8> = Vec::with_capacity(CHUNK + self.longest);
        let mut chunk = vec![0; CHUNK];
        loop {
            let read = match reader.read(&mut chunk) {
                Ok(0) => return Ok(false),
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            window.extend_from_slice(&chunk[..read]);
            if hits.scan(automaton, &self.groups_of, &window) {
                return Ok(true);
            }
            let keep = self.longest.saturating_sub(1).min(window.len());
            window.drain(..window.len() - keep);
        }
    }
}

#[cfg(test)]
mod tests;
