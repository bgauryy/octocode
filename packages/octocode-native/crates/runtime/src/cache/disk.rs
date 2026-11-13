use super::{CacheClass, CacheConfig, CacheKey, DiskCodec};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

mod capacity;
mod read;
pub(super) use capacity::REUSE_LOCK_WAIT;
#[cfg(test)]
pub(super) use capacity::SCANS;

/// The extension of an entry file. Entries of the retired JSON-envelope
/// layout (`.json`, value base64 inside JSON) are removed by the next prune.
pub(super) const ENTRY_EXTENSION: &str = "entry";
const RETIRED_EXTENSION: &str = "json";
/// The extension of a hard link another store's entry is shared through
/// ([`Disk::link`]): its header is that store's, so its age is this store's.
pub(super) const LINK_EXTENSION: &str = "link";

/// The name before an entry's stem in the name of its lock file ([`Disk::entry_lock`]).
pub(super) const ENTRY_LOCK_PREFIX: &str = "assessment-";

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
    /// records as the file's modification time .
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

/// An idle entry's use is recorded once it has aged this fraction of its
/// lifetime: a read can shorten what it keeps by at most this share.
const TOUCH_FRACTION: u32 = 64;

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
        !header.class.is_live(age)
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
        let mut digest = octocode_engine::digest::HashWriter::default();
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

    /// The lock file of `key`'s entry, for a store whose writers take one per
    /// entry. It is named by the entry's own stem, so a scan can tell it has
    /// outlived the entry ([`super::disk::capacity`]).
    pub(crate) fn entry_lock(&self, key: &CacheKey) -> PathBuf {
        let entry = self.file(key);
        let stem = entry
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.dir.join(format!("{ENTRY_LOCK_PREFIX}{stem}.lock"))
    }

    /// Whether `key`'s entry exists and is live; if so it counts as used now.
    pub(super) fn touch(&self, key: &CacheKey) -> bool {
        let path = self.file(key);
        // A use of an expired entry restarts nothing: its lifetime is over,
        // and a touch would serve it again until a scan removes it.
        if expired(&path, now_unix()) {
            return false;
        }
        crate::private_file::open_no_follow(&path, false)
            .map(|file| touch_file(&file, &path))
            .is_ok()
    }

    /// Give this directory its own hard link to the entry `source` holds under
    /// `key`. The file then lives while either directory keeps its link: each
    /// expires and evicts its own, as it would a copy, and the other's reader
    /// is unaffected; a replaced entry is a new file, so a link keeps what it
    /// linked. False (another filesystem, no link support) means no link.
    pub(crate) fn link(&self, source: &Self, key: &CacheKey, config: &CacheConfig) -> bool {
        self.publish_link(&source.file(key), &self.link_file(key), config)
    }

    /// The value bytes of this directory's link for `key`. Every read restarts
    /// the link's lifetime, so a link outlives what its reader read it for.
    pub(crate) fn read_link(&self, key: &CacheKey, config: &CacheConfig) -> Option<Vec<u8>> {
        let path = self.link_file(key);
        let read = crate::private_file::open_no_follow(&path, false).and_then(|file| {
            let bytes = crate::private_file::read_limited(&file, config.disk_bytes)?;
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
        touch_file(&file, &path);
        DiskHeader::split(&bytes).map(|(_, value)| value.to_vec())
    }

    fn link_file(&self, key: &CacheKey) -> PathBuf {
        self.file(key).with_extension(LINK_EXTENSION)
    }

    pub(crate) fn read<V, C: DiskCodec<V>>(
        &self,
        key: &CacheKey,
        config: &CacheConfig,
    ) -> Option<(V, CacheClass, Duration, usize)> {
        let stored = match read::entry::<V, C>(&self.file(key), config) {
            Ok(stored) => stored,
            Err(error) => {
                if entry_is_invalid(&error) {
                    self.discard_invalid::<V, C>(key, config);
                }
                return None;
            }
        };
        // Hot idle reads avoid a metadata write; the stored header still
        // decides whether a volatile value has expired.
        if stored
            .header
            .class
            .idle()
            .is_none_or(|ttl| stored.age >= ttl / TOUCH_FRACTION)
        {
            touch_file(&stored.file, &self.file(key));
        }
        Some((stored.value, stored.header.class, stored.age, stored.bytes))
    }

    pub(super) fn write<V, C: DiskCodec<V>>(
        &self,
        key: &CacheKey,
        value: &V,
        class: CacheClass,
        config: &CacheConfig,
        deadline: Instant,
    ) -> bool {
        let Some(value) = C::encode(value) else {
            return false;
        };
        self.write_encoded(key, &value, class, config, deadline)
    }

    pub(crate) fn write_encoded(
        &self,
        key: &CacheKey,
        value: &[u8],
        class: CacheClass,
        config: &CacheConfig,
        deadline: Instant,
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
        self.publish(&path, [&bytes, value], config, deadline)
    }
}
