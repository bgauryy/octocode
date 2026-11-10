//! The packed encoding's reader: the inverse of [`super::write::Writer`];
//! every method yields `None` on a malformed buffer.

use super::super::page_view::EntryFacts;
use super::super::rows::HitWindow;
use octocode_engine::types::{SearchedSource, TextSearchFile, TextSearchMatch};
use std::time::{Duration, SystemTime};

#[cfg(test)]
thread_local! {
    /// Hits this thread decoded into rows, for tests that pin what a reader
    /// materializes.
    static DECODED_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Hits decoded into rows on this thread so far.
#[cfg(test)]
pub(super) fn decoded_hits_on_this_thread() -> usize {
    DECODED_HITS.with(std::cell::Cell::get)
}

pub(super) struct Reader<'a> {
    raw: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(raw: &'a [u8]) -> Self {
        Self { raw, at: 0 }
    }

    /// Whether every byte has been read.
    pub(super) fn finished(&self) -> bool {
        self.at == self.raw.len()
    }

    fn bytes(&mut self, len: usize) -> Option<&[u8]> {
        let end = self.at.checked_add(len)?;
        let bytes = self.raw.get(self.at..end)?;
        self.at = end;
        Some(bytes)
    }

    fn number(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.raw.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn small(&mut self) -> Option<u32> {
        u32::try_from(self.number()?).ok()
    }

    fn text(&mut self) -> Option<String> {
        let len = usize::try_from(self.number()?).ok()?;
        String::from_utf8(self.bytes(len)?.to_vec()).ok()
    }

    #[expect(
        clippy::option_option,
        reason = "decoder convention: the outer `None` is a malformed buffer, as for every `Reader` method; the inner one is the encoded absent value"
    )]
    fn optional(&mut self) -> Option<Option<u32>> {
        match self.number()? {
            0 => Some(None),
            n => u32::try_from(n - 1).ok().map(Some),
        }
    }

    /// The next file with every hit.
    pub(super) fn file(&mut self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchFile> {
        self.file_with(cancelled, |_| true)
    }

    /// The next file as a store needs it: its path and source. A file whose
    /// source the search hashed comes without hits (they are read past, none
    /// materialized), so a scan of any size is walked with one file's row
    /// in hand; only a file with no digest keeps its hits, which its
    /// bytes are checked against.
    pub(super) fn sourced_file(&mut self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchFile> {
        self.file_with(cancelled, |hashed| !hashed)
    }

    /// The next page-entry frame (see the frame layout in [`super`]).
    pub(super) fn entry(&mut self, cancelled: &dyn Fn() -> bool) -> Option<super::Entry> {
        let file = self.file(cancelled)?;
        let facts = self.facts()?;
        let redacted = (0..self.number()?)
            .map(|_| self.small())
            .collect::<Option<Vec<_>>>()?;
        let key_scan = match self.number()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        Some(super::Entry {
            file,
            facts,
            redacted,
            key_scan,
        })
    }

    /// Past what follows a frame's file: its facts and flags.
    pub(super) fn entry_tail(&mut self) -> Option<()> {
        self.facts()?;
        for _ in 0..self.number()? {
            self.small()?;
        }
        self.number().filter(|flag| *flag <= 1).map(drop)
    }

    fn facts(&mut self) -> Option<EntryFacts> {
        let total = self.small()?;
        let start = self.small()?;
        let end = self.small()?;
        let neighbours = (0..self.number()?)
            .map(|_| {
                Some(HitWindow {
                    line: self.small()?,
                    matched_last: self.small()?,
                    first: self.small()?,
                    last: self.small()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        (start <= end && end <= total).then_some(EntryFacts {
            total,
            start,
            end,
            neighbours,
        })
    }

    /// The next file; its hits are kept when `keep` holds of whether the
    /// search hashed its source, else read past.
    fn file_with(
        &mut self,
        cancelled: &dyn Fn() -> bool,
        keep: impl Fn(bool) -> bool,
    ) -> Option<TextSearchFile> {
        let path = self.text()?;
        let match_count = self.small()?;
        let terms = usize::try_from(self.number()?).ok()?;
        let term_counts = (0..terms)
            .map(|_| self.small())
            .collect::<Option<Vec<_>>>()?;
        let source = match self.number()? {
            0 => None,
            1 => Some(SearchedSource {
                size: self.number()?,
                modified: self.time()?,
                digest: self.bytes(32)?.try_into().ok()?,
            }),
            _ => return None,
        };
        let hits = usize::try_from(self.number()?).ok()?;
        let keep = keep(source.is_some());
        let mut matches = Vec::with_capacity(if keep { hits.min(self.raw.len()) } else { 0 });
        for _ in 0..hits {
            if cancelled() {
                return None;
            }
            let hit = self.hit(keep)?;
            if keep {
                matches.push(hit);
            }
        }
        Some(TextSearchFile {
            term_counts,
            path,
            match_count,
            matches,
            source,
        })
    }

    /// The next hit; without `keep` its value is read past, not copied.
    fn hit(&mut self, keep: bool) -> Option<TextSearchMatch> {
        let line = self.small()?;
        let column = self.small()?;
        let byte_offset = self.small()?;
        let deferred = self.optional()?;
        let value = if keep {
            self.text()?
        } else {
            let len = usize::try_from(self.number()?).ok()?;
            self.bytes(len)?;
            String::new()
        };
        let value_digest = if deferred.is_some() {
            let mut digest = [0; 32];
            digest[..8].copy_from_slice(&self.number()?.to_le_bytes());
            digest
        } else {
            self.bytes(32)?.try_into().ok()?
        };
        let last_line = self.optional()?;
        let count = self.optional()?;
        let rank = self.optional()?;
        let before = u32::try_from(self.number()?).ok()?;
        #[cfg(test)]
        if keep {
            DECODED_HITS.with(|decoded| decoded.set(decoded.get() + 1));
        }
        Some(TextSearchMatch {
            line,
            first_line: line.checked_sub(before)?,
            last_line,
            column,
            byte_offset,
            deferred,
            value,
            value_digest,
            count,
            rank,
        })
    }

    #[expect(
        clippy::option_option,
        reason = "decoder convention: the outer `None` is a malformed buffer, as for every `Reader` method; the inner one is the encoded absent value"
    )]
    fn time(&mut self) -> Option<Option<SystemTime>> {
        let tag = self.number()?;
        if tag == 0 {
            return Some(None);
        }
        let distance = Duration::from_secs(self.number()?)
            .checked_add(Duration::from_nanos(self.number()?))?;
        match tag {
            1 => SystemTime::UNIX_EPOCH.checked_add(distance).map(Some),
            2 => SystemTime::UNIX_EPOCH.checked_sub(distance).map(Some),
            _ => None,
        }
    }
}
