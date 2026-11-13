//! Side-effect-free entry inspection, shared by reads and locked cleanup.
use super::{CacheConfig, DiskCodec, DiskHeader, HEADER_BYTES, fs, now_unix};
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use std::time::Duration;

pub(super) struct Stored<V> {
    pub(super) file: fs::File,
    pub(super) header: DiskHeader,
    pub(super) age: Duration,
    pub(super) bytes: usize,
    pub(super) value: V,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Invalid or expired cache entry",
    )
}

pub(super) fn entry<V, C: DiskCodec<V>>(
    path: &Path,
    config: &CacheConfig,
) -> io::Result<Stored<V>> {
    let file = crate::private_file::open_no_follow(path, false)?;
    if file.metadata()?.len() > config.disk_bytes {
        return Err(invalid());
    }
    let mut reader = BufReader::with_capacity(HEADER_BYTES, file);
    let mut header_bytes = Vec::with_capacity(HEADER_BYTES);
    // The writer's header is a small JSON line. A corrupt file cannot make
    // header inspection allocate or read its entire body.
    (&mut reader)
        .take(HEADER_BYTES as u64)
        .read_until(b'\n', &mut header_bytes)?;
    let (header, _) = DiskHeader::split(&header_bytes).ok_or_else(invalid)?;
    let age = header.age(reader.get_ref(), now_unix());
    if !header.class.is_live(age) {
        return Err(invalid());
    }
    let remaining = config.disk_bytes.saturating_sub(header_bytes.len() as u64);
    let body = crate::private_file::read_limited(&mut reader, remaining)?;
    let value = C::decode(&body).ok_or_else(invalid)?;
    Ok(Stored {
        file: reader.into_inner(),
        header,
        age,
        bytes: header_bytes.len() + body.len(),
        value,
    })
}
