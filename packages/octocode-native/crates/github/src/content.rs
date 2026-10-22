use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use reqwest::header::{ACCEPT, CONTENT_TYPE, ETAG, HeaderValue, IF_NONE_MATCH};
use serde::Deserialize;
use std::{future::Future, pin::Pin, time::Duration};

use super::{
    GitHubTransport, Partition, ProviderError, ProviderErrorKind, RequestContext, RequestSpec,
    ResponsePage,
};

/// GitHub's raw media type: a file's own bytes instead of a JSON payload.
const RAW_MEDIA_TYPE: &str = "application/vnd.github.raw";

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
#[derive(Clone, Copy)]
enum ContentKind {
    Text,
    Bytes,
}
impl GitHubProvider {
    /// A provider reading through `transport`, caching in `cache`.
    pub fn new(mut transport: GitHubTransport, cache: impl ConditionalCache + 'static) -> Self {
        transport.cache = std::sync::Arc::new(cache);
        Self { transport }
    }

    pub async fn get_file_content(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<ContentResponse, ProviderError> {
        self.read_file(request, context, ContentKind::Text).await
    }

    /// Original bytes for callers that validate their own media format.
    /// Shares text reads' authorization, commit identity, cache and single flight.
    pub async fn get_file_bytes(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<ContentResponse, ProviderError> {
        self.read_file(request, context, ContentKind::Bytes).await
    }

    async fn read_file(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
        kind: ContentKind,
    ) -> Result<ContentResponse, ProviderError> {
        validate_name(&request.owner)?;
        validate_name(&request.repo)?;
        if request.path.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                "file path is required",
            ));
        }
        let resolved_ref = self.resolve_commit(request, context).await?;
        let partition = self.transport.cache_partition(context, None)?;
        let key = cache_key(request, &resolved_ref);
        // Single flight: identical concurrent reads wait for the first and
        // then read what it cached; a waiter stops on its own cancellation
        // or deadline, not the holder's.
        let ticket = content_flights().join((partition.clone(), key.clone()));
        let _turn = context.budget.wait(ticket.turn()).await?;
        // The cache key holds the resolved commit SHA, so a cached body is
        // immutable: it is served without asking GitHub again. A movable ref
        // stays fresh through its own resolution, not through this entry.
        if !request.force_refresh
            && let Some(value) = self.transport.cache.get(&partition, &key).await
        {
            kind.check(&value.bytes, None)?;
            return Ok(ContentResponse {
                bytes: value.bytes,
                resolved_ref: value.resolved_ref,
                etag: value.etag,
                from_cache: true,
                raw_response_bytes: 0,
            });
        }
        let spec = self.contents_spec(request, &resolved_ref)?;
        let page = match self.transport.execute(spec, context).await {
            Ok(page) => page,
            Err(error) if error.status == Some(413) => {
                let (bytes, raw_response_bytes) = self
                    .fetch_via_directory_and_blob(request, &resolved_ref, context, kind)
                    .await?;
                let stored = CachedContent {
                    etag: None,
                    bytes: bytes.clone(),
                    resolved_ref: resolved_ref.clone(),
                    has_more: false,
                };
                self.transport
                    .cache
                    .put(&partition, key, stored, Retention::Pinned)
                    .await;
                return Ok(ContentResponse {
                    bytes,
                    resolved_ref,
                    etag: None,
                    from_cache: false,
                    raw_response_bytes,
                });
            }
            Err(error) => return Err(error),
        };
        let (bytes, raw_response_bytes) = self.page_bytes(request, &page, context, kind).await?;
        let etag = page
            .headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let stored = CachedContent {
            etag: etag.clone(),
            bytes: bytes.clone(),
            resolved_ref: resolved_ref.clone(),
            has_more: false,
        };
        self.transport
            .cache
            .put(&partition, key, stored, Retention::Pinned)
            .await;
        Ok(ContentResponse {
            bytes,
            resolved_ref,
            etag,
            from_cache: false,
            raw_response_bytes,
        })
    }

    /// The submodule at `path` of `owner/repo` at commit `reference`, from
    /// its own contents entry; `None` when the path is no submodule.
    pub async fn submodule(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<Option<Submodule>, ProviderError> {
        let mut url = self
            .transport
            .endpoint()
            .rest(&["repos", owner, repo, "contents", path])?;
        url.query_pairs_mut().append_pair("ref", reference);
        let page = self
            .transport
            .execute(RequestSpec::get(url), context)
            .await?;
        let payload: ContentPayload = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
        })?;
        Ok(
            (payload.kind.as_deref() == Some("submodule")).then_some(Submodule {
                url: payload.submodule_git_url,
                commit: payload.sha,
            }),
        )
    }

    /// The contents request for `request` at `resolved_ref`, in the raw
    /// media type: a file (up to 100 MB) comes back as its own bytes, with
    /// no base64 body and no second blob read; a directory, symlink entry,
    /// or submodule still comes back as JSON, which types it.
    fn contents_spec(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
    ) -> Result<RequestSpec, ProviderError> {
        let mut url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "contents",
            &request.path,
        ])?;
        url.query_pairs_mut().append_pair("ref", resolved_ref);
        let mut spec = RequestSpec::get(url);
        spec.headers
            .insert(ACCEPT, HeaderValue::from_static(RAW_MEDIA_TYPE));
        Ok(spec)
    }

    /// The file bytes a contents response carries, and the raw bytes read
    /// for them. A raw-typed response is the file itself (its ETag is the
    /// blob SHA). A JSON one is a non-file entry, or a base64 file payload
    /// from a server that ignored the raw media type; in that payload, files
    /// between 1 MB and 100 MB come back with `encoding:"none"` and an empty
    /// body, and their bytes are only available as a git blob.
    async fn page_bytes(
        &self,
        request: &ContentRequest,
        page: &ResponsePage,
        context: &RequestContext,
        kind: ContentKind,
    ) -> Result<(Bytes, usize), ProviderError> {
        let body = page.body.as_ref();
        let raw_response_bytes = body.len();
        let raw = page
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(RAW_MEDIA_TYPE));
        if raw {
            let blob = page
                .headers
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .map(|etag| etag.trim_start_matches("W/").trim_matches('"'))
                .filter(|sha| is_full_sha(sha));
            kind.check(body, blob)?;
            return Ok((page.body.clone(), raw_response_bytes));
        }
        let payload = parse_content_payload(body, &request.path)?;
        let needs_blob = payload.encoding.as_deref() == Some("none")
            || (payload.content.as_deref().is_none_or(str::is_empty)
                && payload.size.unwrap_or(0) > 0);
        match payload.sha.as_deref().filter(|_| needs_blob) {
            Some(sha) => {
                let (bytes, blob_bytes) = self.fetch_blob(request, sha, context, kind).await?;
                Ok((bytes, raw_response_bytes.saturating_add(blob_bytes)))
            }
            None => {
                let bytes = decode_content(payload.encoding.as_deref(), payload.content)?;
                kind.check(&bytes, payload.sha.as_deref())?;
                Ok((bytes.into(), raw_response_bytes))
            }
        }
    }

    pub async fn repository_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<super::ContentsListing, ProviderError> {
        let partition = self.transport.cache_partition(context, None)?;
        let key = super::cache_key::fields_key("github-tree", &[owner, repo, path, reference]);
        // A listing at a full commit SHA is immutable: a cached one is served
        // without asking GitHub again, like a file read at one.
        let pinned = is_full_sha(reference);
        let cached = self.transport.cache.get(&partition, &key).await;
        if let Some(value) = cached
            .as_ref()
            .filter(|value| pinned || value.etag.is_none())
        {
            return super::tree::parse_contents_listing(&value.bytes);
        }
        let mut segments = vec!["repos", owner, repo, "contents"];
        if !path.is_empty() && path != "." {
            segments.push(path);
        }
        let mut url = self.transport.endpoint().rest(&segments)?;
        url.query_pairs_mut().append_pair("ref", reference);
        let mut spec = RequestSpec::get(url);
        if let Some(etag) = cached.as_ref().and_then(|value| value.etag.as_ref()) {
            spec.headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Validation, "invalid cached ETag")
                })?,
            );
        }
        let page = self.transport.execute(spec, context).await?;
        let body = if page.status == 304 {
            cached
                .ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorKind::Decode,
                        "GitHub returned 304 without cached tree content",
                    )
                })?
                .bytes
        } else {
            Bytes::copy_from_slice(&page.body)
        };
        let listing = super::tree::parse_contents_listing(&body)?;
        if page.status != 304 {
            let etag = page
                .headers
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            self.transport
                .cache
                .put(
                    &partition,
                    key,
                    CachedContent {
                        etag,
                        bytes: body,
                        resolved_ref: reference.to_owned(),
                        has_more: false,
                    },
                    if pinned {
                        Retention::Pinned
                    } else {
                        Retention::Revalidate
                    },
                )
                .await;
        }
        Ok(listing)
    }

    async fn resolve_commit(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        self.resolve_reference(
            &request.owner,
            &request.repo,
            request.reference.as_deref(),
            request.force_refresh,
            context,
        )
        .await
    }

    /// Resolve `reference` (branch, tag, short SHA, or the default branch when
    /// `None`) to a lowercase 40-hex commit SHA in one round trip: the
    /// `vnd.github.sha` media type returns just the SHA (40 B instead of the
    /// full commit with patches), and `HEAD` resolves the default branch
    /// without a separate `GET /repos/{o}/{r}`. Movable refs are memoized for
    /// [`REF_MEMO_TTL`] in the credential partition so a batch of reads on
    /// one ref resolves it once; `force_refresh` bypasses the memo.
    pub async fn resolve_reference(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
        force_refresh: bool,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        if let Some(reference) = reference
            && is_full_sha(reference)
        {
            return Ok(reference.to_ascii_lowercase());
        }
        let reference = reference.unwrap_or("HEAD");
        let partition = self.transport.cache_partition(context, None)?;
        let key = ref_memo_key(owner, repo, reference);
        // Single flight: a concurrent batch on one ref waits (within its own
        // budget) for the first resolution and then reads it from the memo.
        let ticket = ref_flights().join((partition.clone(), key.clone()));
        let _turn = context.budget.wait(ticket.turn()).await?;
        if !force_refresh
            && let Some(sha) = self
                .transport
                .cache
                .get(&partition, &key)
                .await
                .and_then(|value| fresh_ref_memo(&value))
        {
            return Ok(sha);
        }
        let sha = self
            .transport
            .commit_sha(owner, repo, reference, context)
            .await?;
        self.transport
            .cache
            .put(
                &partition,
                key,
                CachedContent {
                    etag: None,
                    bytes: Bytes::copy_from_slice(sha.as_bytes()),
                    resolved_ref: String::new(),
                    has_more: false,
                },
                Retention::Expires(REF_MEMO_TTL),
            )
            .await;
        Ok(sha)
    }

    async fn fetch_via_directory_and_blob(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
        kind: ContentKind,
    ) -> Result<(Bytes, usize), ProviderError> {
        let (parent, name) = request.path.rsplit_once('/').unwrap_or(("", &request.path));
        let mut directory_url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "contents",
            parent,
        ])?;
        directory_url
            .query_pairs_mut()
            .append_pair("ref", resolved_ref);
        let directory = self
            .transport
            .execute(RequestSpec::get(directory_url), context)
            .await?;
        let entries: Vec<DirectoryEntry> =
            serde_json::from_slice(&directory.body).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub directory response",
                )
            })?;
        let entry = entries
            .into_iter()
            .find(|entry| entry.kind == "file" && entry.name == name)
            .ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::NotFound,
                    "file was not present in its parent directory",
                )
            })?;
        let (bytes, blob_bytes) = self.fetch_blob(request, &entry.sha, context, kind).await?;
        Ok((bytes, directory.body.len().saturating_add(blob_bytes)))
    }

    /// A blob's bytes as GitHub stores them: the raw media type answers
    /// with the bytes themselves (up to 100 MB), not a base64 JSON body a
    /// third larger that must then be decoded. The response buffer is the
    /// answer, shared, never copied.
    async fn fetch_blob(
        &self,
        request: &ContentRequest,
        sha: &str,
        context: &RequestContext,
        kind: ContentKind,
    ) -> Result<(Bytes, usize), ProviderError> {
        let blob_url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "blobs",
            sha,
        ])?;
        let mut spec = RequestSpec::get(blob_url);
        spec.headers
            .insert(ACCEPT, HeaderValue::from_static(RAW_MEDIA_TYPE));
        let blob = self.transport.execute(spec, context).await?;
        kind.check(&blob.body, Some(sha))?;
        let size = blob.body.len();
        Ok((blob.body, size))
    }
}

