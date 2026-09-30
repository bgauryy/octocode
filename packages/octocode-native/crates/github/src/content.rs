use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{ACCEPT, HeaderValue, IF_NONE_MATCH};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{future::Future, pin::Pin};

use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CachedContent {
    pub etag: Option<String>,
    /// Base64 on disk (a JSON number array is ~3.5× the body); legacy array
    /// entries still decode.
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    pub resolved_ref: String,
}
mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Encoded {
        Base64(String),
        Legacy(Vec<u8>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        match Encoded::deserialize(deserializer)? {
            Encoded::Base64(text) => STANDARD.decode(text).map_err(D::Error::custom),
            Encoded::Legacy(bytes) => Ok(bytes),
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CachePartition(pub(crate) String);
impl CachePartition {
    /// Opaque credential-scoped identity for host cache storage.
    pub fn identity(&self) -> &str {
        &self.0
    }
}
pub trait ConditionalCache: Send + Sync {
    fn get<'a>(
        &'a self,
        partition: &'a CachePartition,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>>;
    fn put<'a>(
        &'a self,
        partition: &'a CachePartition,
        key: String,
        value: CachedContent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
#[derive(Default)]
pub struct NoCache;
impl ConditionalCache for NoCache {
    fn get<'a>(
        &'a self,
        _: &'a CachePartition,
        _: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>> {
        Box::pin(async { None })
    }
    fn put<'a>(
        &'a self,
        _: &'a CachePartition,
        _: String,
        _: CachedContent,
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
    pub session_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentResponse {
    pub bytes: Vec<u8>,
    pub resolved_ref: String,
    pub etag: Option<String>,
    pub from_cache: bool,
    pub raw_response_bytes: usize,
}
pub struct GitHubProvider<R, C> {
    pub transport: GitHubTransport<R>,
    pub cache: C,
}
impl<R: CredentialResolver, C: ConditionalCache> GitHubProvider<R, C> {
    pub async fn get_file_content(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
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
        let partition = self
            .transport
            .cache_partition(context, request.session_id.as_deref())
            .await?;
        let key = cache_key(request, &resolved_ref);
        let cached = if request.force_refresh {
            None
        } else {
            self.cache.get(&partition, &key).await
        };
        // A cached large-file entry has no ETag (it came from the blob fallback
        // below). The cache key includes the resolved commit SHA, so that content
        // is immutable — serve it directly instead of re-issuing a request that
        // will 413 again and re-download the whole blob on every read.
        if let Some(value) = cached.as_ref().filter(|value| value.etag.is_none()) {
            return Ok(ContentResponse {
                bytes: value.bytes.clone(),
                resolved_ref: value.resolved_ref.clone(),
                etag: None,
                from_cache: true,
                raw_response_bytes: 0,
            });
        }
        let url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "contents",
            &request.path,
        ])?;
        let mut url = url;
        url.query_pairs_mut().append_pair("ref", &resolved_ref);
        let mut spec = RequestSpec::get(url);
        if let Some(etag) = cached.as_ref().and_then(|v| v.etag.as_ref()) {
            spec.headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Validation, "invalid cached ETag")
                })?,
            );
        }
        let page = match self.transport.execute(spec, context).await {
            Ok(page) => page,
            Err(error) if error.status == Some(413) => {
                let (bytes, raw_response_bytes) = self
                    .fetch_via_directory_and_blob(request, &resolved_ref, context)
                    .await?;
                let stored = CachedContent {
                    etag: None,
                    bytes: bytes.clone(),
                    resolved_ref: resolved_ref.clone(),
                };
                self.cache.put(&partition, key, stored).await;
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
        if page.status == 304 {
            let value = cached.ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "GitHub returned 304 without cached content",
                )
            })?;
            return Ok(ContentResponse {
                bytes: value.bytes,
                resolved_ref: value.resolved_ref,
                etag: value.etag,
                from_cache: true,
                raw_response_bytes: 0,
            });
        }
        let mut raw_response_bytes = page.body.len();
        let payload = parse_content_payload(&page.body, &request.path)?;
        // Files between 1 MB and 100 MB come back 200 with `encoding:"none"`
        // and an empty body; their bytes are only available as a git blob.
        let needs_blob = payload.encoding.as_deref() == Some("none")
            || (payload.content.as_deref().is_none_or(str::is_empty)
                && payload.size.unwrap_or(0) > 0);
        let bytes = match payload.sha.as_deref().filter(|_| needs_blob) {
            Some(sha) => {
                let (bytes, blob_bytes) = self.fetch_blob(request, sha, context).await?;
                raw_response_bytes = raw_response_bytes.saturating_add(blob_bytes);
                bytes
            }
            None => decode_bytes(payload.encoding.as_deref(), payload.content)?,
        };
        let etag = page
            .headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let stored = CachedContent {
            etag: etag.clone(),
            bytes: bytes.clone(),
            resolved_ref: resolved_ref.clone(),
        };
        self.cache.put(&partition, key, stored).await;
        Ok(ContentResponse {
            bytes,
            resolved_ref,
            etag,
            from_cache: false,
            raw_response_bytes,
        })
    }

    pub async fn repository_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<super::ContentsListing, ProviderError> {
        let partition = self.transport.cache_partition(context, None).await?;
        let key = {
            let mut digest = Sha256::new();
            for value in [owner, repo, path, reference] {
                digest.update(value.as_bytes());
                digest.update([0]);
            }
            format!("github-tree:{}", hex::encode(digest.finalize()))
        };
        let cached = self.cache.get(&partition, &key).await;
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
            page.body.to_vec()
        };
        let listing = parse_contents_listing(&body)?;
        if page.status != 304 {
            let etag = page
                .headers
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            self.cache
                .put(
                    &partition,
                    key,
                    CachedContent {
                        etag,
                        bytes: body,
                        resolved_ref: reference.to_owned(),
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
    /// [`REF_MEMO_TTL_SECS`] in the credential partition so a batch of reads on
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
        let partition = self.transport.cache_partition(context, None).await?;
        let key = ref_memo_key(owner, repo, reference);
        // Single flight: a concurrent batch on one ref waits for the first
        // resolution and then reads it from the memo.
        let flight = ref_flight(&partition, &key);
        let _guard = flight.lock().await;
        if !force_refresh
            && let Some(sha) = self
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
        self.cache
            .put(
                &partition,
                key,
                CachedContent {
                    etag: None,
                    bytes: sha.as_bytes().to_vec(),
                    resolved_ref: format!("{REF_MEMO_PREFIX}{}", unix_now()),
                },
            )
            .await;
        Ok(sha)
    }

    async fn fetch_via_directory_and_blob(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> Result<(Vec<u8>, usize), ProviderError> {
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
        let (bytes, blob_bytes) = self.fetch_blob(request, &entry.sha, context).await?;
        Ok((bytes, directory.body.len().saturating_add(blob_bytes)))
    }

    async fn fetch_blob(
        &self,
        request: &ContentRequest,
        sha: &str,
        context: &RequestContext,
    ) -> Result<(Vec<u8>, usize), ProviderError> {
        let blob_url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "blobs",
            sha,
        ])?;
        let blob = self
            .transport
            .execute(RequestSpec::get(blob_url), context)
            .await?;
        let payload: BlobPayload = serde_json::from_slice(&blob.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub blob response")
        })?;
        Ok((
            decode_bytes(Some(&payload.encoding), Some(payload.content))?,
            blob.body.len(),
        ))
    }
}

