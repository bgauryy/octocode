//! One file's bytes at a commit and the blob fallback past the contents size cap.
use bytes::Bytes;
use reqwest::header::{ACCEPT, CONTENT_TYPE, ETAG, HeaderValue};

use super::links::{Checked, Unlisted};
use super::payload::{ContentPayload, DirectoryEntry, decode_content, parse_content_payload};
use super::{ContentRequest, ContentResponse, GitHubProvider, is_full_sha};
use crate::{ProviderError, ProviderErrorKind, RequestContext, RequestSpec, ResponsePage};

/// GitHub's raw media type: a file's own bytes instead of a JSON payload.
const RAW_MEDIA_TYPE: &str = "application/vnd.github.raw";

impl GitHubProvider {
    /// A file's own bytes at a commit: the one GitHub file read (auth,
    /// commit identity and the blob fallback past 1 MB).
    /// Whether the bytes are text is the caller's rule, not the provider's.
    /// A repository symlink on the path is refused, naming its target: the
    /// bytes under a link's name are the target's, which the caller's rules
    /// judge by the target's name ([`Self::get_file_bytes_following_links`]).
    pub async fn get_file_bytes(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<ContentResponse, ProviderError> {
        self.file_bytes(request, context, BodyOwner::Transport, Links::Refuse)
            .await
            .and_then(only_read)
    }

    /// [`Self::get_file_bytes`] for a caller that judges the name a read
    /// resolved to: a symlink is followed to its file (a few links deep),
    /// and the repository path it resolved to comes back beside the bytes.
    /// Before any of the target's bytes are asked for, `withholds` judges
    /// the target's path: a target it withholds comes back as `Err(target)`,
    /// with nothing of it requested or kept. The link's own path is never
    /// read as a file, so no answer holds the target's bytes under the
    /// link's name.
    pub async fn get_file_bytes_following_links(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
        withholds: &(dyn Fn(&str) -> bool + Sync),
    ) -> Result<Result<(ContentResponse, Option<String>), String>, ProviderError> {
        self.file_bytes(
            request,
            context,
            BodyOwner::Transport,
            Links::Follow(withholds),
        )
        .await
    }

    /// [`Self::get_file_bytes`] for a caller that stores the bytes itself (a
    /// materialized checkout): the transport saves no answer holding them,
    /// so each body is stored once. A saved answer another read kept still
    /// answers it. The path must be a regular file entry of a git tree of
    /// the same commit, which types a link by its mode, so no request goes
    /// to ask whether it is one.
    pub async fn get_file_bytes_kept_by_caller(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<ContentResponse, ProviderError> {
        self.file_bytes(request, context, BodyOwner::Caller, Links::Listed)
            .await
            .and_then(only_read)
    }

    async fn file_bytes(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
        owner: BodyOwner,
        links: Links<'_>,
    ) -> Result<Result<(ContentResponse, Option<String>), String>, ProviderError> {
        validate_name(&request.owner)?;
        validate_name(&request.repo)?;
        if request.path.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                "file path is required",
            ));
        }
        let resolved_ref = self.resolve_commit(request, context).await?;
        // A batch reading its rows' files ahead keeps what this read finds.
        if let Some(memo) = context.ref_memo() {
            memo.files_warmed().await;
        }
        let (at, target, unlisted) = match self
            .resolve_links(request, &resolved_ref, context, links)
            .await?
        {
            Checked::Withheld(target) => return Ok(Err(target)),
            Checked::Read {
                request,
                followed,
                unlisted,
            } => (request, followed, unlisted),
        };
        let request = &at;
        // When no tree answered at all, nothing says whether the path is a
        // link, and it may well exist: GitHub is first asked only whether it
        // does (its status alone, no byte of the file), and a path that
        // exists is refused there. Any other answer is asked again in full,
        // as its body states the reason (a commit or a path that is missing).
        if unlisted == Some(Unlisted::Unchecked) {
            let status = self
                .contents_spec(request, &resolved_ref)?
                .status_only()
                .kept_by_caller();
            match self.transport.execute(status, context).await {
                Ok(_) => return Err(unchecked_body(&request.path, Unlisted::Unchecked)),
                Err(error) if error.status.is_none() => return Err(error),
                Err(_) => {}
            }
        }
        // A path no tree lists may only be answered by GitHub's not-found: a
        // body is bytes no link check covered, so none is kept or served.
        let spec = match unlisted {
            Some(_) => self.contents_spec(request, &resolved_ref)?.kept_by_caller(),
            None => owner.of(self.contents_spec(request, &resolved_ref)?),
        };
        let page = match (
            self.transport.execute(spec.clone(), context).await,
            unlisted,
        ) {
            (Ok(_), Some(unlisted)) => return Err(unchecked_body(&request.path, unlisted)),
            (Ok(page), None) => page,
            (Err(error), None) if error.status == Some(413) => {
                let (bytes, raw_response_bytes, blob) = self
                    .fetch_via_directory_and_blob(request, &resolved_ref, context, owner)
                    .await?;
                return Ok(Ok((
                    ContentResponse {
                        bytes,
                        resolved_ref,
                        raw_response_bytes,
                        blob,
                    },
                    target,
                )));
            }
            (Err(error), _) => return Err(error),
        };
        let (bytes, raw_response_bytes, blob) = self
            .page_bytes(request, &page, context, owner)
            .await
            .inspect_err(|error| self.discard_undecodable(&spec, error, context))?;
        Ok(Ok((
            ContentResponse {
                bytes,
                resolved_ref,
                raw_response_bytes,
                blob,
            },
            target,
        )))
    }

    /// The contents request for `request` at `resolved_ref`, in the raw
    /// media type: a file (up to 100 MB) comes back as its own bytes, with
    /// no base64 body and no second blob read; a directory, symlink entry,
    /// or submodule still comes back as JSON, which types it.
    pub(super) fn contents_spec(
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
        if raw_response(page) {
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
            || payload.content.is_none()
            || (payload.content.as_deref().is_some_and(str::is_empty)
                && payload.size.unwrap_or(0) > 0);
        if needs_blob {
            let sha = payload.sha.as_deref().ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "GitHub file content is unavailable and no blob SHA was returned",
                )
            })?;
            let (bytes, blob_bytes) = self.fetch_blob(request, sha, context, owner).await?;
            return Ok((
                bytes,
                raw_response_bytes.saturating_add(blob_bytes),
                Some(sha.to_owned()),
            ));
        }
        let bytes = decode_content(payload.encoding.as_deref(), payload.content)?;
        Ok((bytes.into(), raw_response_bytes, payload.sha))
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
    fn discard_undecodable(
        &self,
        spec: &RequestSpec,
        error: &ProviderError,
        context: &RequestContext,
    ) {
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
        let blob = self.transport.execute(spec.clone(), context).await?;
        let size = blob.body.len();
        let bytes = blob_bytes(&blob)
            .inspect_err(|error| self.discard_undecodable(&spec, error, context))?;
        Ok((bytes, size))
    }
}

