//! Reuse single-file extraction across outline pages. Authorization and source
//! decoding happen in the caller on every request; the key uses the decoded
//! content, canonical path and parser override, rather than mutable timestamps.

use crate::cache::{BoundedCache, CacheConfig, CacheKey, CacheLookup, CachePartition};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static CACHE: OnceLock<Mutex<BoundedCache<String>>> = OnceLock::new();

pub(super) fn extract(
    source: &str,
    canonical_path: &str,
    cpp_header: bool,
    extraction: impl FnOnce() -> Option<String>,
) -> Option<Arc<String>> {
    let key = CacheKey {
        namespace: "ast-declarations".into(),
        resource: hex::encode(Sha256::digest(source.as_bytes())),
        partition: CachePartition {
            endpoint: canonical_path.into(),
            credential_fingerprint: if cpp_header { "cpp" } else { "auto" }.into(),
        },
    };
    let cache = CACHE.get_or_init(|| {
        Mutex::new(BoundedCache::new(CacheConfig {
            max_entries: 128,
            max_bytes: 32 * 1024 * 1024,
            ttl: Duration::from_secs(120),
        }))
    });
    if let Ok(mut cache) = cache.lock()
        && let CacheLookup::Hit { value, .. } = cache.get(&key, 0, None, Instant::now())
    {
        return Some(value);
    }
    // Extraction can block or time out. Never hold the global cache mutex while
    // parsing, and never retain failed extraction as a successful empty outline.
    let raw = extraction()?;
    let bytes = raw.len() + canonical_path.len() + key.resource.len();
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key.clone(), raw.clone(), bytes, 0, Instant::now());
        if let CacheLookup::Hit { value, .. } = cache.get(&key, 0, None, Instant::now()) {
            return Some(value);
        }
    }
    Some(Arc::new(raw))
}

#[cfg(test)]
mod tests {
    use super::extract;
    use std::cell::Cell;

    #[test]
    fn content_path_and_parser_identity_control_reuse() {
        let calls = Cell::new(0);
        let parse = || {
            calls.set(calls.get() + 1);
            Some("declarations".to_owned())
        };
        let path = format!("cache-test-{}", std::process::id());
        let first = extract("fn one() {}", &path, false, parse).unwrap();
        let second = extract("fn one() {}", &path, false, parse).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(calls.get(), 1);
        extract("fn two() {}", &path, false, parse).unwrap();
        extract("fn one() {}", &format!("{path}-other"), false, parse).unwrap();
        extract("fn one() {}", &path, true, parse).unwrap();
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn failed_extraction_is_not_cached() {
        let path = format!("failed-cache-test-{}", std::process::id());
        assert!(extract("source", &path, false, || None).is_none());
        assert_eq!(
            extract("source", &path, false, || Some("retry".into()))
                .as_deref()
                .map(String::as_str),
            Some("retry")
        );
    }
}