/// Parse a Contents API response for a single file. Directories (arrays),
/// symlinks and submodules get their own actionable validation errors instead
/// of an opaque decode failure.
impl<R: CredentialResolver> GitHubTransport<R> {
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
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.sha"),
        );
        let page = self
            .execute(spec, context)
            .await
            .map_err(classify_ref_failure)?;
        parse_commit_sha(&page.body)
    }
}

/// Type a ref-resolution failure: a 404 is the repository (missing, private,
/// or hidden from the token), a 422 "No commit found" the ref itself.
fn classify_ref_failure(error: ProviderError) -> ProviderError {
    match error.status {
        Some(404) => {
            let mut error = error.with_reason(super::ProviderErrorReason::RepositoryNotFound);
            // GitHub links its commit docs; the failure is the repository.
            error.documentation_url =
                Some("https://docs.github.com/rest/repos/repos#get-a-repository".into());
            error
        }
        Some(422) if error.message.starts_with("No commit found") => {
            error.with_reason(super::ProviderErrorReason::RefNotFound)
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
        })),
        _ => Err(not_a_file(format!("GitHub path \"{path}\" is not a file"))),
    }
}
/// A directory read, typed so `runtime::github::file_error` keys its
/// tree-listing recovery on the reason rather than the message.
fn is_a_directory(path: &str) -> ProviderError {
    not_a_file(format!(
        "Path \"{path}\" is a directory, not a file; list it with ghStructure."
    ))
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
const REF_MEMO_TTL_SECS: u64 = 60;
/// Marker stored in `CachedContent.resolved_ref` for ref memo entries, followed
/// by the unix time the SHA was resolved.
const REF_MEMO_PREFIX: &str = "ref-memo@";
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
type RefFlights = std::sync::Mutex<
    std::collections::HashMap<(CachePartition, String), std::sync::Arc<tokio::sync::Mutex<()>>>,
>;
fn ref_flight(partition: &CachePartition, key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    static FLIGHTS: std::sync::OnceLock<RefFlights> = std::sync::OnceLock::new();
    let mut flights = FLIGHTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Drop idle entries so a long session does not grow the map unbounded.
    if flights.len() > 256 {
        flights.retain(|_, flight| std::sync::Arc::strong_count(flight) > 1);
    }
    flights
        .entry((partition.clone(), key.to_owned()))
        .or_default()
        .clone()
}
fn ref_memo_key(owner: &str, repo: &str, reference: &str) -> String {
    let mut digest = Sha256::new();
    // Owner/repo names are case-insensitive on GitHub; refs are not.
    for value in [
        owner.to_ascii_lowercase().as_str(),
        repo.to_ascii_lowercase().as_str(),
        reference,
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("github-ref:{}", hex::encode(digest.finalize()))
}
fn fresh_ref_memo(value: &CachedContent) -> Option<String> {
    let stored_at: u64 = value
        .resolved_ref
        .strip_prefix(REF_MEMO_PREFIX)?
        .parse()
        .ok()?;
    let sha = std::str::from_utf8(&value.bytes).ok()?;
    (unix_now().saturating_sub(stored_at) <= REF_MEMO_TTL_SECS && is_full_sha(sha))
        .then(|| sha.to_ascii_lowercase())
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
#[derive(Deserialize)]
struct BlobPayload {
    encoding: String,
    content: String,
}
fn decode_bytes(encoding: Option<&str>, content: Option<String>) -> Result<Vec<u8>, ProviderError> {
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
    if bytes.contains(&0) {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            "binary files are not supported",
        ));
    }
    Ok(bytes)
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
fn parse_contents_listing(body: &[u8]) -> Result<super::ContentsListing, ProviderError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::Decode,
            "invalid GitHub repository contents response",
        )
    })?;
    let raw_entry_count = value.as_array().map_or(1, Vec::len);
    let raw_entries = match value {
        serde_json::Value::Array(entries) => entries,
        entry @ serde_json::Value::Object(_) => vec![entry],
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub repository contents response",
            ));
        }
    };
    let entries = raw_entries
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub repository contents entry",
            )
        })?;
    Ok(super::ContentsListing {
        entries,
        raw_entry_count,
    })
}

fn cache_key(request: &ContentRequest, resolved_ref: &str) -> String {
    let mut h = Sha256::new();
    for value in [&request.owner, &request.repo, &request.path, resolved_ref] {
        h.update(value.as_bytes());
        h.update([0]);
    }
    format!("github-content:{}", hex::encode(h.finalize()))
}
