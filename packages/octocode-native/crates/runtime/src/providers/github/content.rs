use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderValue, IF_NONE_MATCH};
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
    pub bytes: Vec<u8>,
    pub resolved_ref: String,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CachePartition(pub(crate) String);
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
        let raw_response_bytes = page.body.len();
        let payload: ContentPayload = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
        })?;
        if payload.kind.as_deref() != Some("file") {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                "GitHub path is not a file",
            ));
        }
        let bytes = decode_bytes(payload.encoding.as_deref(), payload.content)?;
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
        if let Some(reference) = request.reference.as_deref()
            && reference.len() == 40
            && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(reference.to_ascii_lowercase());
        }
        let reference = match request.reference.as_deref() {
            Some(reference) => reference.to_owned(),
            None => {
                let url =
                    self.transport
                        .endpoint()
                        .rest(&["repos", &request.owner, &request.repo])?;
                let page = self
                    .transport
                    .execute(RequestSpec::get(url), context)
                    .await?;
                let repo: RepositoryPayload = serde_json::from_slice(&page.body).map_err(|_| {
                    ProviderError::new(
                        ProviderErrorKind::Decode,
                        "invalid GitHub repository response",
                    )
                })?;
                repo.default_branch
            }
        };
        let url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "commits",
            &reference,
        ])?;
        let page = self
            .transport
            .execute(RequestSpec::get(url), context)
            .await?;
        let commit: CommitPayload = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub commit response")
        })?;
        if commit.sha.len() != 40 || !commit.sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "GitHub returned an invalid commit SHA",
            ));
        }
        Ok(commit.sha.to_ascii_lowercase())
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
        let blob_url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "blobs",
            &entry.sha,
        ])?;
        let blob = self
            .transport
            .execute(RequestSpec::get(blob_url), context)
            .await?;
        let raw_response_bytes = directory.body.len().saturating_add(blob.body.len());
        let payload: BlobPayload = serde_json::from_slice(&blob.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub blob response")
        })?;
        Ok((
            decode_bytes(Some(&payload.encoding), Some(payload.content))?,
            raw_response_bytes,
        ))
    }
}
#[derive(Deserialize)]
struct ContentPayload {
    #[serde(rename = "type")]
    kind: Option<String>,
    encoding: Option<String>,
    content: Option<String>,
}
#[derive(Deserialize)]
struct RepositoryPayload {
    default_branch: String,
}
#[derive(Deserialize)]
struct CommitPayload {
    sha: String,
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
