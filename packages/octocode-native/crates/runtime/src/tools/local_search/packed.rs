//! Bounded in-memory continuation scans. Ordinary rows carry source anchors,
//! exact prices and duplicate equivalence classes, without snippet text. Class
//! IDs preserve equality and ranking; they are not source identities. The
//! initial snapshot hashes full value digests and source digests separately.
//! Other views retain their values. No scan is written to disk.
//!
//! A fresh page encodes its scan ([`Encoded`]) before shaping reorders it,
//! and deflates it ([`Encoded::deflate_checked`]) only when its cursor keeps
//! the scan for later pages: a one-page answer compresses nothing.
//!
//! The encoding itself is written by [`write`] and read back by [`read`].

mod read;
mod write;

use octocode_engine::types::{TextSearchResult, TextSearchStats};
use read::Reader;
use std::io::{Read, Write};
pub use write::Identity;
use write::Writer;

/// A scan in the packed encoding, not yet deflated: what a fresh page holds
/// until its cursor decides whether the scan is kept.
pub struct Encoded {
    raw: Vec<u8>,
    files: usize,
    stats: TextSearchStats,
}

impl Encoded {
    /// `scan` encoded, or `None` when its files take more than `max_raw`
    /// bytes (a page would inflate that much to read it back). Encoding
    /// stops at the file that passes the bound, so a scan too large to keep
    /// costs no more than the bound to refuse.
    pub fn encode_checked(
        scan: &TextSearchResult,
        max_raw: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut out = Writer::new(Vec::new(), max_raw);
        for file in &scan.files {
            out.file(file, true, cancelled)?;
        }
        Some(Self {
            raw: out.into_sink(),
            files: scan.files.len(),
            stats: scan.stats.clone(),
        })
    }

    /// The scan this was encoded from.
    pub fn decode_checked(&self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchResult> {
        decode(&self.raw, self.files, &self.stats, cancelled)
    }

    /// Deflated for storage, or `None` when the deflated bytes pass
    /// `max_packed` (checked as they grow, so an oversized scan stops early).
    pub fn deflate_checked(
        self,
        max_packed: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Packed> {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        for block in self.raw.chunks(64 * 1024) {
            if cancelled() {
                return None;
            }
            encoder.write_all(block).ok()?;
            // The deflated bytes so far only grow.
            (encoder.get_ref().len() <= max_packed).then_some(())?;
        }
        #[cfg(test)]
        DEFLATED.with(|deflated| deflated.set(deflated.get() + self.raw.len()));
        let bytes = encoder.finish().ok()?;
        (bytes.len() <= max_packed).then_some(())?;
        Some(Packed {
            bytes,
            raw_len: self.raw.len(),
            files: self.files,
            stats: self.stats,
        })
    }
}

/// A scan deflated by [`Encoded::deflate_checked`].
pub struct Packed {
    /// The deflated files.
    bytes: Vec<u8>,
    /// Bytes the files take unpacked: the buffer [`Packed::unpack`] inflates.
    raw_len: usize,
    files: usize,
    stats: TextSearchStats,
}

impl Packed {
    /// `scan` encoded within `max_raw` bytes and deflated within
    /// `max_packed` (see [`Encoded`]).
    #[cfg(test)]
    pub fn pack(scan: &TextSearchResult, max_raw: usize, max_packed: usize) -> Option<Self> {
        Encoded::encode_checked(scan, max_raw, &|| false)?.deflate_checked(max_packed, &|| false)
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
        decode(&raw, self.files, &self.stats, cancelled)
    }

    /// Bytes this keeps in memory: the deflated files.
    pub fn weight(&self) -> usize {
        self.bytes.len()
    }
}

/// The `files` files encoded in `raw`, with `stats`; `None` when `raw` is
/// not exactly that encoding.
fn decode(
    raw: &[u8],
    files: usize,
    stats: &TextSearchStats,
    cancelled: &dyn Fn() -> bool,
) -> Option<TextSearchResult> {
    let mut reader = Reader::new(raw);
    let files = (0..files)
        .map(|_| reader.file(cancelled))
        .collect::<Option<Vec<_>>>()?;
    reader.finished().then(|| TextSearchResult {
        files,
        stats: stats.clone(),
    })
}

#[cfg(test)]
thread_local! {
    /// Raw bytes this thread deflated, for tests that pin when a scan is packed.
    static DEFLATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Raw bytes deflated on this thread so far.
#[cfg(test)]
pub fn deflated_on_this_thread() -> usize {
    DEFLATED.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests;
