//! Bounded in-memory continuation scans. Ordinary rows carry source anchors,
//! exact prices and duplicate equivalence classes, without snippet text. Class
//! IDs preserve equality and ranking; they are not source identities. The
//! initial snapshot hashes full value digests and source digests separately.
//! Other views retain their values. No scan is written to disk.

use octocode_engine::types::{
    SearchedSource, TextSearchFile, TextSearchMatch, TextSearchResult, TextSearchStats,
};
use std::io::{BufWriter, Read, Write};
use std::time::{Duration, SystemTime};

/// A scan packed by [`Packed::pack`].
pub struct Packed {
    /// The deflated files.
    bytes: Vec<u8>,
    /// Bytes the files take unpacked: the buffer [`Packed::unpack`] inflates.
    raw_len: usize,
    files: usize,
    stats: TextSearchStats,
}

impl Packed {
    /// `scan` packed, or `None` when its files take more than `max_raw`
    /// bytes unpacked (a page would inflate that much to read it) or more
    /// than `max_packed` packed. Packing stops at the file that passes
    /// either bound, so a scan too large to keep costs no more than the
    /// bound to refuse.
    #[cfg(test)]
    pub fn pack(scan: &TextSearchResult, max_raw: usize, max_packed: usize) -> Option<Self> {
        Self::pack_checked(scan, max_raw, max_packed, &|| false)
    }

    pub fn pack_checked(
        scan: &TextSearchResult,
        max_raw: usize,
        max_packed: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut out = Writer {
            sink: BufWriter::with_capacity(
                64 * 1024,
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast()),
            ),
            written: 0,
            max: max_raw,
        };
        for file in &scan.files {
            out.file(file, true, cancelled)?;
            // The deflated bytes so far only grow.
            (out.sink.get_ref().get_ref().len() <= max_packed).then_some(())?;
        }
        let raw_len = out.written;
        let bytes = out.sink.into_inner().ok()?.finish().ok()?;
        (bytes.len() <= max_packed).then_some(())?;
        Some(Self {
            bytes,
            raw_len,
            files: scan.files.len(),
            stats: scan.stats.clone(),
        })
    }

    /// The scan this was packed from.
    #[cfg(test)]
    pub fn unpack(&self) -> Option<TextSearchResult> {
        self.unpack_checked(&|| false)
    }

    pub fn unpack_checked(&self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchResult> {
        let mut raw = Vec::with_capacity(self.raw_len);
        let mut input =
            flate2::read::DeflateDecoder::new(self.bytes.as_slice()).take(self.raw_len as u64);
        let mut block = vec![0; 65536];
        loop {
            if cancelled() {
                return None;
            }
            let n = input.read(&mut block).ok()?;
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&block[..n]);
        }
        let mut reader = Reader { raw: &raw, at: 0 };
        let files = (0..self.files)
            .map(|_| reader.file(cancelled))
            .collect::<Option<Vec<_>>>()?;
        (reader.at == raw.len()).then(|| TextSearchResult {
            files,
            stats: self.stats.clone(),
        })
    }

    /// Bytes this keeps in memory: the deflated files.
    pub fn weight(&self) -> usize {
        self.bytes.len()
    }
}

struct Writer<W: Write> {
    sink: W,
    written: usize,
    max: usize,
}

