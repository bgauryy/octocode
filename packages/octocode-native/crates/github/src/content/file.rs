//! One file's bytes at a commit and the blob fallback past the contents size cap.
use bytes::Bytes;
use reqwest::header::{ACCEPT, CONTENT_TYPE, ETAG, HeaderValue};

use super::payload::{DirectoryEntry, decode_content, parse_content_payload};
use super::{ContentRequest, ContentResponse, GitHubProvider, is_full_sha};
use crate::{ProviderError, ProviderErrorKind, RequestContext, RequestSpec, ResponsePage};

/// GitHub's raw media type: a file's own bytes instead of a JSON payload.
const RAW_MEDIA_TYPE: &str = "application/vnd.github.raw";

impl GitHubProvider {
    /// A file's own bytes at a commit: the one GitHub file read (auth,
    /// commit identity and the blob fallback past 1 MB).
    /// Whether the bytes are text is the caller's rule, not the provider's.
    pub async fn get_file_bytes(
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
        let spec = self.contents_spec(request, &resolved_ref)?;
        let page = match self.transport.execute(spec, context).await {
            Ok(page) => page,
            Err(error) if error.status == Some(413) => {
                let (bytes, raw_response_bytes, blob) = self
                    .fetch_via_directory_and_blob(request, &resolved_ref, context)
                    .await?;
                return Ok(ContentResponse {
                    bytes,
                    resolved_ref,
                    raw_response_bytes,
                    blob,
                });
            }
            Err(error) => return Err(error),
        };
        let (bytes, raw_response_bytes, blob) = self.page_bytes(request, &page, context).await?;
        Ok(ContentResponse {
            bytes,
            resolved_ref,
            raw_response_bytes,
            blob,
        })
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
    ) -> Result<(Bytes, usize, Option<String>), ProviderError> {
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
                .filter(|sha| is_full_sha(sha))
                .map(str::to_owned);
            return Ok((page.body.clone(), raw_response_bytes, blob));
        }
        let payload = parse_content_payload(body, &request.path)?;
        let needs_blob = payload.encoding.as_deref() == Some("none")
            || (payload.content.as_deref().is_none_or(str::is_empty)
                && payload.size.unwrap_or(0) > 0);
        match payload.sha.as_deref().filter(|_| needs_blob) {
            Some(sha) => {
                let (bytes, blob_bytes) = self.fetch_blob(request, sha, context).await?;
                Ok((
                    bytes,
                    raw_response_bytes.saturating_add(blob_bytes),
                    Some(sha.to_owned()),
                ))
            }
            None => {
                let bytes = decode_content(payload.encoding.as_deref(), payload.content)?;
                Ok((bytes.into(), raw_response_bytes, payload.sha))
            }
        }
    }

    async fn fetch_via_directory_and_blob(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> Result<(Bytes, usize, Option<String>), ProviderError> {
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
        Ok((
            bytes,
            directory.body.len().saturating_add(blob_bytes),
            Some(entry.sha),
        ))
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
        let size = blob.body.len();
        Ok((blob.body, size))
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
