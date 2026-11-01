use super::ast_search_query_symbols;
use super::directory::DirectoryWalk;
use super::select::SymbolSet;
use crate::cache::{CacheConfig, CacheKey, Store};
use crate::policy::prune::DefaultsFlag;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::json;

/// Page memo for directory outlines: one query shape over unchanged files.
pub(super) static SYMBOL_PAGES: std::sync::LazyLock<Store<SymbolSet>> =
    std::sync::LazyLock::new(|| {
        Store::new(
            CacheConfig {
                max_entries: 8,
                max_bytes: 64 * 1024 * 1024,
                ..CacheConfig::default()
            },
            None,
        )
    });

/// `key` in the partition of selections stored while their files were
/// `settled`.
pub(super) fn with_settled(mut key: CacheKey, settled: bool) -> CacheKey {
    key.namespace = if settled {
        "ast-symbol-pages/settled"
    } else {
        "ast-symbol-pages"
    }
    .into();
    key
}

/// The memo key: every query field except the page cursor, plus the
/// fingerprint of the files the outline scans, partitioned by whether every
/// one of them was last written over two seconds ago (returned too).
pub(super) fn directory_memo_key(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    walk: &DirectoryWalk,
) -> (CacheKey, bool) {
    let shape = json!([
        canonical.to_string_lossy(),
        ast_search_query_symbols::language_globs(q),
        q.symbol_filter,
        ast_search_query_symbols::kinds(q),
        ast_search_query_symbols::exclude(q),
        q.default_excludes.defaults(),
        q.include_ignored.unwrap_or(false),
        q.include_hidden.unwrap_or(false),
        q.include_provenance.unwrap_or(false),
        q.test_code,
        ast_search_query_symbols::scan_offset(q)
    ]);
    let (fingerprint, settled) = directory_fingerprint(walk);
    let key = CacheKey {
        namespace: "ast-symbol-pages".into(),
        scope: fingerprint,
        resource: crate::digest::json_sha256(&shape),
    };
    (with_settled(key, settled), settled)
}

/// Digest of the files a directory outline scans (its walk: an outlined
/// extension or a `parserGlobs` match, under the caller's path policy):
/// path, size and modification time; and whether every one was last
/// modified at least [`SourceStamp::SETTLE`] ago.
///
/// [`SourceStamp::SETTLE`]: octocode_engine::stamp::SourceStamp::SETTLE
fn directory_fingerprint(walk: &DirectoryWalk) -> (String, bool) {
    let now_ms = octocode_engine::stamp::settle_clock()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_millis() as f64);
    let settle_ms = octocode_engine::stamp::SourceStamp::SETTLE.as_millis() as f64;
    let mut stats = walk
        .files
        .entries
        .iter()
        .map(|entry| {
            (
                &entry.relative_path,
                entry.size,
                entry.modified_ms().map(f64::to_bits),
            )
        })
        .collect::<Vec<_>>();
    let settled = stats.iter().all(|(_, _, modified)| {
        modified.is_some_and(|bits| f64::from_bits(bits) + settle_ms <= now_ms)
    });
    stats.sort_unstable();
    (crate::digest::json_sha256(&json!(stats)), settled)
}
