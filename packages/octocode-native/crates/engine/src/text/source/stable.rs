//! The one stable-read rule. Bytes read from a file are one version of it
//! only when the opened file's stamp ([`SourceStamp`]: size, modification
//! and change times, inode) is the same before and after the read. A write
//! that lands while a read is under way (a save in place truncates the file,
//! then writes it) can join the old version's first bytes to the new
//! version's rest, and that join can hold a piece of a secret too short for
//! any pattern to name: no text of it may be served.
//!
//! A read the stamp moved under is made again, from a fresh open, while the
//! caller's check passes (its request's deadline and cancel) and the file
//! has not kept changing for longer than its settle window
//! ([`SourceStamp::settle_for`]) since the first read a write moved: a file
//! written all through that window is written without a pause any read of
//! it could fit in. Then the read fails as [`changed_while_read`], and what
//! it read is left out ([`Settling`]).
//!
//! A file that is only appended to (a live log) moves the stamp of every
//! read longer than the pause between two writes, so no read of it would
//! settle. A read that says what it took ([`Taken`],
//! [`Settling::settled_read`]) is proved by its prefix instead: when the
//! opened file only grew (the same inode, not shorter than the read), its
//! first bytes, as many as the read took, are read again through the same
//! handle with no write moving the stamp, and they are the bytes the read
//! took. They are then the start of one version of the file; when that
//! version holds more, they must end at a line break (so no line is cut),
//! and the read may be served as of its length ([`Proved::Prefix`]). A save
//! in place that tore the read leaves other bytes there, and the read is
//! made again as above. A prefix read again while a write moves it proves
//! nothing, and is read again within the same window.

use crate::stamp::SourceStamp;
use std::io::{Read, Seek, SeekFrom};

/// Whether `before` and `after`, metadata of one opened file, carry the
/// same stamp: no write reached the file between them.
fn same_stamp(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    before.len() == after.len() && SourceStamp::of(before) == SourceStamp::of(after)
}

/// Whether `file`, whose metadata was `opened` before a read of it, still
/// has that stamp now: the read between them is of one version.
pub fn unchanged_since(file: &std::fs::File, opened: &std::fs::Metadata) -> bool {
    file.metadata()
        .is_ok_and(|after| same_stamp(opened, &after))
}

/// What a read of a file proved, by [`Settling::settled`].
#[derive(Debug)]
pub enum Settled {
    /// No write moved the stamp: the read is of one version, which this
    /// metadata, taken after it, describes.
    Stable(std::fs::Metadata),
    /// A write moved it: read the file again.
    Again,
    /// The file kept changing past its settle window: no read of it is
    /// proved, and none is made again.
    Unsettled,
}

/// What a read took of a file, from its first byte to where it ended, for
/// [`Settling::settled_read`] to prove it the start of one version.
#[derive(Clone, Copy, Debug)]
pub enum Taken<'a> {
    /// The bytes it took, held by the caller.
    Bytes(&'a [u8]),
    /// How many bytes it took and their SHA-256: a read that streamed them.
    /// With `keep`, a proof returns those bytes as read again
    /// ([`Prefix::bytes`]), for a caller that works on them next.
    Hashed {
        len: u64,
        digest: [u8; 32],
        keep: bool,
    },
}

impl Taken<'_> {
    fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            Self::Hashed { len, .. } => *len,
        }
    }
}

/// A read proved to be the start of one version of its file, a version
/// that holds more after it ([`Proved::Prefix`]).
#[derive(Debug)]
pub struct Prefix {
    /// The bytes proved, from the file's start: all the read took.
    pub len: u64,
    /// Those bytes as read again, for a read that took them hashed with
    /// `keep`; empty otherwise.
    pub bytes: Vec<u8>,
}

/// What a read that says what it took proved, by
/// [`Settling::settled_read`]: [`Settled`]'s outcomes, or that it is the
/// start of a version the file grew to.
#[derive(Debug)]
pub enum Proved {
    /// [`Settled::Stable`]: the read is the whole of one version.
    Whole(std::fs::Metadata),
    /// The file only grew, and the read is the start of one version of it:
    /// serve it as of its length, saying the file grew since.
    Prefix(Prefix),
    /// [`Settled::Again`].
    Again,
    /// [`Settled::Unsettled`].
    Unsettled,
}

