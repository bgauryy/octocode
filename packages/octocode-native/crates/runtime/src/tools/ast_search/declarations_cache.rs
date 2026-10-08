//! Reuse single-file extraction across outline pages. Authorization and source
//! decoding happen in the caller on every request; the key uses the decoded
//! content, canonical path and parser override, rather than mutable timestamps.

use crate::cache::{CacheClass, CacheConfig, CacheKey, Store, StorePartition};
use std::sync::{Arc, OnceLock};

static CACHE: OnceLock<Store<String>> = OnceLock::new();

pub(crate) fn extract(
    source: &str,
    canonical_path: &str,
    cpp_header: bool,
    extraction: impl FnOnce() -> Option<String>,
) -> Option<Arc<String>> {
    let key = CacheKey {
        namespace: "ast-declarations".into(),
        resource: crate::digest::sha256(source.as_bytes()),
        partition: StorePartition {
            endpoint: canonical_path.into(),
            credential_fingerprint: if cpp_header { "cpp" } else { "auto" }.into(),
        },
    };
    let cache = CACHE.get_or_init(|| {
        Store::new(
            CacheConfig {
                max_entries: 128,
                max_bytes: 32 * 1024 * 1024,
                ..CacheConfig::default()
            },
            None,
        )
    });
    if let Some(hit) = cache.get(&key) {
        return Some(hit.value);
    }
    // Extraction can block or time out. Never hold the cache lock while
    // parsing, and never retain failed extraction as a successful empty outline.
    let raw = extraction()?;
    let bytes = raw.len() + canonical_path.len() + key.resource.len();
    // Keyed by the source digest, so an entry never goes stale.
    cache.put(key.clone(), raw.clone(), bytes, CacheClass::Immutable);
    Some(
        cache
            .get(&key)
            .map_or_else(|| Arc::new(raw), |hit| hit.value),
    )
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
