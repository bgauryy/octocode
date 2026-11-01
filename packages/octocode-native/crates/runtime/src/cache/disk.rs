use super::{CacheClass, CacheConfig, CacheKey, DiskCodec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

mod capacity;

/// The extension of an entry file. Entries of the retired JSON-envelope
/// layout (`.json`, value base64 inside JSON) are removed by the next prune.
pub(super) const ENTRY_EXTENSION: &str = "entry";
const RETIRED_EXTENSION: &str = "json";

/// The first line of an entry file: how the entry ages. The codec's value
/// bytes follow the newline as they are, so a body is never re-encoded.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DiskHeader {
    pub(super) class: CacheClass,
    pub(super) stored_at_unix: u64,
}

impl DiskHeader {
    /// How old the entry in `file` is at `now` (unix seconds): since it was
    /// stored, or for an idle class since its last use, which every read
    /// records as the file's modification time ([`Disk::touch`]).
    pub(super) fn age(&self, file: &fs::File, now: u64) -> Duration {
        if self.class.idle().is_some()
            && let Ok(used) = file.metadata().and_then(|meta| meta.modified())
        {
            return SystemTime::now().duration_since(used).unwrap_or_default();
        }
        Duration::from_secs(now.saturating_sub(self.stored_at_unix))
    }

    /// The header line and the value bytes of an entry file.
    pub(super) fn split(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let end = bytes.iter().position(|byte| *byte == b'\n')?;
        let header = serde_json::from_slice(&bytes[..end]).ok()?;
        Some((header, &bytes[end + 1..]))
    }
}

/// Age past which a `.tmp` file in the store is a crash leftover; a live
/// write holds its temp file for milliseconds.
const ORPHAN_TEMP_AGE: Duration = Duration::from_secs(10 * 60);

/// Whether a failed read means the entry itself is unusable (a symlink, a
/// non-regular file, or past the entry bound), not that reading it failed for
/// an environmental reason such as permissions or open-file limits.
fn entry_is_invalid(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::InvalidInput {
        return true;
    }
    // `O_NOFOLLOW` reports a symlink swapped in after the type check as ELOOP.
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ELOOP) {
        return true;
    }
    false
}

/// How much of an entry file [`expired`] reads: its header line.
const HEADER_BYTES: usize = 512;

/// Whether the entry file at `path` is past its class's lifetime, read from
/// its header line alone, never its value. A file whose header does not
/// parse is not judged here (its read evicts it).
fn expired(path: &std::path::Path, now: u64) -> bool {
    let Ok(file) = crate::private_file::open_no_follow(path, false) else {
        return false;
    };
    let mut head = Vec::with_capacity(HEADER_BYTES);
    if std::io::Read::read_to_end(
        &mut std::io::Read::take(&file, HEADER_BYTES as u64),
        &mut head,
    )
    .is_err()
    {
        return false;
    }
    DiskHeader::split(&head).is_some_and(|(header, _)| {
        let age = header.age(&file, now);
        header.class.freshness(age).is_none()
    })
}

/// The wall clock as the header's unsigned unix seconds.
pub(super) fn now_unix() -> u64 {
    crate::civil_date::unix_secs().unsigned_abs()
}

/// Set `file`'s modification time to now. A read-only handle may not set
/// times (Windows), so a writer is the fallback.
fn touch_file(file: &fs::File, path: &std::path::Path) {
    let now = SystemTime::now();
    if file.set_modified(now).is_err()
        && let Ok(writer) = fs::File::options().write(true).open(path)
    {
        let _ = writer.set_modified(now);
    }
}

pub(crate) struct Disk {
    dir: PathBuf,
}

impl Disk {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn file(&self, key: &CacheKey) -> PathBuf {
        let mut digest = Sha256::new();
        // Domain separation: files of an older layout are never read.
        // v2: bodies are base64 only (the number-array form was retired).
        // v3: one opaque scope replaced the endpoint/credential pair.
        // v4: a header line, then the codec's value bytes (no JSON envelope).
        digest.update(b"octocode-cache-store-v4");
        for value in [&key.namespace, &key.scope, &key.resource] {
            digest.update((value.len() as u64).to_le_bytes());
            digest.update(value.as_bytes());
        }
        self.dir.join(format!(
            "{}.{ENTRY_EXTENSION}",
            hex::encode(digest.finalize())
        ))
    }

    pub(crate) fn read<V, C: DiskCodec<V>>(
        &self,
        key: &CacheKey,
        config: &CacheConfig,
    ) -> Option<(V, CacheClass, Duration, usize)> {
        let path = self.file(key);
        // An entry is only a regular file this store could have written: a
        // FIFO, device, symlink, or file past the entry bound is evicted.
        let read = crate::private_file::open_no_follow(&path, false).and_then(|file| {
            let bytes = crate::private_file::read_limited(&file, config.max_bytes as u64)?;
            Ok((file, bytes))
        });
        let (file, bytes) = match read {
            Ok(read) => read,
            Err(error) => {
                if entry_is_invalid(&error) {
                    let _ = fs::remove_file(&path);
                }
                return None;
            }
        };
        let Some((header, value)) =
            DiskHeader::split(&bytes).and_then(|(header, value)| Some((header, C::decode(value)?)))
        else {
            let _ = fs::remove_file(&path);
            return None;
        };
        let age = header.age(&file, now_unix());
        if header.class.freshness(age).is_none() {
            let _ = fs::remove_file(&path);
            return None;
        }
        // A read counts as use for the oldest-first pruning and restarts an
        // idle entry's lifetime.
        touch_file(&file, &path);
        Some((value, header.class, age, bytes.len()))
    }

    /// Record a use of `key`'s entry file: its modification time becomes now.
    pub(super) fn touch(&self, key: &CacheKey) {
        let path = self.file(key);
        if let Ok(file) = crate::private_file::open_no_follow(&path, false) {
            touch_file(&file, &path);
        }
    }

    pub(super) fn write<V, C: DiskCodec<V>>(
        &self,
        key: &CacheKey,
        value: &V,
        class: CacheClass,
        config: &CacheConfig,
    ) -> bool {
        let Some(value) = C::encode(value) else {
            return false;
        };
        self.write_encoded(key, &value, class, config)
    }

    pub(crate) fn write_encoded(
        &self,
        key: &CacheKey,
        value: &[u8],
        class: CacheClass,
        config: &CacheConfig,
    ) -> bool {
        let path = self.file(key);
        let header = DiskHeader {
            class,
            stored_at_unix: now_unix(),
        };
        let Ok(mut bytes) = serde_json::to_vec(&header) else {
            return false;
        };
        bytes.push(b'\n');
        self.publish(&path, [&bytes, value], config)
    }
}