/// One read again of a file's first bytes ([`Settling::settled_read`]).
enum Reread {
    /// They are the file whole, as this metadata, taken after them, says.
    Whole(std::fs::Metadata),
    /// They are the start of a longer version, ending at a line break.
    Proved(Prefix),
    /// A write moved the stamp while they were read: read them again.
    Moved,
    /// They are not the bytes the read took (a save in place tore it, or
    /// the file is shorter now), or the read took no whole last line.
    Other,
}

/// Tests: a write registered for the inode of a file, made after a read of
/// it ends and before the read is proved ([`Settling::settled`],
/// [`Settling::settled_read`]), as a writer appending to a live file makes
/// it; it returns whether it stays registered for the next proof.
#[cfg(any(test, feature = "test-hooks"))]
pub type ProofRace = (u64, Box<dyn FnMut() -> bool + Send>);

/// Tests: the writes [`before_proof`] makes.
#[cfg(any(test, feature = "test-hooks"))]
pub static BEFORE_PROOF: std::sync::Mutex<Vec<ProofRace>> = std::sync::Mutex::new(Vec::new());

/// Tests: makes the write registered for the inode of `opened`.
#[cfg(any(test, feature = "test-hooks"))]
fn before_proof(opened: &std::fs::Metadata) {
    let mut races = BEFORE_PROOF
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if races.is_empty() {
        return;
    }
    let Some(inode) = SourceStamp::of(opened).map(|stamp| stamp.inode) else {
        return;
    };
    if let Some(at) = races
        .iter()
        .position(|(registered, _)| *registered == inode)
        && !(races[at].1)()
    {
        drop(races.remove(at));
    }
}

/// Bytes each read of a prefix read again takes.
const REREAD_CHUNK_BYTES: usize = 64 * 1024;

/// The reads of one file under the stable-read rule (see the module).
#[derive(Debug, Default)]
pub struct Settling {
    /// When a write first moved a read of the file.
    since: Option<std::time::Instant>,
}

impl Settling {
    /// What the read of `file` just made proved, its metadata `opened`
    /// taken before it. A file whose metadata cannot be taken proves none.
    pub fn settled(&mut self, file: &std::fs::File, opened: &std::fs::Metadata) -> Settled {
        #[cfg(any(test, feature = "test-hooks"))]
        before_proof(opened);
        let Ok(after) = file.metadata() else {
            return Settled::Unsettled;
        };
        if same_stamp(opened, &after) {
            return Settled::Stable(after);
        }
        if self.within_window(&after) {
            Settled::Again
        } else {
            Settled::Unsettled
        }
    }

    /// [`Self::settled`] of a read that took `taken()` of `file` (asked
    /// only when a write moved the read; `None`: it took nothing it can
    /// say). A read the stamp moved under, of a file that only grew, is
    /// proved by its prefix (see the module): its first bytes are read
    /// again while the settle window lasts and `live` holds, at least once.
    /// Bytes read again that are the file whole (a write that changed only
    /// its times) prove the read whole.
    pub fn settled_read<'a>(
        &mut self,
        file: &std::fs::File,
        opened: &std::fs::Metadata,
        taken: impl FnOnce() -> Option<Taken<'a>>,
        live: &dyn Fn() -> bool,
    ) -> Proved {
        #[cfg(any(test, feature = "test-hooks"))]
        before_proof(opened);
        let Ok(after) = file.metadata() else {
            return Proved::Unsettled;
        };
        if same_stamp(opened, &after) {
            return Proved::Whole(after);
        }
        // The window starts at the first read a write moved, this one when
        // no earlier one was: every read of the prefix below counts in it.
        self.since.get_or_insert_with(std::time::Instant::now);
        if let Some(taken) = taken().filter(|taken| only_grew(opened, &after, taken.len())) {
            loop {
                match reread(file, opened, taken) {
                    Reread::Whole(metadata) => return Proved::Whole(metadata),
                    Reread::Proved(prefix) => return Proved::Prefix(prefix),
                    Reread::Moved if self.within_window(&after) && live() => {}
                    Reread::Moved | Reread::Other => break,
                }
            }
        }
        if self.within_window(&after) {
            Proved::Again
        } else {
            Proved::Unsettled
        }
    }

    /// Whether the settle window since the first read a write moved, which
    /// this one is when no earlier one was, still lasts.
    fn within_window(&mut self, after: &std::fs::Metadata) -> bool {
        let window = SourceStamp::of(after).map_or(SourceStamp::SETTLE, |stamp| stamp.settle_for());
        let since = *self.since.get_or_insert_with(std::time::Instant::now);
        since.elapsed() < window
    }
}

