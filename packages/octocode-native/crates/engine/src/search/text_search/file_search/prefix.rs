//! Reading and classifying the pre-NUL prefix of one opened file.

/// Leading bytes a format header occupies. A first NUL inside them, before
/// any line break, ends a binary signature (font, database, archive magic),
/// not text cut short.
const LEADING_BINARY_BYTES: usize = 1024;

/// Whether a file quit at its first NUL is binary from its leading bytes,
/// and so outside a text search: the bytes before the NUL are binary by the
/// one sniff ([`crate::text::sniff`]), or they are a short single-line header
/// that matched nothing. A match, a complete text line, or a long text run
/// before the NUL is searchable text the cut leaves unread.
pub(super) fn is_leading_binary(prefix: &[u8], matched: bool) -> bool {
    crate::text::sniff(prefix) == crate::text::BinaryVerdict::Binary
        || (!matched && prefix.len() < LEADING_BINARY_BYTES && !prefix.contains(&b'\n'))
}

/// Read the first `len` bytes of an already-open file.
pub(in super::super) fn read_prefix(file: &std::fs::File, len: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = file;
    handle.seek(SeekFrom::Start(0))?;
    let mut prefix = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    handle.take(len).read_to_end(&mut prefix)?;
    Ok(prefix)
}