/// The bytes of a read that follows no link (a withheld target only comes
/// back from a following read).
fn only_read(
    read: Result<(ContentResponse, Option<String>), String>,
) -> Result<ContentResponse, ProviderError> {
    read.map(|(read, _)| read).map_err(|target| {
        ProviderError::new(
            ProviderErrorKind::Validation,
            format!("The read resolved to \"{target}\", which is withheld."),
        )
    })
}

/// The refusal of a body GitHub answered for a path no link check covered.
fn unchecked_body(path: &str, unlisted: Unlisted) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        match unlisted {
            Unlisted::Absent => format!(
                "GitHub's git tree of this commit lists no \"{path}\", yet its contents read answered; whether it is a symlink cannot be checked, so its bytes are not served."
            ),
            Unlisted::Unchecked => format!(
                "GitHub answered no git tree of any directory of \"{path}\" (404 or 422), so whether it is a symlink cannot be checked; its bytes are not served."
            ),
        },
    )
}

pub(super) fn blob_bytes(page: &ResponsePage) -> Result<Bytes, ProviderError> {
    let media_type = response_media_type(page);
    let json = media_type.eq_ignore_ascii_case("application/json")
        || media_type
            .rsplit_once('+')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("json"));
    if raw_response(page) || !json {
        return Ok(page.body.clone());
    }
    let payload: ContentPayload = serde_json::from_slice(&page.body).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub blob response")
    })?;
    decode_content(payload.encoding.as_deref(), payload.content).map(Bytes::from)
}

fn response_media_type(page: &ResponsePage) -> &str {
    page.headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or("")
        .trim()
}

fn raw_response(page: &ResponsePage) -> bool {
    response_media_type(page)
        .get(..RAW_MEDIA_TYPE.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(RAW_MEDIA_TYPE))
}

/// How a file read treats a repository symlink.
#[derive(Clone, Copy)]
pub(super) enum Links<'a> {
    Refuse,
    /// Follow links; the caller withholds the target paths this judges.
    Follow(&'a (dyn Fn(&str) -> bool + Sync)),
    /// The caller read this path as a regular file in a git tree of the
    /// same commit (a materialize walk: a link is listed as a link and never
    /// read), so its path needs no check.
    Listed,
}

///
/// Who stores a file read's body: the transport's saved answers, or the
/// caller alone. The directory listing a large file's fallback reads holds no
/// body and is kept as usual.
#[derive(Clone, Copy)]
enum BodyOwner {
    Transport,
    Caller,
}

#[cfg(test)]
mod tests;

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