/// Whether a read that took `len` bytes of a file opened as `opened`, with
/// metadata `after` once it ended, can be the start of a version it grew to:
/// the same inode, holding at least what the read took. (What proves it is
/// the read of those bytes again, [`reread`].)
fn only_grew(opened: &std::fs::Metadata, after: &std::fs::Metadata, len: u64) -> bool {
    let inode = |meta: &std::fs::Metadata| SourceStamp::of(meta).map(|stamp| stamp.inode);
    len > 0 && inode(opened) == inode(after) && after.len() >= len
}

/// The first bytes `taken` names, read again from the start of `file`
/// through the same handle, between two equal stamps (see the module).
fn reread(file: &std::fs::File, opened: &std::fs::Metadata, taken: Taken<'_>) -> Reread {
    let len = taken.len();
    let Ok(before) = file.metadata() else {
        return Reread::Other;
    };
    if !only_grew(opened, &before, len) {
        return Reread::Other;
    }
    let mut handle = file;
    if handle.seek(SeekFrom::Start(0)).is_err() {
        return Reread::Other;
    }
    #[cfg(test)]
    tests::during_prefix_reread();
    let (same, last, again) = match taken {
        Taken::Bytes(bytes) => (read_equal(handle, bytes), bytes.last().copied(), Vec::new()),
        Taken::Hashed { digest, keep, .. } => match read_hashed(handle, len, keep) {
            Some((read, last, kept)) => (read == digest, last, kept),
            None => (false, None, Vec::new()),
        },
    };
    let Ok(after) = file.metadata() else {
        return Reread::Other;
    };
    if !same_stamp(&before, &after) {
        return Reread::Moved;
    }
    if !same {
        Reread::Other
    } else if after.len() == len {
        Reread::Whole(after)
    } else if last == Some(b'\n') {
        Reread::Proved(Prefix { len, bytes: again })
    } else {
        Reread::Other
    }
}

/// Whether `reader` starts with `bytes`.
fn read_equal(mut reader: impl Read, bytes: &[u8]) -> bool {
    let mut chunk = vec![0u8; REREAD_CHUNK_BYTES.min(bytes.len())];
    let mut at = 0;
    while at < bytes.len() {
        let room = chunk.len().min(bytes.len() - at);
        match reader.read(&mut chunk[..room]) {
            Ok(0) => return false,
            Ok(read) if chunk[..read] != bytes[at..at + read] => return false,
            Ok(read) => at += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    true
}

/// The SHA-256 of the first `len` bytes of `reader`, its last byte, and,
/// with `keep`, the bytes; `None` when it holds fewer or cannot be read.
fn read_hashed(reader: impl Read, len: u64, keep: bool) -> Option<([u8; 32], Option<u8>, Vec<u8>)> {
    use sha2::{Digest as _, Sha256};
    let mut reader = reader.take(len);
    let mut hasher = Sha256::new();
    let mut kept = Vec::new();
    if keep {
        kept.try_reserve_exact(usize::try_from(len).ok()?).ok()?;
    }
    let mut chunk = vec![0u8; REREAD_CHUNK_BYTES];
    let (mut read, mut last) = (0u64, None);
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(got) => {
                hasher.update(&chunk[..got]);
                if keep {
                    kept.extend_from_slice(&chunk[..got]);
                }
                last = chunk[..got].last().copied();
                read += got as u64;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    (read == len).then(|| (hasher.finalize().into(), last, kept))
}

/// Why a read ended without bytes it could prove: its file kept changing
/// while it was read ([`Settled::Unsettled`]).
#[derive(Debug)]
struct ChangedWhileRead;

impl std::fmt::Display for ChangedWhileRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the file kept changing while it was read")
    }
}

impl std::error::Error for ChangedWhileRead {}

/// The one recovery of a read whose file kept changing while it was read
/// ([`changed_while_read`]): no other input reads it, a later read may.
pub const CHANGING_HINT: &str =
    "Retry once the file stops changing, or read a copy of it made while it is not being written.";

/// The error of a read whose file kept changing while it was read.
pub fn changed_while_read() -> std::io::Error {
    std::io::Error::other(ChangedWhileRead)
}

/// Whether `error` is [`changed_while_read`]'s.
pub fn is_changed_while_read(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.downcast_ref::<ChangedWhileRead>().is_some())
}

#[cfg(test)]
mod tests;