/// Parse a Contents API response for a single file. Directories (arrays),
/// symlinks and submodules get their own actionable validation errors instead
/// of an opaque decode failure.
impl GitHubTransport {
    /// Resolve `reference` (branch, tag, short SHA, or `HEAD`) to its lowercase
    /// 40-hex commit SHA in one round trip: the `vnd.github.sha` media type
    /// returns just the SHA. A 404 means the repository itself did not
    /// resolve; GitHub answers a missing ref in an existing repository with
    /// 422 "No commit found". Both failures carry a typed reason.
    pub async fn commit_sha(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "commits", reference])?;
        // A full SHA that resolved once always resolves to itself: cache the
        // hit (commit-pinned namespace, partitioned by credential).
        let cached = if crate::is_full_sha(reference) {
            let partition = self.cache_partition(context, None)?;
            let key = format!("github-commit:sha:{owner}/{repo}/{reference}");
            if let Some(hit) = self.cache.get(&partition, &key).await
                && let Ok(sha) = std::str::from_utf8(&hit.bytes)
            {
                return Ok(sha.to_owned());
            }
            Some((partition, key))
        } else {
            None
        };
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.sha"),
        );
        let page = self
            .execute(spec, context)
            .await
            .map_err(classify_ref_failure)?;
        let sha = parse_commit_sha(&page.body)?;
        if let Some((partition, key)) = cached {
            self.cache
                .put(
                    &partition,
                    key,
                    super::CachedContent {
                        etag: None,
                        bytes: sha.clone().into_bytes().into(),
                        resolved_ref: String::new(),
                        has_more: false,
                    },
                    Retention::Pinned,
                )
                .await;
        }
        Ok(sha)
    }
}

