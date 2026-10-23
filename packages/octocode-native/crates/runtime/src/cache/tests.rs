use super::*;
use crate::cache::disk::DiskHeader;
use crate::cache::disk::ENTRY_EXTENSION;
use std::path::Path;
mod disk;
mod memory;

fn key(resource: &str, credential: &str) -> CacheKey {
    CacheKey {
        namespace: "test".into(),
        scope: format!("https://api.github.com#{credential}"),
        resource: resource.into(),
    }
}

/// A listing fresh for `seconds`, and a search page living as long.
fn revalidate(seconds: u64) -> CacheClass {
    CacheClass::Revalidate {
        fresh: Duration::from_secs(seconds),
    }
}

fn volatile(seconds: u64) -> CacheClass {
    CacheClass::Volatile {
        ttl: Duration::from_secs(seconds),
    }
}

/// The entry files directly under `dir`.
pub(crate) fn entry_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == ENTRY_EXTENSION))
        .collect()
}

/// Rewind every entry under `dir` by `seconds`: its header's store time.
pub(crate) fn age_disk_entries(dir: &Path, seconds: u64) {
    for file in entry_files(dir) {
        let bytes = fs::read(&file).unwrap();
        let (header, value) = DiskHeader::split(&bytes).unwrap();
        let header = DiskHeader {
            stored_at_unix: header.stored_at_unix - seconds,
            ..header
        };
        let mut rewritten = serde_json::to_vec(&header).unwrap();
        rewritten.push(b'\n');
        rewritten.extend_from_slice(value);
        fs::write(&file, rewritten).unwrap();
    }
}