impl<W: Write> Writer<W> {
    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        self.written = self.written.checked_add(bytes.len())?;
        (self.written <= self.max).then_some(())?;
        self.sink.write_all(bytes).ok()
    }

    /// LEB128: seven bits a byte, low first, high bit set on all but the last.
    fn number(&mut self, mut value: u64) -> Option<()> {
        let mut buffer = [0u8; 10];
        let mut len = 0;
        loop {
            let low = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                buffer[len] = low;
                len += 1;
                break;
            }
            buffer[len] = low | 0x80;
            len += 1;
        }
        self.bytes(&buffer[..len])
    }

    fn text(&mut self, text: &str) -> Option<()> {
        self.number(text.len() as u64)?;
        self.bytes(text.as_bytes())
    }

    /// `None` as 0, `Some(n)` as n + 1.
    fn optional(&mut self, value: Option<u32>) -> Option<()> {
        self.number(value.map_or(0, |n| u64::from(n) + 1))
    }

    /// Every field of `file`; its source's size and time only when
    /// `stamps` (an identity binds the source by its digest alone).
    fn file(
        &mut self,
        file: &TextSearchFile,
        stamps: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<()> {
        self.text(&file.path)?;
        self.number(u64::from(file.match_count))?;
        match &file.source {
            None => self.number(0)?,
            Some(source) => {
                self.number(1)?;
                if stamps {
                    self.number(source.size)?;
                    self.time(source.modified)?;
                }
                self.bytes(&source.digest)?;
            }
        }
        self.number(file.matches.len() as u64)?;
        let mut classes = std::collections::HashMap::new();
        for hit in &file.matches {
            if cancelled() {
                return None;
            }
            self.number(u64::from(hit.line))?;
            self.number(u64::from(hit.column))?;
            self.number(u64::from(hit.byte_offset))?;
            self.optional(hit.deferred)?;
            self.text(&hit.value)?;
            if stamps && hit.deferred.is_some() {
                let next = classes.len() as u64;
                let class = *classes.entry(hit.value_digest).or_insert(next);
                self.number(class)?;
            } else {
                self.bytes(&hit.value_digest)?;
            }
            for value in [
                hit.last_line,
                hit.count,
                hit.rank,
                hit.original_chars,
                hit.original_bytes,
            ] {
                self.optional(value)?;
            }
        }
        Some(())
    }

    /// No time as 0; a time at or after the epoch as 1, before it as 2,
    /// then the distance in seconds and nanoseconds.
    fn time(&mut self, time: Option<SystemTime>) -> Option<()> {
        let Some(time) = time else {
            return self.number(0);
        };
        let (tag, distance) = match time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(after) => (1, after),
            Err(before) => (2, before.duration()),
        };
        self.number(tag)?;
        self.number(distance.as_secs())?;
        self.number(u64::from(distance.subsec_nanos()))
    }
}

/// SHA-256 over a scan written in the packed encoding (every value is
/// length-prefixed, so distinct inputs write distinct bytes), streamed into
/// the hasher: hashing a scan of any size keeps no copy of it.
pub struct Identity(Writer<Hashing>);

struct Hashing(sha2::Sha256);

impl Write for Hashing {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        sha2::Digest::update(&mut self.0, bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Default for Identity {
    fn default() -> Self {
        Self(Writer {
            sink: Hashing(<sha2::Sha256 as sha2::Digest>::new()),
            written: 0,
            max: usize::MAX,
        })
    }
}

impl Identity {
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.0.text(text);
        self
    }

    pub fn number(&mut self, value: u64) -> &mut Self {
        self.0.number(value);
        self
    }

    /// `None` as 0, `Some(n)` as n + 1.
    pub fn optional(&mut self, value: Option<u32>) -> &mut Self {
        self.0.optional(value);
        self
    }

    /// `None` as 0, `Some(text)` as 1 then the text.
    pub fn optional_text(&mut self, text: Option<&str>) -> &mut Self {
        match text {
            None => self.number(0),
            Some(text) => self.number(1).text(text),
        }
    }

    /// Every field of `file` except its source's size and time: the source
    /// is bound by the digest of its bytes.
    pub fn file(&mut self, file: &TextSearchFile) -> &mut Self {
        self.0.file(file, false, &|| false);
        self
    }

    /// Lowercase-hex SHA-256 of everything written.
    pub fn finish(self) -> String {
        hex::encode(sha2::Digest::finalize(self.0.sink.0))
    }
}

struct Reader<'a> {
    raw: &'a [u8],
    at: usize,
}

impl Reader<'_> {
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

    fn file(&mut self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchFile> {
        let path = self.text()?;
        let match_count = self.small()?;
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
                original_chars: self.optional()?,
                original_bytes: self.optional()?,
            });
        }
        Some(TextSearchFile {
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

#[cfg(test)]
mod tests;
