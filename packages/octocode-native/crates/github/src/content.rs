//! GitHub file and tree reads through one transport: the cache seam, the
//! request/response types, and the provider that owns them.
use bytes::Bytes;
use std::{future::Future, pin::Pin, time::Duration};

use super::{GitHubTransport, Partition};

mod file;
mod listing;
mod payload;
mod reference;

pub use payload::decode_content;

/// A cached GitHub read; the host decides how it is stored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedContent {
    pub etag: Option<String>,
    /// Shared, so a cache hit hands out the stored body without copying it.
    pub bytes: Bytes,
    /// The commit a content read resolved; empty for other reads.
    pub resolved_ref: String,
    /// A listing page GitHub linked a next page from.
    pub has_more: bool,
}
/// How long a cached GitHub read stays good. The writer picks it at
/// [`ConditionalCache::put`]; the host maps it onto its own windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retention {
    /// Keyed by a resolved commit SHA (or a fact below one): never stale.
    Pinned,
    /// Served as is while fresh, then revalidated with its ETag (a 304 costs
    /// no rate limit); an entry without an ETag expires instead.
    Revalidate,
    /// Revalidated with its ETag on every read: mutable history.
    AlwaysRevalidate,
    /// Expires `ttl` after it is stored.
    Expires(Duration),
}

/// Cache seam for GitHub reads. A hit without an ETag is fresh: serve it as
/// is. A hit with an ETag must be revalidated with `If-None-Match` (a 304
/// costs no rate limit). Keys are `<namespace>:<resource>`.
pub trait ConditionalCache: Send + Sync {
    fn get<'a>(
        &'a self,
        partition: &'a Partition,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>>;
    fn put<'a>(
        &'a self,
        partition: &'a Partition,
        key: String,
        value: CachedContent,
        retention: Retention,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
#[derive(Default)]
pub struct NoCache;
impl ConditionalCache for NoCache {
    fn get<'a>(
        &'a self,
        _: &'a Partition,
        _: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>> {
        Box::pin(async { None })
    }
    fn put<'a>(
        &'a self,
        _: &'a Partition,
        _: String,
        _: CachedContent,
        _: Retention,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

#[derive(Clone, Debug)]
pub struct ContentRequest {
    pub owner: String,
    pub repo: String,
    pub path: String,
    pub reference: Option<String>,
    pub force_refresh: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentResponse {
    /// Shared with the cache entry: a hit copies no body.
    pub bytes: Bytes,
    pub resolved_ref: String,
    pub etag: Option<String>,
    pub from_cache: bool,
    pub raw_response_bytes: usize,
    /// The git blob SHA of the bytes, when the read named it (a cache hit
    /// does not): the evidence a caller cites when it refuses them.
    pub blob: Option<String>,
}
/// A submodule entry of a repository: the clone URL `.gitmodules` names
/// for it and the commit its parent pins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Submodule {
    pub url: Option<String>,
    pub commit: Option<String>,
}
/// GitHub reads through one transport; its response cache is the
/// transport's own (`transport.cache`).
pub struct GitHubProvider {
    pub transport: GitHubTransport,
}
impl GitHubProvider {
    /// A provider reading through `transport`, caching in `cache`.
    pub fn new(mut transport: GitHubTransport, cache: impl ConditionalCache + 'static) -> Self {
        transport.cache = std::sync::Arc::new(cache);
        Self { transport }
    }
}

/// A full 40-hex-digit commit SHA (either case).
pub fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