/// Type a ref-resolution failure: a bare 404 is the repository (missing,
/// private, or hidden from the token); the transport already typed a missing
/// ref ("No commit found") as `RefNotFound`.
fn classify_ref_failure(error: ProviderError) -> ProviderError {
    match error.status {
        Some(404) if error.reason.is_none() => {
            let mut error = error.with_reason(super::ProviderErrorReason::RepositoryNotFound);
            // GitHub links its commit docs; the failure is the repository.
            error.documentation_url =
                Some("https://docs.github.com/rest/repos/repos#get-a-repository".into());
            error
        }
        _ => error,
    }
}

fn parse_content_payload(body: &[u8], path: &str) -> Result<ContentPayload, ProviderError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
    })?;
    if value.is_array() {
        return Err(is_a_directory(path));
    }
    let payload: ContentPayload = serde_json::from_value(value).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
    })?;
    match payload.kind.as_deref() {
        Some("file") => Ok(payload),
        Some("dir") => Err(is_a_directory(path)),
        Some("symlink") => Err(not_a_file(match payload.target.as_deref() {
            Some(target) => format!(
                "Path \"{path}\" is a symlink to \"{target}\"; read the target path instead."
            ),
            None => format!("Path \"{path}\" is a symlink; read its target path instead."),
        })),
        Some("submodule") => Err(not_a_file(match payload.submodule_git_url.as_deref() {
            Some(url) => format!(
                "Path \"{path}\" is a git submodule ({url}); read files from the submodule repository instead."
            ),
            None => format!(
                "Path \"{path}\" is a git submodule; read files from the submodule repository instead."
            ),
        })
        .with_reason(super::ProviderErrorReason::Submodule)),
        _ => Err(not_a_file(format!("GitHub path \"{path}\" is not a file"))),
    }
}
/// A directory read, typed so `runtime::github::file_error` keys its
/// tree-listing recovery on the reason rather than the message.
fn is_a_directory(path: &str) -> ProviderError {
    not_a_file(format!("Path \"{path}\" is a directory, not a file"))
        .with_reason(super::ProviderErrorReason::PathIsDirectory)
}
fn not_a_file(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}
#[derive(Deserialize)]
struct ContentPayload {
    #[serde(rename = "type")]
    kind: Option<String>,
    encoding: Option<String>,
    content: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    submodule_git_url: Option<String>,
}
#[derive(Deserialize)]
struct CommitPayload {
    sha: String,
}
/// Seconds a movable ref → SHA resolution is reused. Short enough that a push
/// shows up on the next research step; long enough to cover one batch.
const REF_MEMO_TTL: Duration = Duration::from_secs(60);
/// A full 40-hex-digit commit SHA (either case).
pub fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// The process's in-flight content reads, one per `(credential, read)`.
fn content_flights() -> &'static super::KeyedFlight<(Partition, String), ()> {
    static FLIGHTS: super::KeyedFlight<(Partition, String), ()> = super::KeyedFlight::transient();
    &FLIGHTS
}
/// The process's in-flight ref resolutions, one per `(credential, ref)`.
fn ref_flights() -> &'static super::KeyedFlight<(Partition, String), ()> {
    static FLIGHTS: super::KeyedFlight<(Partition, String), ()> = super::KeyedFlight::transient();
    &FLIGHTS
}
fn ref_memo_key(owner: &str, repo: &str, reference: &str) -> String {
    // Owner/repo names are case-insensitive on GitHub; refs are not.
    super::cache_key::fields_key(
        "github-ref",
        &[
            owner.to_ascii_lowercase().as_str(),
            repo.to_ascii_lowercase().as_str(),
            reference,
        ],
    )
}
/// The SHA a ref memo entry holds; the cache expires it after
/// [`REF_MEMO_TTL`].
fn fresh_ref_memo(value: &CachedContent) -> Option<String> {
    let sha = std::str::from_utf8(&value.bytes).ok()?;
    is_full_sha(sha).then(|| sha.to_ascii_lowercase())
}
/// `vnd.github.sha` answers with the bare SHA; GHES versions (and test
/// fixtures) that ignore the media type answer with the commit JSON.
fn parse_commit_sha(body: &[u8]) -> Result<String, ProviderError> {
    let text = std::str::from_utf8(body).unwrap_or_default().trim();
    let sha = if is_full_sha(text) {
        text.to_owned()
    } else {
        serde_json::from_slice::<CommitPayload>(body)
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub commit response")
            })?
            .sha
    };
    if !is_full_sha(&sha) {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            "GitHub returned an invalid commit SHA",
        ));
    }
    Ok(sha.to_ascii_lowercase())
}
#[derive(Deserialize)]
struct DirectoryEntry {
    name: String,
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}
/// The bytes of a Contents API body by its `encoding` (`base64`, wrapped
/// at line breaks, or `utf-8`); any other encoding (`none` for a body over
/// 1 MB) is a decode error.
pub fn decode_content(
    encoding: Option<&str>,
    content: Option<String>,
) -> Result<Vec<u8>, ProviderError> {
    let bytes = match encoding {
        Some("base64") => STANDARD
            .decode(content.unwrap_or_default().replace(['\r', '\n'], ""))
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid base64 file content")
            })?,
        Some("utf-8") => content.unwrap_or_default().into_bytes(),
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "unsupported GitHub content encoding",
            ));
        }
    };
    Ok(bytes)
}
/// Text content only: a NUL byte marks binary content, reported with its size
/// and blob SHA as a request the file read cannot serve (not a decode fault).
impl ContentKind {
    fn check(self, bytes: &[u8], blob_sha: Option<&str>) -> Result<(), ProviderError> {
        if matches!(self, Self::Bytes) || !bytes.contains(&0) {
            return Ok(());
        }
        let blob = blob_sha
            .map(|sha| format!(", blob {sha}"))
            .unwrap_or_default();
        Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!("Binary file ({} bytes{blob})", bytes.len()),
        )
        .with_reason(super::ProviderErrorReason::BinaryFile))
    }
}
fn validate_name(value: &str) -> Result<(), ProviderError> {
    if value.is_empty() || value == "." || value == ".." || value.contains('/') {
        Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "invalid GitHub owner or repository name",
        ))
    } else {
        Ok(())
    }
}
fn cache_key(request: &ContentRequest, resolved_ref: &str) -> String {
    super::cache_key::fields_key(
        "github-content",
        &[&request.owner, &request.repo, &request.path, resolved_ref],
    )
}
