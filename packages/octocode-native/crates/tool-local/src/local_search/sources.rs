//! The reads a page makes again of the files a walk searched, and their
//! digests. A walked file may have been swapped for a FIFO or a symlink since
//! the walk: every re-read opens only a regular file and never follows a final
//! link ([`octocode_engine::text::read_regular`],
//! [`octocode_engine::text::open_regular`]), so it neither blocks on a FIFO
//! nor reads the bytes of a file the walk never admitted.

use octocode_engine::num::saturating_u64;

/// SHA-256 of a matched file's bytes.
pub(super) type Digest = [u8; 32];

/// What a re-read of a searched file must find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Expected {
    /// A fresh scan: its values were made from the bytes its search read,
    /// whose digest it carries ([`octocode_engine::types::SearchedSource`]).
    /// Only a read of those bytes judges the values; any other read, or a
    /// file the scan bound to no digest, shows each value judged alone.
    Fresh,
    /// A stored scan: the file must still hash to the digest stored with
    /// it; one stored without a digest reads as changed.
    Stored(Option<Digest>),
}

/// Streamed SHA-256 of the regular file at `path` (a FIFO or link fails).
pub(super) fn digest_file(path: &std::path::Path) -> std::io::Result<Digest> {
    let (file, _) = octocode_engine::text::open_regular(path)?;
    let (_, hex) = octocode_engine::digest::copy_hashed(file, std::io::sink())?;
    let mut digest = [0; 32];
    hex::decode_to_slice(hex, &mut digest).map_err(std::io::Error::other)?;
    Ok(digest)
}

/// The bytes of the regular file at `path`, at most `max_bytes`; `None` when
/// it is larger, unreadable, or no longer a regular file (a FIFO or link).
pub(super) fn read_entry(path: &std::path::Path, max_bytes: usize) -> Option<Vec<u8>> {
    octocode_engine::text::read_regular(path, saturating_u64(max_bytes))
        .ok()
        .map(|source| source.bytes)
}
