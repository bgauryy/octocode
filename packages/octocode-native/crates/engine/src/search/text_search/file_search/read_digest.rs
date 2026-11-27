//! The digest of the bytes one read of a file served its search, taken as
//! that read goes, so the digest a kept file carries is of the very bytes its
//! values were made from and no second read is made for it. Most files a walk
//! searches are dropped: a read holds its first bytes unhashed until its file
//! is known to be kept, and hashes only what passes beyond them.

use sha2::{Digest as _, Sha256};

/// Bytes a read holds before hashing them: a file no longer than this is
/// hashed only once it is kept; past it, the read's bytes are hashed as they
/// pass. One line buffer's unit ([`super::LINE_BUFFER_UNIT_BYTES`]).
const HELD_BYTES: usize = 1024 * 1024;

/// Bytes each read of a file's rest takes when a digest is finished.
const REST_CHUNK_BYTES: usize = 64 * 1024;

/// What one read of a file took from it, from its first byte: the bytes a
/// search of it read, as [`super::SourceTap`] passed them, or the bytes a
/// search held whole. A search that binds no digest to its files takes
/// nothing ([`Self::new`]).
#[derive(Default)]
pub(in super::super) struct ReadDigest {
    /// The search binds a digest to each kept file.
    wanted: bool,
    /// A read of the file's own bytes started (none: its search read only
    /// the text its judge served, or no digest is wanted).
    started: bool,
    /// The read's bytes not hashed yet: all of them while they fit
    /// [`HELD_BYTES`].
    held: Vec<u8>,
    /// The bytes before `held`, hashed once the read outgrew it.
    hasher: Option<Sha256>,
    lines: crate::text::LineCount,
    utf16: bool,
    /// Bytes read.
    len: u64,
    /// The read reached the end of the file.
    eof: bool,
}

/// The digest of a whole read of one file.
pub(super) struct ReadBytes {
    pub(super) size: u64,
    pub(super) digest: [u8; 32],
    /// Its lines; `None` for a UTF-16 file, whose lines are counted once
    /// decoded.
    pub(super) lines: Option<u32>,
}

impl ReadDigest {
    /// The reads of a search that binds a digest to each kept file when
    /// `wanted`; otherwise reads that take nothing.
    pub(in super::super) fn new(wanted: bool) -> Self {
        Self {
            wanted,
            ..Self::default()
        }
    }

    /// No read of the file's own bytes yet.
    pub(super) fn clear(&mut self) {
        self.restart();
        self.started = false;
    }

    /// A read of the file from its first byte starts: what an earlier read
    /// took is dropped.
    pub(super) fn restart(&mut self) {
        self.started = self.wanted;
        self.held.clear();
        self.held.shrink_to(HELD_BYTES);
        self.hasher = None;
        self.lines = crate::text::LineCount::default();
        self.utf16 = false;
        self.len = 0;
        self.eof = false;
    }

    /// The read's next `bytes`.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || !self.started {
            return;
        }
        if self.len == 0 {
            self.utf16 = crate::text::has_utf16_bom(bytes);
        }
        self.len += bytes.len() as u64;
        if self.hasher.is_none() && self.held.len() + bytes.len() <= HELD_BYTES {
            self.held.extend_from_slice(bytes);
            return;
        }
        let hasher = self.hasher.get_or_insert_with(Sha256::new);
        hasher.update(&self.held);
        self.lines.feed(&self.held);
        self.held.clear();
        hasher.update(bytes);
        self.lines.feed(bytes);
    }

    /// The read reached the end of the file.
    pub(super) fn ended(&mut self) {
        self.eof = true;
    }

    /// The digest of the read's bytes, and of the file's bytes after them
    /// when the read stopped before its end (read on from where it stopped,
    /// so the digest covers the whole file as this read took it). `None`
    /// when no read of the file's bytes started, its rest could not be read,
    /// or `cancelled` ended that read.
    pub(super) fn finish(
        &mut self,
        file: &std::fs::File,
        cancelled: Option<&(dyn Fn() -> bool + Sync)>,
    ) -> Option<ReadBytes> {
        use std::io::{Read, Seek, SeekFrom};
        if !std::mem::take(&mut self.started) {
            return None;
        }
        let mut hasher = self.hasher.take().unwrap_or_default();
        hasher.update(&self.held);
        self.lines.feed(&self.held);
        self.held.clear();
        if !self.eof {
            let mut handle = file;
            handle.seek(SeekFrom::Start(self.len)).ok()?;
            self.held.resize(REST_CHUNK_BYTES, 0);
            let rest = loop {
                if cancelled.is_some_and(|gone| gone()) {
                    break None;
                }
                match handle.read(&mut self.held) {
                    Ok(0) => break Some(()),
                    Ok(read) => {
                        hasher.update(&self.held[..read]);
                        self.lines.feed(&self.held[..read]);
                        self.len += read as u64;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break None,
                }
            };
            self.held.clear();
            rest?;
        }
        Some(ReadBytes {
            size: self.len,
            digest: hasher.finalize().into(),
            lines: (!self.utf16).then(|| crate::num::saturating_u32(self.lines.lines())),
        })
    }
}
