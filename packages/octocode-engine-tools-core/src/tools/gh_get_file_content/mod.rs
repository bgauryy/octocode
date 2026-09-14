use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::github::{
    ConditionalCache, ContentRequest, CredentialResolver, GitHubProvider, ProviderError,
    RequestContext,
};
use crate::tools::local_fetch::{
    CancellationCheck, ChunkType, ContentScan, LocalFetchRegex, LocalFetchRequest, MinifyMode,
    RegexMatch, process_fetched_content,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GhGetFileContentQuery {
    pub owner: String,
    pub repo: String,
    pub path: String,
    pub branch: Option<String>,
    pub full_content: Option<bool>,
    pub match_string: Option<String>,
    pub match_string_is_regex: Option<bool>,
    pub match_string_case_sensitive: Option<bool>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub context_lines: Option<usize>,
    pub context_bytes: Option<usize>,
    pub chunk_type: Option<ChunkType>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub minify: Option<MinifyMode>,
    pub force_refresh: Option<bool>,
    pub goal: Option<String>,
    pub reasoning: Option<String>,
}

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
    pub resolved_branch: String,
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
    let acquired = match provider
        .get_file_content(
            &ContentRequest {
                owner: query.owner.clone(),
                repo: query.repo.clone(),
                path: query.path.clone(),
                reference: query.branch.clone(),
                force_refresh: query.force_refresh.unwrap_or(false),
                session_id: session_id.map(str::to_owned),
            },
            request_context,
        )
        .await
    {
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
    let local = LocalFetchRequest {
        path: query.path.clone(),
        full_content: query.full_content,
        match_string: query.match_string.clone(),
        match_string_is_regex: query.match_string_is_regex,
        match_string_case_sensitive: query.match_string_case_sensitive,
        start_line: query.start_line,
        end_line: query.end_line,
        context_lines: query.context_lines,
        context_bytes: query.context_bytes,
        chunk_type: query.chunk_type,
        offset: query.offset,
        limit: query.limit.or_else(|| {
            (query.full_content != Some(true)
                && query.match_string.is_none()
                && !(query.start_line.is_some() && query.end_line.is_some()))
            .then_some(match query.chunk_type.unwrap_or_default() {
                ChunkType::Lines => 100,
                ChunkType::Bytes => 16384,
            })
        }),
        minify: query.minify,
    };
    let mut content = process_fetched_content(
        &local,
        &acquired.bytes,
        Path::new(&query.path),
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
        content.path = query.path.clone();
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
            chunk_type: query.chunk_type.unwrap_or_default(),
            offset: 0,
            length: 0,
            limit: query
                .limit
                .unwrap_or(match query.chunk_type.unwrap_or_default() {
                    ChunkType::Lines => 1,
                    ChunkType::Bytes => 16384,
                }),
            total_lines: 0,
            total_bytes: 0,
            has_more: false,
            next_offset: None,
        });
    }
    if content.returned_bytes == Some(0) {
        content.source_line_ranges.clear();
    }
    // Remote reads treat an exhausted match selection as a successful search,
    // and an empty file or bounded complete view as an empty read.
    let match_not_found = content.selected_match_count == Some(0);
    if match_not_found {
        content.error_code = None;
        content.hints.clear();
        content.pagination = Some(crate::tools::local_fetch::Pagination {
            chunk_type: query.chunk_type.unwrap_or_default(),
            offset: 0,
            length: 0,
            limit: query
                .limit
                .unwrap_or(match query.chunk_type.unwrap_or_default() {
                    ChunkType::Lines => 1,
                    ChunkType::Bytes => 16384,
                }),
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
    if content.error_code.as_deref() == Some("fullContentLimit") {
        content.error = None;
        content.content = Some(String::new());
        content.content_view = Some(query.minify.unwrap_or_default());
    }
    if content.error.is_none() {
        content.status =
            if match_not_found || content.content.as_ref().is_some_and(|s| !s.is_empty()) {
                "success"
            } else {
                "empty"
            }
            .into();
    }
    let next = rewrite_continuations(&mut content, query, &acquired.resolved_ref);
    let (last_modified, last_modified_by) = if query.offset.unwrap_or(0) == 0 {
        file_timestamp(provider, query, &acquired.resolved_ref, request_context).await
    } else {
        (None, None)
    };
    Ok(GhGetFileContentResult {
        owner: query.owner.clone(),
        repo: query.repo.clone(),
        files: vec![GhGetFileContentFile {
            content,
            resolved_branch: acquired.resolved_ref,
            file_type: match crate::content::classify_file_type(&query.path) {
                Some(crate::content::FileType::Config) => Some("config"),
                Some(crate::content::FileType::Lock) => Some("lock"),
                Some(crate::content::FileType::Doc) => Some("doc"),
                Some(crate::content::FileType::Code) => Some("code"),
                None => None,
            },
            match_not_found: match_not_found.then_some(true),
            searched_for: match_not_found
                .then(|| query.match_string.clone())
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
    let Ok(page) = provider
        .transport
        .list_commits(
            &crate::providers::github::CommitListRequest {
                owner: query.owner.clone(),
                repo: query.repo.clone(),
                branch: Some(reference.to_owned()),
                path: Some(query.path.clone()),
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
    let Some(commit) = page.items.first() else {
        return (None, None);
    };
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
        if entry.name.to_ascii_lowercase() == target && entry.name != name {
            suggestions.push(entry.path);
        } else if entry.name.starts_with(&format!("{stem}.")) && entry.name != name {
            suggestions.push(entry.path);
        }
    }
    Ok(suggestions
        .into_iter()
        .take(3)
        .map(|path| format!("Try path \"{path}\"; GitHub paths are case-sensitive."))
        .collect())
}

pub async fn execute_default_regex<R, C>(
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
        &LocalFetchRegex::default(),
    )
    .await
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
            query.insert("owner".into(), Value::String(source.owner.clone()));
            query.insert("repo".into(), Value::String(source.repo.clone()));
            query.insert("branch".into(), Value::String(resolved_ref.to_owned()));
            if result.error_code.as_deref() != Some("fullContentLimit") {
                query.insert("fullContent".into(), Value::Bool(false));
            }
            query.insert(
                "minify".into(),
                Value::String(
                    match source.minify.unwrap_or_default() {
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

pub fn continuation_query(
    source: &GhGetFileContentQuery,
    local_query: &LocalFetchRequest,
    resolved_ref: &str,
) -> Value {
    let mut value = serde_json::to_value(local_query).unwrap_or(Value::Null);
    if let Value::Object(ref mut object) = value {
        object.insert("owner".into(), Value::String(source.owner.clone()));
        object.insert("repo".into(), Value::String(source.repo.clone()));
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::{path::Path, sync::Arc, time::Duration};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

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
        let query = GhGetFileContentQuery {
            owner: "a".into(),
            repo: "b".into(),
            path: "src/lib.rs".into(),
            branch: Some("main".into()),
            full_content: None,
            match_string: None,
            match_string_is_regex: None,
            match_string_case_sensitive: None,
            start_line: None,
            end_line: None,
            context_lines: None,
            context_bytes: None,
            chunk_type: Some(ChunkType::Lines),
            offset: None,
            limit: Some(2),
            minify: None,
            force_refresh: None,
            goal: None,
            reasoning: None,
        };
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
        assert_eq!(result.files[0].resolved_branch, sha);
        let next = result.files[0].next.clone().expect("continuation");
        assert_eq!(next["continue"]["tool"], "ghGetFileContent");
        assert_eq!(next["continue"]["query"]["owner"], "a");
        assert_eq!(next["continue"]["query"]["branch"], sha);
    }
}
