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
        self.file_bytes(request, context, BodyOwner::Transport)
            .await
    }

    /// [`Self::get_file_bytes`] for a caller that stores the bytes itself (a
    /// materialized checkout): the transport saves no answer holding them,
    /// so each body is stored once. A saved answer another read kept still
    /// answers it.
    pub async fn get_file_bytes_kept_by_caller(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<ContentResponse, ProviderError> {
        self.file_bytes(request, context, BodyOwner::Caller).await
    }

    async fn file_bytes(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
        owner: BodyOwner,
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
        let spec = owner.of(self.contents_spec(request, &resolved_ref)?);
        let page = match self.transport.execute(spec.clone(), context).await {
            Ok(page) => page,
            Err(error) if error.status == Some(413) => {
                let (bytes, raw_response_bytes, blob) = self
                    .fetch_via_directory_and_blob(request, &resolved_ref, context, owner)
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
        let (bytes, raw_response_bytes, blob) = self
            .page_bytes(request, &page, context, owner)
            .await
            .inspect_err(|error| self.discard_undecodable(&spec, error, context))?;
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
        owner: BodyOwner,
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
                let (bytes, blob_bytes) = self.fetch_blob(request, sha, context, owner).await?;
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
        owner: BodyOwner,
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
        let directory_spec = RequestSpec::get(directory_url);
        let directory = self
            .transport
            .execute(directory_spec.clone(), context)
            .await?;
        let entries: Vec<DirectoryEntry> =
            serde_json::from_slice(&directory.body).map_err(|_| {
                let error = ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub directory response",
                );
                self.discard_undecodable(&directory_spec, &error, context);
                error
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
        let (bytes, blob_bytes) = self.fetch_blob(request, &entry.sha, context, owner).await?;
        Ok((
            bytes,
            directory.body.len().saturating_add(blob_bytes),
            Some(entry.sha),
        ))
    }

    /// A body this read could not decode is no answer to keep: a retry asks
    /// GitHub again rather than meeting the same body. A file read's other
    /// failures (a directory, a submodule) are GitHub's answer and stay.
    fn discard_undecodable(&self, spec: &RequestSpec, error: &ProviderError, context: &RequestContext) {
        if error.kind == ProviderErrorKind::Decode {
            self.transport.discard(spec, context);
        }
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
        owner: BodyOwner,
    ) -> Result<(Bytes, usize), ProviderError> {
        let blob_url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "blobs",
            sha,
        ])?;
        // The URL names only the bytes: what holds them must know whose they are.
        let mut spec = owner.of(RequestSpec::get(blob_url).reading_file(&request.path));
        spec.headers
            .insert(ACCEPT, HeaderValue::from_static(RAW_MEDIA_TYPE));
        let blob = self.transport.execute(spec, context).await?;
        let size = blob.body.len();
        Ok((blob.body, size))
    }
}

/// Who stores a file read's body: the transport's saved answers, or the
/// caller alone. The directory listing a large file's fallback reads holds no
/// body and is kept as usual.
#[derive(Clone, Copy)]
enum BodyOwner {
    Transport,
    Caller,
}

impl BodyOwner {
    fn of(self, spec: RequestSpec) -> RequestSpec {
        match self {
            Self::Transport => spec,
            Self::Caller => spec.kept_by_caller(),
        }
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
