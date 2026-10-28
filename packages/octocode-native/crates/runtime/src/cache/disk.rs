use super::{CacheClass, CacheConfig, CacheKey, DiskCodec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Disk scans are O(files); amortize them across writes.
const PRUNE_EVERY_WRITES: usize = 64;
/// Cross-process throttle: one-shot CLI processes each write a few entries, so
/// a marker file bounds pruning to once per interval across processes.
const PRUNE_INTERVAL: Duration = Duration::from_secs(60);
pub(super) const PRUNE_MARKER: &str = ".last-prune";

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
        let age = Duration::from_secs(now.saturating_sub(header.stored_at_unix));
        header.class.freshness(age).is_none()
    })
}

/// The wall clock as the header's unsigned unix seconds.
pub(super) fn now_unix() -> u64 {
    crate::civil_date::unix_secs().unsigned_abs()
}

pub(crate) struct Disk {
    dir: PathBuf,
    /// Writes by this process: pruning runs on the first and then every
    /// [`PRUNE_EVERY_WRITES`] writes instead of scanning per write.
    writes: AtomicUsize,
}

impl Disk {
    pub(crate) fn new(dir: PathBuf) -> Self {
        let _ = octocode_io::atomic::create_private_dir_all(&dir);
        Self {
            dir,
            writes: AtomicUsize::new(0),
        }
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
        let age = Duration::from_secs(now_unix().saturating_sub(header.stored_at_unix));
        if header.class.freshness(age).is_none() {
            let _ = fs::remove_file(&path);
            return None;
        }
        // A read counts as use for the oldest-first pruning. A read-only
        // handle may not set times (Windows), so a writer is the fallback.
        let now = SystemTime::now();
        if file.set_modified(now).is_err()
            && let Ok(writer) = fs::File::options().write(true).open(&path)
        {
            let _ = writer.set_modified(now);
        }
        Some((value, header.class, age, bytes.len()))
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
        bytes.extend_from_slice(value);
        if bytes.len() > config.max_bytes {
            return false;
        }
        // Owner-only replacement: values may be private source, and an
        // in-place rewrite would keep a pre-existing file's loose mode.
        if octocode_io::atomic::write_atomic(&path, &bytes, false).is_err() {
            return false;
        }
        if self
            .writes
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(PRUNE_EVERY_WRITES)
            && self.prune_due()
        {
            self.prune(config.disk_bytes);
        }
        path.is_file()
    }

    /// True when no process pruned within [`PRUNE_INTERVAL`]; claims the slot
    /// by touching the marker.
    fn prune_due(&self) -> bool {
        let marker = self.dir.join(PRUNE_MARKER);
        let recent = fs::metadata(&marker)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed < PRUNE_INTERVAL);
        if recent {
            return false;
        }
        let _ = fs::write(&marker, b"");
        true
    }

    /// Remove expired entries, then keep the directory within `budget`
    /// bytes, removing the least recently used files first.
    pub(crate) fn prune(&self, budget: u64) {
        let Ok(read_dir) = fs::read_dir(&self.dir) else {
            return;
        };
        let now = SystemTime::now();
        let unix_now = now_unix();
        let mut files: Vec<(SystemTime, u64, PathBuf)> = read_dir
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let meta = entry.metadata().ok()?;
                let modified = meta.modified().ok()?;
                match path.extension().and_then(|ext| ext.to_str()) {
                    // An entry past its `Volatile` TTL is dead weight a read
                    // would delete anyway: it goes before any live one.
                    Some(ENTRY_EXTENSION) if expired(&path, unix_now) => {
                        let _ = fs::remove_file(&path);
                        None
                    }
                    Some(ENTRY_EXTENSION) => Some((modified, meta.len(), path)),
                    // No reader understands the retired layout any more.
                    Some(RETIRED_EXTENSION) => {
                        let _ = fs::remove_file(&path);
                        None
                    }
                    // A temp file older than any write could take is a crash
                    // leftover from between the temp write and the rename.
                    Some("tmp") => {
                        if now
                            .duration_since(modified)
                            .is_ok_and(|age| age > ORPHAN_TEMP_AGE)
                        {
                            let _ = fs::remove_file(&path);
                        }
                        None
                    }
                    _ => None,
                }
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        if total <= budget {
            return;
        }
        files.sort_by_key(|(mtime, _, _)| *mtime);
        for (_, len, path) in files {
            if total <= budget {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }
}
