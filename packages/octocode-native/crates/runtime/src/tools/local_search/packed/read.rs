//! The packed encoding's reader: the inverse of [`super::write::Writer`];
//! every method yields `None` on a malformed buffer.

use octocode_engine::types::{SearchedSource, TextSearchFile, TextSearchMatch};
use std::time::{Duration, SystemTime};

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

    pub(super) fn file(&mut self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchFile> {
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
        let mut matches = Vec::with_capacity(hits.min(self.raw.len()));
        for _ in 0..hits {
            if cancelled() {
                return None;
            }
            let line = self.small()?;
            let column = self.small()?;
            let byte_offset = self.small()?;
            let deferred = self.optional()?;
            let value = self.text()?;
            let value_digest = if deferred.is_some() {
                let mut digest = [0; 32];
                digest[..8].copy_from_slice(&self.number()?.to_le_bytes());
                digest
            } else {
                self.bytes(32)?.try_into().ok()?
            };
            matches.push(TextSearchMatch {
                line,
                last_line: self.optional()?,
                column,
                byte_offset,
                deferred,
                value,
                value_digest,
                count: self.optional()?,
                rank: self.optional()?,
            });
        }
        Some(TextSearchFile {
            term_counts,
            path,
            match_count,
            matches,
            source,
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
