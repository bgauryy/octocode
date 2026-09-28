use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::providers::github::{
    ConditionalCache, ContentRequest, CredentialResolver, GitHubProvider, ProviderError,
    RequestContext,
};
use crate::tools::local_fetch::{
    CancellationCheck, ChunkType, ContentScan, LocalFetchQuery, MinifyMode, RegexMatch,
    process_fetched_content,
};

pub use crate::contracts::tool_types::GhGetFileContentQuery;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhGetFileContentResult {
    pub owner: String,
    pub repo: String,
    pub files: Vec<GhGetFileContentFile>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhGetFileContentFile {
    #[serde(flatten)]
    pub content: crate::tools::local_fetch::LocalFetchResult,
    /// Commit the read was pinned to (a branch or default-branch ref resolves
    /// to its current SHA; continuations reuse it).
    pub commit_sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_not_found: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub searched_for: Option<String>,
    #[serde(skip)]
    pub etag: Option<String>,
    #[serde(skip)]
    pub raw_response_bytes: usize,
    #[serde(skip)]
    pub from_cache: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
}

pub async fn execute<R, C>(
    provider: &GitHubProvider<R, C>,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    session_id: Option<&str>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> Result<GhGetFileContentResult, ProviderError>
where
    R: CredentialResolver,
    C: ConditionalCache,
{
    // Resolve the ref once (memoized across a batch), then read the body at
    // the immutable SHA. The timestamp runs only after a successful read so a
    // missing path or a rate limit costs no extra request.
    let fetched = match provider
        .resolve_reference(
            &query.owner,
            &query.repo,
            query.branch.as_deref(),
            query.force_refresh.unwrap_or(false),
            request_context,
        )
        .await
    {
        Ok(sha) => {
            let content_request = ContentRequest {
                owner: query.owner.to_string(),
                repo: query.repo.to_string(),
                path: query.path.to_string(),
                reference: Some(sha.clone()),
                force_refresh: query.force_refresh.unwrap_or(false),
                session_id: session_id.map(str::to_owned),
            };
            match provider
                .get_file_content(&content_request, request_context)
                .await
            {
                Ok(acquired) => {
                    let timestamp = if query.offset.unwrap_or(0) == 0 {
                        file_timestamp(provider, query, &sha, request_context).await
                    } else {
                        (None, None)
                    };
                    Ok((acquired, timestamp))
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };
    let (acquired, (last_modified, last_modified_by)) = match fetched {
        Ok(value) => value,
        Err(mut error)
            if error.kind == crate::providers::github::ProviderErrorKind::NotFound
                || error.status == Some(404) =>
        {
            if let Ok(hints) = path_suggestions(provider, query, request_context).await
                && !hints.is_empty()
            {
                error.message = format!("{}. {}", error.message, hints.join(" ")).into();
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let local = local_fetch_query(query)?;
    let mut content = process_fetched_content(
        &local,
        &acquired.bytes,
        Path::new(query.path.as_str()),
        None,
        security,
        cancel,
        regex,
    );
    if query.minify == Some(MinifyMode::Symbols) {
        content
            .warnings
            .retain(|warning| !warning.starts_with("No smaller outline is available for "));
    }
    if content.error_code.as_deref() == Some("invalidPagination") {
        return Err(ProviderError::new(
            crate::providers::github::ProviderErrorKind::Decode,
            "offset must be on a UTF-8 code point boundary",
        ));
    }
    if content.error_code.as_deref() == Some("noMatches") && content.error.is_some() {
        let raw = String::from_utf8_lossy(&acquired.bytes);
        content.path = query.path.to_string();
        content.error = None;
        content.content = Some(String::new());
        content.content_view = Some(MinifyMode::None);
        content.total_lines = Some(raw.lines().count());
        content.source_chars = Some(raw.encode_utf16().count());
        content.source_bytes = Some(raw.len());
        content.returned_chars = Some(0);
        content.returned_bytes = Some(0);
        content.returned_lines = Some(0);
        content.pagination = Some(crate::tools::local_fetch::Pagination {
            chunk_type: local.chunk_type.unwrap_or(ChunkType::Lines),
            offset: 0,
            length: 0,
            chunk_size: local.chunk_size().unwrap_or(
                match local.chunk_type.unwrap_or(ChunkType::Lines) {
                    ChunkType::Lines => 1,
                    ChunkType::Bytes => 16384,
                },
            ),
            total_lines: 0,
            total_bytes: 0,
            has_more: false,
            next_offset: None,
        });
    }
    if content.returned_bytes == Some(0) {
        content.source_line_ranges.clear();
    }
    // An exhausted match selection, an empty file, or a bounded complete view
    // with no content is an empty read.
    let match_not_found = content.selected_match_count == Some(0);
    if match_not_found {
        content.error_code = None;
        content.hints = vec![crate::tools::local_fetch::no_match_hint(
            query.match_string_is_regex.unwrap_or(false),
            query.match_string_case_sensitive.unwrap_or(false),
            "ghSearchCode",
        )];
        content.pagination = Some(crate::tools::local_fetch::Pagination {
            chunk_type: local.chunk_type.unwrap_or(ChunkType::Lines),
            offset: 0,
            length: 0,
            chunk_size: local.chunk_size().unwrap_or(
                match local.chunk_type.unwrap_or(ChunkType::Lines) {
                    ChunkType::Lines => 1,
                    ChunkType::Bytes => 16384,
                },
            ),
            total_lines: 0,
            total_bytes: 0,
            has_more: false,
            next_offset: None,
        });
        if let Some(requested) = query.minify.filter(|mode| *mode != MinifyMode::None) {
            content.minify_fallback = Some(crate::tools::local_fetch::MinifyFallback {
                requested,
                applied: MinifyMode::None,
                reason: "match-evidence".into(),
            });
        }
    }
    if content.error.is_none() {
        content.status = if match_not_found {
            // Same as localFetch: no selected line is an empty read, which
            // keeps the recovery hint visible under the hint policy.
            "empty"
        } else if content.content.as_ref().is_some_and(|s| !s.is_empty()) {
            "success"
        } else {
            "empty"
        }
        .into();
    }
    let next = rewrite_continuations(&mut content, query, &acquired.resolved_ref);
    Ok(GhGetFileContentResult {
        owner: query.owner.to_string(),
        repo: query.repo.to_string(),
        files: vec![GhGetFileContentFile {
            content,
            commit_sha: acquired.resolved_ref,
            file_type: match crate::content::classify_file_type(&query.path) {
                Some(crate::content::FileType::Config) => Some("config"),
                Some(crate::content::FileType::Lock) => Some("lock"),
                Some(crate::content::FileType::Doc) => Some("doc"),
                Some(crate::content::FileType::Code) => Some("code"),
                None => None,
            },
            match_not_found: match_not_found.then_some(true),
            searched_for: match_not_found
                .then(|| query.match_string.as_deref().cloned())
                .flatten(),
            etag: acquired.etag,
            raw_response_bytes: acquired.raw_response_bytes,
            from_cache: acquired.from_cache,
            next,
            last_modified,
            last_modified_by,
        }],
    })
}

/// Last commit touching `path` at `reference` (a resolved SHA). History below
/// a commit is immutable, so the answer is cached per (owner, repo, SHA, path)
/// and repeated offset-0 reads skip the `commits?path=` round trip.
async fn file_timestamp<R, C>(
    provider: &GitHubProvider<R, C>,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> (Option<String>, Option<String>)
where
    R: CredentialResolver,
    C: ConditionalCache,
{
    let partition = provider.transport.cache_partition(context, None).await.ok();
    let key = {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        for value in [
            query.owner.to_ascii_lowercase().as_str(),
            query.repo.to_ascii_lowercase().as_str(),
            reference,
            query.path.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        format!("github-file-timestamp:{}", hex::encode(digest.finalize()))
    };
    if let Some(partition) = &partition
        && let Some(cached) = provider.cache.get(partition, &key).await
        && let Ok((date, author)) =
            serde_json::from_slice::<(Option<String>, Option<String>)>(&cached.bytes)
    {
        return (date, author);
    }
    let Ok(page) = provider
        .transport
        .list_commits(
            &crate::providers::github::CommitListRequest {
                owner: query.owner.to_string(),
                repo: query.repo.to_string(),
                branch: Some(reference.to_owned()),
                path: Some(query.path.to_string()),
                author: None,
                since: None,
                until: None,
                page: 1,
                per_page: 1,
            },
            context,
        )
        .await
    else {
        return (None, None);
    };
    let stamp = page.items.first().map_or((None, None), |commit| {
        (
            commit
                .pointer("/commit/committer/date")
                .or_else(|| commit.pointer("/commit/author/date"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            commit
                .pointer("/commit/author/name")
                .or_else(|| commit.pointer("/author/login"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    });
    if let Some(partition) = &partition
        && let Ok(bytes) = serde_json::to_vec(&stamp)
    {
        provider
            .cache
            .put(
                partition,
                key,
                crate::providers::github::CachedContent {
                    etag: None,
                    bytes,
                    resolved_ref: reference.to_owned(),
                },
            )
            .await;
    }
    stamp
}

async fn path_suggestions<R, C>(
    provider: &GitHubProvider<R, C>,
    query: &GhGetFileContentQuery,
    context: &RequestContext,
) -> Result<Vec<String>, ProviderError>
where
    R: CredentialResolver,
    C: ConditionalCache,
{
    let Some((parent, name)) = query.path.rsplit_once('/') else {
        return Ok(Vec::new());
    };
    let listing = provider
        .transport
        .repository_contents(
            &query.owner,
            &query.repo,
            parent,
            query.branch.as_deref().unwrap_or("HEAD"),
            context,
        )
        .await?;
    let target = name.to_ascii_lowercase();
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    let mut suggestions = Vec::new();
    for entry in listing.entries {
        if entry.name != name
            && (entry.name.to_ascii_lowercase() == target
                || entry.name.starts_with(&format!("{stem}.")))
        {
            suggestions.push(entry.path);
        }
    }
    Ok(suggestions
        .into_iter()
        .take(3)
        .map(|path| format!("Try path \"{path}\"; GitHub paths are case-sensitive."))
        .collect())
}

fn rewrite_continuations(
    result: &mut crate::tools::local_fetch::LocalFetchResult,
    source: &GhGetFileContentQuery,
    resolved_ref: &str,
) -> Option<Value> {
    let mut value = serde_json::to_value(result.next.take()?).ok()?;
    let Value::Object(object) = &mut value else {
        return None;
    };
    for continuation in object.values_mut() {
        let Value::Object(fields) = continuation else {
            continue;
        };
        fields.insert("tool".into(), Value::String("ghGetFileContent".into()));
        if let Some(Value::Object(query)) = fields.get_mut("query") {
            query.insert("owner".into(), Value::String(source.owner.to_string()));
            query.insert("repo".into(), Value::String(source.repo.to_string()));
            query.insert("branch".into(), Value::String(resolved_ref.to_owned()));
            query.insert("fullContent".into(), Value::Bool(false));
            query.insert(
                "minify".into(),
                Value::String(
                    match source.minify.unwrap_or(MinifyMode::None) {
                        MinifyMode::None => "none",
                        MinifyMode::Standard => "standard",
                        MinifyMode::Symbols => "symbols",
                    }
                    .to_owned(),
                ),
            );
            if let Some(force) = source.force_refresh {
                query.insert("forceRefresh".into(), Value::Bool(force));
            }
        }
    }
    Some(value)
}

/// Remembers the secret-scanner output for recently read views so paging one
/// large file does not rescan the whole blob on every `next.continue`.
///
/// A line/byte page is cut from the sanitized full view, so each page
/// otherwise re-runs `sanitize` over the entire file (~55 ms release for a
/// 176 KB file, dominating a cache-hit read). Entries are keyed by a SHA-256
/// of the scanned text and path, so a hit needs the exact bytes in hand and
/// returns exactly what the scanner produced for them; redaction is unchanged.
/// One memo must only ever wrap one scanner (the owning runtime's policy).
pub struct SanitizedViewMemo {
    entries: std::sync::Mutex<std::collections::VecDeque<MemoEntry>>,
}

type ScanOutcome = Result<(String, Vec<String>), (String, String)>;

struct MemoEntry {
    key: [u8; 32],
    bytes: usize,
    outcome: std::sync::Arc<ScanOutcome>,
}

impl SanitizedViewMemo {
    const MAX_ENTRIES: usize = 8;
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    /// Small views are cheap to rescan; memoizing them only churns entries.
    const MIN_TEXT_BYTES: usize = 16 * 1024;

    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    fn key(text: &str, path: &Path) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        let path = path.to_string_lossy();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update(text.as_bytes());
        digest.finalize().into()
    }

    fn scan(&self, text: &str, path: &Path, inner: &impl ContentScan) -> ScanOutcome {
        if text.len() < Self::MIN_TEXT_BYTES {
            return inner.sanitize(text, path);
        }
        let key = Self::key(text, path);
        {
            let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(entry) = entries
                .iter()
                .position(|entry| entry.key == key)
                .and_then(|index| entries.remove(index))
            {
                let outcome = std::sync::Arc::clone(&entry.outcome);
                entries.push_back(entry);
                return (*outcome).clone();
            }
        }
        let outcome = inner.sanitize(text, path);
        let bytes = text.len().saturating_add(match &outcome {
            Ok((safe, warnings)) => safe.len() + warnings.iter().map(String::len).sum::<usize>(),
            Err((code, message)) => code.len() + message.len(),
        });
        if bytes <= Self::MAX_BYTES {
            let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            if !entries.iter().any(|entry| entry.key == key) {
                entries.push_back(MemoEntry {
                    key,
                    bytes,
                    outcome: std::sync::Arc::new(outcome.clone()),
                });
            }
            let mut total: usize = entries.iter().map(|entry| entry.bytes).sum();
            while entries.len() > Self::MAX_ENTRIES || total > Self::MAX_BYTES {
                let Some(evicted) = entries.pop_front() else {
                    break;
                };
                total = total.saturating_sub(evicted.bytes);
            }
        }
        outcome
    }
}

impl Default for SanitizedViewMemo {
    fn default() -> Self {
        Self::new()
    }
}

/// `ContentScan` adapter that routes `sanitize` through a [`SanitizedViewMemo`]
/// and forwards everything else (full-file key-block redaction still runs on
/// every read) to the wrapped scanner.
pub struct MemoizedScan<'a, S> {
    inner: &'a S,
    memo: &'a SanitizedViewMemo,
}

impl<'a, S: ContentScan> MemoizedScan<'a, S> {
    pub fn new(inner: &'a S, memo: &'a SanitizedViewMemo) -> Self {
        Self { inner, memo }
    }
}

impl<S: ContentScan> ContentScan for MemoizedScan<'_, S> {
    fn sanitize(&self, text: &str, path: &Path) -> ScanOutcome {
        self.memo.scan(text, path, self.inner)
    }
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        self.inner.redact_key_blocks(content)
    }
}

/// The GitHub file query is the localFetch extraction query plus repository
/// coordinates; project it onto the generated localFetch wire type so both
/// tools share one extraction request. Paged reads get the default page size.
fn local_fetch_query(query: &GhGetFileContentQuery) -> Result<LocalFetchQuery, ProviderError> {
    let mut value = serde_json::to_value(query).map_err(decode_error)?;
    if let Value::Object(object) = &mut value {
        for key in ["owner", "repo", "branch", "forceRefresh"] {
            object.remove(key);
        }
    }
    let mut local: LocalFetchQuery = serde_json::from_value(value).map_err(decode_error)?;
    let paged = local.full_content != Some(true)
        && local.match_string.is_none()
        && !(local.start_line.is_some() && local.end_line.is_some());
    if local.chunk_size.is_none() && paged {
        local.chunk_size = crate::tools::local_fetch::wire_positive(default_chunk_size(&local));
    }
    Ok(local)
}

fn default_chunk_size(local: &LocalFetchQuery) -> usize {
    match local.chunk_type.unwrap_or(ChunkType::Lines) {
        ChunkType::Lines => crate::tools::local_fetch::DEFAULT_LINE_CHUNK,
        ChunkType::Bytes => 16384,
    }
}

fn decode_error(error: serde_json::Error) -> ProviderError {
    ProviderError::new(
        crate::providers::github::ProviderErrorKind::Decode,
        format!("ghGetFileContent query does not map onto localFetch: {error}"),
    )
}

pub fn continuation_query(
    source: &GhGetFileContentQuery,
    local_query: &LocalFetchQuery,
    resolved_ref: &str,
) -> Value {
    let mut value = serde_json::to_value(local_query).unwrap_or(Value::Null);
    if let Value::Object(ref mut object) = value {
        object.insert("owner".into(), Value::String(source.owner.to_string()));
        object.insert("repo".into(), Value::String(source.repo.to_string()));
        object.insert("branch".into(), Value::String(resolved_ref.to_owned()));
        if let Some(force) = source.force_refresh {
            object.insert("forceRefresh".into(), Value::Bool(force));
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::github::{
        CredentialSource, GitHubEndpoint, GitHubTransport, NoCache, RetryPolicy,
        StaticCredentialResolver,
    };
    use crate::tools::local_fetch::NeverCancel;
    use crate::tools::local_fetch::wire_positive;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::{path::Path, sync::Arc, time::Duration};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    async fn execute_default_regex<R, C>(
        provider: &GitHubProvider<R, C>,
        query: &GhGetFileContentQuery,
        request_context: &RequestContext,
        session_id: Option<&str>,
        security: &impl ContentScan,
        cancel: &impl CancellationCheck,
    ) -> Result<GhGetFileContentResult, ProviderError>
    where
        R: CredentialResolver,
        C: ConditionalCache,
    {
        execute(
            provider,
            query,
            request_context,
            session_id,
            security,
            cancel,
            &crate::tools::local_fetch::LocalFetchRegex::default(),
        )
        .await
    }

    struct Safe;
    impl ContentScan for Safe {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.replace("TOKEN", "[REDACTED]"), vec![]))
        }
    }

    #[derive(Default)]
    struct Counting {
        scans: std::sync::atomic::AtomicUsize,
    }
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            path: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Safe.sanitize(text, path)
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            (content.replace("KEYBODY", "[KEY]"), true)
        }
    }

    #[test]
    fn line_pages_scan_only_their_window_and_keep_redaction() {
        let body = format!("TOKEN KEYBODY\n{}", "line of text\n".repeat(4000));
        let scanner = Counting::default();
        let memo = SanitizedViewMemo::new();
        let security = MemoizedScan::new(&scanner, &memo);
        let mut pages = Vec::new();
        for offset in [0, 100, 200] {
            let request = LocalFetchQuery {
                path: "big.txt".parse().expect("path"),
                chunk_type: Some(ChunkType::Lines),
                offset: Some(offset),
                chunk_size: wire_positive(100),
                ..LocalFetchQuery::test_default()
            };
            let page = process_fetched_content(
                &request,
                body.as_bytes(),
                Path::new("big.txt"),
                None,
                &security,
                &NeverCancel,
                &crate::tools::local_fetch::LocalFetchRegex::default(),
            );
            pages.push(page.content.unwrap_or_default());
        }
        // Line pages sanitize only their window (plus a margin), one small
        // scan per page, instead of the whole file.
        assert_eq!(
            scanner.scans.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "each line page scans its own window"
        );
        assert!(pages[0].starts_with("[REDACTED] [KEY]\n"), "{}", pages[0]);
        assert_eq!(pages[1], "line of text\n".repeat(100));
        // The memo still serves repeated full views (byte/fullContent pages):
        // a new view is scanned once, then reused.
        memo.scan("x".repeat(20_000).as_str(), Path::new("other"), &scanner)
            .expect("scan");
        memo.scan("x".repeat(20_000).as_str(), Path::new("other"), &scanner)
            .expect("scan");
        assert_eq!(scanner.scans.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn shared_pipeline_ranges_redacts_and_emits_remote_continuation() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})))
            .mount(&server)
            .await;
        Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Flib.rs")).and(query_param("ref",sha)).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nneedle TOKEN\nthree\n")}))).mount(&server).await;
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
                .expect("endpoint");
        let transport = GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::new(
                "fixture",
                CredentialSource::Override,
            )),
            RetryPolicy::default(),
        )
        .expect("transport");
        let provider = GitHubProvider {
            transport,
            cache: NoCache,
        };
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "src/lib.rs", "branch": "main",
            "chunkType": "lines", "chunkSize": 2, "reasoning": "test"
        }))
        .expect("ghGetFileContent query");
        let result = execute_default_regex(
            &provider,
            &query,
            &RequestContext::with_timeout(Duration::from_secs(2), 4096),
            Some("s"),
            &Safe,
            &NeverCancel,
        )
        .await
        .expect("result");
        assert_eq!(
            result.files[0].content.content.as_deref(),
            Some("one\nneedle [REDACTED]\n")
        );
        assert_eq!(result.files[0].commit_sha, sha);
        let wire = serde_json::to_value(&result.files[0]).expect("file json");
        assert_eq!(wire["commitSha"], sha);
        assert!(wire.get("resolvedBranch").is_none(), "{wire}");
        let next = result.files[0].next.clone().expect("continuation");
        assert_eq!(next["continue"]["tool"], "ghGetFileContent");
        assert_eq!(next["continue"]["query"]["owner"], "a");
        assert_eq!(next["continue"]["query"]["branch"], sha);
    }

    #[tokio::test]
    async fn match_string_runs_on_redacted_text() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\n")})))
            .mount(&server)
            .await;
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
                .expect("endpoint");
        let transport = GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::new(
                "fixture",
                CredentialSource::Override,
            )),
            RetryPolicy::default(),
        )
        .expect("transport");
        let provider = GitHubProvider {
            transport,
            cache: NoCache,
        };
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "src/lib.rs", "branch": "main",
            "matchString": "TOKEN", "contextLines": 0, "reasoning": "test"
        }))
        .expect("query");
        let result = execute_default_regex(
            &provider,
            &query,
            &RequestContext::with_timeout(Duration::from_secs(2), 4096),
            Some("s"),
            &Safe,
            &NeverCancel,
        )
        .await
        .expect("result");
        assert_eq!(
            result.files[0].match_not_found,
            Some(true),
            "{:?}",
            result.files[0].content
        );
    }

    #[tokio::test]
    async fn oversized_full_content_is_partial_not_empty() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/big.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("line of text\n".repeat(6000))})))
            .mount(&server)
            .await;
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
                .expect("endpoint");
        let transport = GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::new(
                "fixture",
                CredentialSource::Override,
            )),
            RetryPolicy::default(),
        )
        .expect("transport");
        let provider = GitHubProvider {
            transport,
            cache: NoCache,
        };
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "big.txt", "branch": sha,
            "fullContent": true, "reasoning": "test"
        }))
        .expect("query");
        let result = execute_default_regex(
            &provider,
            &query,
            &RequestContext::with_timeout(Duration::from_secs(2), 1 << 20),
            Some("s"),
            &Safe,
            &NeverCancel,
        )
        .await
        .expect("result");
        let file = &result.files[0];
        // The first bounded page comes back inline, not an empty body.
        assert_eq!(file.content.error_code, None);
        assert_eq!(file.content.status, "success");
        assert!(
            file.content
                .content
                .as_deref()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(file.content.is_partial, Some(true));
        assert!(
            file.next
                .as_ref()
                .is_some_and(|next| next.get("continue").is_some())
        );
    }
}
