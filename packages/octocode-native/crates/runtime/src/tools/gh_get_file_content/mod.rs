pub(crate) mod errors;

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use crate::providers::github::{
    ConditionalCache, ContentRequest, GitHubProvider, ProviderError, RequestContext,
};
use crate::security::scan::ContentScan;
use crate::tools::cancel::CancellationCheck;
use crate::tools::gh_shared::{GhFailure, locate_path, missing_path};
use crate::tools::local_fetch::{
    LocalFetchQuery, MinifyMode, RegexMatch, WindowUnit, process_fetched_content,
};
use crate::tools::result::ToolData;

pub use crate::contracts::tool_types::GhGetFileContentQuery;

/// One read's result. The wire row is flat: the read file's fields beside
/// `owner`/`repo`, with no `files` wrapper (a query reads one path).
#[derive(Clone, Debug)]
pub struct GhGetFileContentResult {
    pub owner: String,
    pub repo: String,
    pub files: Vec<GhGetFileContentFile>,
}

impl Serialize for GhGetFileContentResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Flat<'a> {
            owner: &'a str,
            repo: &'a str,
            #[serde(flatten)]
            file: Option<&'a GhGetFileContentFile>,
        }
        Flat {
            owner: &self.owner,
            repo: &self.repo,
            file: self.files.first(),
        }
        .serialize(serializer)
    }
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
    #[serde(skip)]
    pub from_cache: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
}

/// One ghGetFileContent row as the runtime emits it.
pub struct FileRead {
    pub output: ToolData,
    /// Every byte came from the content cache.
    pub cache: bool,
}

/// Run one ghGetFileContent row: the read, or the failure with its one
/// recovery (a missing path walks to the case-corrected file or the nearest
/// existing directory).
pub async fn run<C>(
    provider: &GitHubProvider<C>,
    query: &GhGetFileContentQuery,
    request: Result<&RequestContext, ProviderError>,
    window: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> Result<FileRead, GhFailure>
where
    C: ConditionalCache,
{
    let request_context = request.map_err(|error| errors::failure(error, query, None))?;
    let error = match execute(
        provider,
        query,
        request_context,
        window,
        security,
        cancel,
        regex,
    )
    .await
    {
        Ok(result) => return Ok(file_read(result, query)),
        Err(error) => error,
    };
    // Recovery leads name a renamed repository as it is now.
    let canonical = crate::tools::gh_shared::canonical_repo(
        provider,
        query.owner.as_str(),
        query.repo.as_str(),
        request_context,
    )
    .await
    .and_then(|(owner, repo)| {
        Some(GhGetFileContentQuery {
            owner: owner.parse().ok()?,
            repo: repo.parse().ok()?,
            ..query.clone()
        })
    });
    let query = canonical.as_ref().unwrap_or(query);
    let found = if missing_path(&error, Some(query.path.as_str())) {
        locate_path(provider, &errors::repo_path(query), request_context).await
    } else {
        None
    };
    Err(errors::failure(error, query, found))
}

/// The row of a read: its status from the file, and the resolved commit
/// unless the caller already named that full SHA.
fn file_read(result: GhGetFileContentResult, query: &GhGetFileContentQuery) -> FileRead {
    let status = match result
        .files
        .first()
        .map(|file| file.content.status.as_str())
    {
        Some("error") => Some("error"),
        Some("empty") => Some("empty"),
        _ => None,
    };
    let cache = !result.files.is_empty() && result.files.iter().all(|file| file.from_cache);
    let mut data = serde_json::to_value(&result).unwrap_or_default();
    if data["commitSha"].as_str() == query.ref_.as_deref()
        && let Some(map) = data.as_object_mut()
    {
        map.remove("commitSha");
    }
    // An empty file (not a missed match, which says why) names what to check.
    if status == Some("empty") && data.get("hints").is_none() && data.get("isPartial").is_none() {
        data["hints"] = json!(["The file is empty at this ref; verify ref and path."]);
    }
    FileRead {
        output: ToolData {
            status,
            ..ToolData::from(data)
        },
        cache,
    }
}

pub async fn execute<C>(
    provider: &GitHubProvider<C>,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> Result<GhGetFileContentResult, ProviderError>
where
    C: ConditionalCache,
{
    // Resolve the ref once (memoized across a batch), then read the body at
    // the immutable SHA.
    let sha = provider
        .resolve_reference(
            &query.owner,
            &query.repo,
            query.ref_.as_deref(),
            query.force_refresh.unwrap_or(false),
            request_context,
        )
        .await?;
    let content_request = ContentRequest {
        owner: query.owner.to_string(),
        repo: query.repo.to_string(),
        path: query.path.to_string(),
        reference: Some(sha),
        force_refresh: query.force_refresh.unwrap_or(false),
        session_id: None,
    };
    let acquired = provider
        .get_file_content(&content_request, request_context)
        .await?;
    // A renamed repository: later requests and every lead use the canonical
    // name (a redirect-free read of a never-redirected name costs nothing).
    let renamed = crate::tools::gh_shared::canonical_repo(
        provider,
        query.owner.as_str(),
        query.repo.as_str(),
        request_context,
    )
    .await;
    let renamed_warning = renamed.as_ref().map(|to| {
        crate::tools::gh_shared::renamed_warning(query.owner.as_str(), query.repo.as_str(), to)
    });
    let canonical;
    let query = match &renamed {
        Some((owner, repo)) => {
            canonical = GhGetFileContentQuery {
                owner: owner.parse().map_err(decode_display)?,
                repo: repo.parse().map_err(decode_display)?,
                ..query.clone()
            };
            &canonical
        }
        None => query,
    };
    // The first page states when the file last changed: one commits request
    // after the body arrived, so a failed or throttled read never spends it,
    // and later pages repeat nothing.
    let (last_modified, last_modified_by) = if query.offset.unwrap_or(0) == 0 {
        file_timestamp(provider, query, &acquired.resolved_ref, request_context).await
    } else {
        (None, None)
    };
    let local = local_fetch_query(query)?;
    // The configured response window bounds a whole-file view.
    let facts = crate::tools::local_fetch::SourceFacts {
        modified: None,
        window,
    };
    let content = process_fetched_content(
        &local,
        &acquired.bytes,
        Path::new(query.path.as_str()),
        &facts,
        security,
        cancel,
        regex,
    );
    let mut content = complete_small_file(
        content,
        &local,
        &acquired.bytes,
        Path::new(query.path.as_str()),
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
    let match_not_found = content.selected_match_count == Some(0)
        || (content.error_code.as_deref() == Some("noMatches") && content.error.is_some());
    if match_not_found {
        no_match(&mut content, &local, query, &acquired.bytes);
    }
    if content.returned_bytes == Some(0) {
        content.source_line_ranges.clear();
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
    // Shared local read errors omit their path; the remote file envelope
    // requires the repository-relative identity even when processing fails.
    if content.path.is_empty() {
        content.path = query.path.to_string();
    }
    if let Some(warning) = renamed_warning {
        content.warnings.insert(0, warning);
    }
    let mut next = rewrite_continuations(&mut content, query, &acquired.resolved_ref);
    if match_not_found
        && let Some((name, lead)) = empty_match_lead(
            query,
            &local,
            content.total_lines.unwrap_or(0),
            &acquired.resolved_ref,
        )
        && let Some(calls) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        calls.insert(name.into(), lead);
    }
    if match_not_found
        && let Some(lead) = ignore_case_lead(query, &local, &acquired.resolved_ref)
        && let Some(calls) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        calls.insert("ignoreCase".into(), lead);
    }
    Ok(GhGetFileContentResult {
        owner: query.owner.to_string(),
        repo: query.repo.to_string(),
        files: vec![GhGetFileContentFile {
            content,
            commit_sha: acquired.resolved_ref,
            file_type: file_type(&query.path),
            match_not_found: match_not_found.then_some(true),
            from_cache: acquired.from_cache,
            next,
            last_modified,
            last_modified_by,
        }],
    })
}

/// A selection with no hit in the file is an empty read that says why: the
/// hint and the file's size, with the contract's empty `content` (as in
/// localFetch) and no zero counters.
fn no_match(
    content: &mut crate::tools::local_fetch::LocalFetchResult,
    local: &LocalFetchQuery,
    query: &GhGetFileContentQuery,
    bytes: &[u8],
) {
    let raw = String::from_utf8_lossy(bytes);
    content.path = query.path.to_string();
    content.error = None;
    content.error_code = None;
    content.content = Some(String::new());
    content.content_view = None;
    content.total_lines = Some(raw.lines().count());
    content.source_chars = None;
    content.source_bytes = None;
    content.returned_chars = None;
    content.returned_bytes = None;
    content.returned_lines = None;
    content.pagination = None;
    content.source_line_ranges.clear();
    content.hints = vec![crate::tools::local_fetch::no_match_hint(
        local.is_regex(),
        local
            .match_strings()
            .iter()
            .any(|pattern| local.case_sensitive_for(pattern)),
        crate::tools::id::ToolId::GhSearchCode.as_str(),
    )];
    if let Some(requested) = query.minify.filter(|mode| *mode != MinifyMode::None) {
        content.minify_fallback = Some(crate::tools::local_fetch::MinifyFallback {
            requested,
            applied: MinifyMode::None,
            reason: "match-evidence".into(),
        });
    }
}

/// Lines from which a missed literal is located semantically (clasify)
/// rather than searched for again: the size where the clasify gate starts.
const LOCATE_FILE_LINES: usize = 1000;

/// The runnable next step of a match that found nothing: in a large file,
/// a clasify locate of the same file at the same commit (the literal was a
/// guess); otherwise a ghSearchCode of the repository for the literal (it
/// may live in another file). A regex has no literal to search.
fn empty_match_lead(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    total_lines: usize,
    sha: &str,
) -> Option<(&'static str, Value)> {
    let literals = local.match_strings();
    let first = literals.first()?.trim();
    if total_lines >= LOCATE_FILE_LINES {
        let ask = format!(
            "Which lines of {} handle {}?",
            query.path.as_str(),
            literals.join(" or ")
        );
        let goal: String = query
            .main_goal
            .as_ref()
            .map_or(ask.as_str(), |goal| goal.as_str())
            .chars()
            .take(crate::tools::id::query_limits::clasify::MAIN_GOAL_MAX_LENGTH)
            .collect();
        let ask: String = ask
            .chars()
            .take(crate::tools::id::query_limits::clasify::MAIN_GOAL_MAX_LENGTH)
            .collect();
        let resource = json!({
            "id": "file",
            "tool": crate::tools::id::ToolId::GhGetFileContent.as_str(),
            "query": {"owner": query.owner.as_str(), "repo": query.repo.as_str(), "path": query.path.as_str(), "ref": sha},
        });
        return Some((
            "clasify",
            crate::tools::result::Continuation::new(
                crate::tools::id::ToolId::Clasify,
                json!({
                    "mainGoal": goal,
                    "resources": [resource],
                    "questions": [{"id": "target", "type": "locate", "ask": ask}],
                }),
            )
            .why("Locate what the missed literal names in this large file.")
            .confidence("medium")
            .build(),
        ));
    }
    if local.is_regex() || first.is_empty() {
        return None;
    }
    Some((
        "searchCode",
        crate::tools::result::Continuation::new(
            crate::tools::id::ToolId::GhSearchCode,
            json!({"owner": query.owner.as_str(), "repo": query.repo.as_str(), "keywords": [first]}),
        )
        .why("Find the file that holds the literal.")
        .confidence("medium")
        .build(),
    ))
}

/// The same read at the same commit, matched case-insensitively: the
/// runnable recovery of a case-sensitive matchString that selected no line.
fn ignore_case_lead(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    sha: &str,
) -> Option<Value> {
    let sensitive = local
        .match_strings()
        .iter()
        .any(|pattern| local.case_sensitive_for(pattern));
    if !sensitive {
        return None;
    }
    let mut row = query.clone();
    row.case_mode = Some(crate::contracts::tool_types::ReadCaseMode::Insensitive);
    row.ref_ = Some(sha.to_owned());
    let mut row = serde_json::to_value(row).ok()?;
    crate::tools::result::remove_null_fields(&mut row);
    Some(
        crate::tools::result::Continuation::new(crate::tools::id::ToolId::GhGetFileContent, row)
            .why("Match the text case-insensitively.")
            .confidence("high")
            .build(),
    )
}

/// The kind of file a read returned (`fileType`).
fn file_type(path: &str) -> Option<&'static str> {
    match crate::content::classify_file_type(path)? {
        crate::content::FileType::Config => Some("config"),
        crate::content::FileType::Lock => Some("lock"),
        crate::content::FileType::Doc => Some("doc"),
        crate::content::FileType::Code => Some("code"),
    }
}

/// Small files a window mostly covers come back whole.
const SMALL_FILE_LINES: usize = 200;
const SMALL_FILE_BYTES: usize = 8 * 1024;

/// A line or match window covering at least half of a small file returns the
/// whole file: the rest costs little and saves the follow-up read that a
/// window stopping short of the answer forces. Match anchors stay in
/// `matchedLines`.
fn complete_small_file(
    content: crate::tools::local_fetch::LocalFetchResult,
    local: &LocalFetchQuery,
    bytes: &[u8],
    path: &Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> crate::tools::local_fetch::LocalFetchResult {
    let (Some(total), Some(source_bytes)) = (content.total_lines, content.source_bytes) else {
        return content;
    };
    let returned: usize = content
        .source_line_ranges
        .iter()
        .map(|range| range.end + 1 - range.start)
        .sum();
    // The whole file holds the rest of every declaration a window cut, so a
    // `readBlock` lead never blocks completion; any other continuation does.
    let only_read_block = content.next.as_ref().is_none_or(|next| {
        next.read_block.is_some()
            && crate::tools::local_fetch::NextCalls {
                read_block: None,
                ..next.clone()
            }
            .is_empty()
    });
    let windowed = local.full_content != Some(true)
        && local.minify_mode() == MinifyMode::None
        && local.context_bytes.is_none()
        && (local.match_string.is_some() || local.has_ranges());
    if !windowed
        || content.error.is_some()
        || !only_read_block
        || total > SMALL_FILE_LINES
        || source_bytes > SMALL_FILE_BYTES
        || returned == 0
        || returned >= total
        || returned * 2 < total
    {
        return content;
    }
    let mut whole = local.clone();
    whole.match_string = None;
    whole.regex = None;
    whole.case_mode = None;
    whole.context_lines = None;
    whole.unit = None;
    whole.length = None;
    whole.offset = None;
    whole.clear_block_selectors();
    whole.set_line_span(1, total);
    let mut completed = process_fetched_content(
        &whole,
        bytes,
        path,
        &Default::default(),
        security,
        cancel,
        regex,
    );
    if completed.error.is_some() || completed.source_line_ranges.len() != 1 {
        return content;
    }
    completed.matched_lines = content.matched_lines;
    completed.selected_match_count = content.selected_match_count;
    completed.warnings = content.warnings;
    completed
}

/// Last commit touching `path` at `reference` (a resolved SHA). History below
/// a commit is immutable, so the answer is cached per (owner, repo, SHA, path)
/// and repeated offset-0 reads skip the `commits?path=` round trip.
async fn file_timestamp<C>(
    provider: &GitHubProvider<C>,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> (Option<String>, Option<String>)
where
    C: ConditionalCache,
{
    let partition = provider.transport.cache_partition(context, None).ok();
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
        // v2: the author is the login when GitHub links one.
        format!(
            "github-file-timestamp:v2-{}",
            hex::encode(digest.finalize())
        )
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
                .pointer("/author/login")
                .or_else(|| commit.pointer("/commit/author/name"))
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
                    bytes: bytes.into(),
                    resolved_ref: reference.to_owned(),
                },
            )
            .await;
    }
    stamp
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
    // Each localFetch call becomes the same read of the GitHub file. A lead
    // to another local tool (the miss's `textSearch` localSearch of the
    // file's directory) has no GitHub form here and is dropped; the GitHub
    // miss adds its own repository-level lead.
    object.retain(|_, continuation| {
        continuation["tool"] == crate::tools::id::ToolId::LocalFetch.as_str()
    });
    for continuation in object.values_mut() {
        let Some(Value::Object(mut query)) =
            crate::tools::result::continuation_row(continuation).cloned()
        else {
            continue;
        };
        query.insert("owner".into(), Value::String(source.owner.to_string()));
        query.insert("repo".into(), Value::String(source.repo.to_string()));
        query.insert("ref".into(), Value::String(resolved_ref.to_owned()));
        // A continuation reads a window, never the whole file, and keeps
        // the caller's view; the default view (`minify: none`) is omitted.
        query.remove("fullContent");
        match source.minify.unwrap_or(MinifyMode::None) {
            MinifyMode::None => {
                query.remove("minify");
            }
            mode => {
                let name = if mode == MinifyMode::Standard {
                    "standard"
                } else {
                    "symbols"
                };
                query.insert("minify".into(), Value::String(name.to_owned()));
            }
        }
        if let Some(force) = source.force_refresh {
            query.insert("forceRefresh".into(), Value::Bool(force));
        }
        let mut call = crate::tools::result::Continuation::new(
            crate::tools::id::ToolId::GhGetFileContent,
            Value::Object(query),
        );
        if let Some(why) = continuation["why"].as_str() {
            call = call.why(why);
        }
        if let Some(confidence) = continuation["confidence"].as_str() {
            call = call.confidence(confidence);
        }
        *continuation = call.build();
    }
    (!object.is_empty()).then_some(value)
}

/// The GitHub file query is the localFetch extraction query plus repository
/// coordinates; project it onto the generated localFetch wire type so both
/// tools share one extraction request. Paged reads get the default page size.
fn local_fetch_query(query: &GhGetFileContentQuery) -> Result<LocalFetchQuery, ProviderError> {
    let mut value = serde_json::to_value(query).map_err(decode_error)?;
    if let Value::Object(object) = &mut value {
        for key in ["owner", "repo", "ref", "forceRefresh"] {
            object.remove(key);
        }
    }
    let mut local: LocalFetchQuery = serde_json::from_value(value).map_err(decode_error)?;
    let paged =
        local.full_content != Some(true) && local.match_string.is_none() && !local.has_ranges();
    if local.length.is_none() && paged {
        local.length = crate::tools::local_fetch::wire_positive(default_chunk_size(&local));
    }
    Ok(local)
}

fn default_chunk_size(local: &LocalFetchQuery) -> usize {
    match local.unit.unwrap_or(WindowUnit::Lines) {
        WindowUnit::Lines => crate::tools::local_fetch::DEFAULT_LINE_CHUNK,
        WindowUnit::Bytes => 16384,
    }
}

fn decode_display(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(
        crate::providers::github::ProviderErrorKind::Decode,
        error.to_string(),
    )
}

fn decode_error(error: serde_json::Error) -> ProviderError {
    ProviderError::new(
        crate::providers::github::ProviderErrorKind::Decode,
        format!("ghGetFileContent query does not map onto localFetch: {error}"),
    )
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Verify owner/repo/ref/path, or remove matchString."
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    fn text_shape(&self) -> crate::tools::output::TextShape {
        crate::tools::output::TextShape::FileText
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::github::{NoCache, RetryPolicy};
    use crate::security::scan::{MemoizedScan, SanitizedViewMemo};
    use crate::tools::cancel::NeverCancel;
    use crate::tools::gh_shared::test_support::{fixture_context, mock_provider, mount_json};
    use crate::tools::local_fetch::wire_positive;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::{path::Path, time::Duration};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    async fn execute_default_regex<C>(
        provider: &GitHubProvider<C>,
        query: &GhGetFileContentQuery,
        request_context: &RequestContext,
        window: Option<usize>,
        security: &impl ContentScan,
        cancel: &impl CancellationCheck,
    ) -> Result<GhGetFileContentResult, ProviderError>
    where
        C: ConditionalCache,
    {
        execute(
            provider,
            query,
            request_context,
            window,
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
                unit: Some(WindowUnit::Lines),
                offset: Some(offset),
                length: wire_positive(100),
                ..LocalFetchQuery::test_default()
            };
            let page = process_fetched_content(
                &request,
                body.as_bytes(),
                Path::new("big.txt"),
                &Default::default(),
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
        security
            .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
            .expect("scan");
        security
            .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
            .expect("scan");
        assert_eq!(scanner.scans.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn shared_pipeline_ranges_redacts_and_emits_remote_continuation() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        mount_json(
            &server,
            "/api/v3/repos/a/b/commits/main",
            200,
            serde_json::json!({"sha":sha}),
        )
        .await;
        Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Flib.rs")).and(query_param("ref",sha)).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nneedle TOKEN\nthree\n")}))).mount(&server).await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "src/lib.rs", "ref": "main",
            "unit": "lines", "length": 2, "mainGoal": "test", "reasoning": "test"
        }))
        .expect("ghGetFileContent query");
        let result = execute_default_regex(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(2), 4096),
            None,
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
        assert!(wire.get("resolvedRef").is_none(), "{wire}");
        let next = result.files[0].next.clone().expect("continuation");
        assert_eq!(next["continue"]["tool"], "ghGetFileContent");
        assert_eq!(next["continue"]["query"]["queries"][0]["owner"], "a");
        assert_eq!(next["continue"]["query"]["queries"][0]["ref"], sha);
    }

    #[tokio::test]
    async fn match_string_runs_on_redacted_text() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        mount_json(
            &server,
            "/api/v3/repos/a/b/commits/main",
            200,
            serde_json::json!({"sha":sha}),
        )
        .await;
        mount_json(&server, "/api/v3/repos/a/b/contents/src%2Flib.rs", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\n")})).await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "src/lib.rs", "ref": "main",
            "matchString": "TOKEN", "contextLines": 0, "mainGoal": "test", "reasoning": "test"
        }))
        .expect("query");
        let result = execute_default_regex(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(2), 4096),
            None,
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

    async fn read(provider: &GitHubProvider<NoCache>, query: Value) -> GhGetFileContentResult {
        let mut query = query;
        query["mainGoal"] = "test".into();
        query["reasoning"] = "test".into();
        let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
        execute_default_regex(
            provider,
            &query,
            &fixture_context(Duration::from_secs(5), 1 << 20),
            Some(50_000),
            &Safe,
            &NeverCancel,
        )
        .await
        .expect("result")
    }

    /// Freshness: the first page of a read (offset 0) states when the file
    /// last changed and by whom, without debug, for one commits request
    /// sent after the body arrived; a later page and a failed read send none.
    #[tokio::test]
    async fn the_first_page_states_the_last_change_for_one_extra_request() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let body: String = (1..=40).map(|i| format!("line {i}\n")).collect();
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/a.txt",
            200,
            serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/missing.txt",
            404,
            serde_json::json!({"message":"Not Found"}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits"))
            .and(wiremock::matchers::query_param("path", "a.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!([{"author":{"login":"ada-l"},"commit":{"committer":{"date":"2026-01-02T00:00:00Z"},"author":{"name":"Ada"}}}]),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let base = serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha});
        let stamped = read(&provider, base.clone()).await;
        assert_eq!(
            stamped.files[0].last_modified.as_deref(),
            Some("2026-01-02T00:00:00Z")
        );
        assert_eq!(stamped.files[0].last_modified_by.as_deref(), Some("ada-l"));
        let mut later = base;
        later["offset"] = 20.into();
        let later = read(&provider, later).await;
        assert_eq!(later.files[0].last_modified, None);
        assert_eq!(later.files[0].last_modified_by, None);
        let mut missing =
            serde_json::json!({"owner":"a","repo":"b","path":"missing.txt","ref":sha});
        missing["mainGoal"] = "test".into();
        missing["reasoning"] = "test".into();
        let missing: GhGetFileContentQuery = serde_json::from_value(missing).expect("query");
        assert!(
            execute_default_regex(
                &provider,
                &missing,
                &fixture_context(Duration::from_secs(5), 1 << 20),
                None,
                &Safe,
                &NeverCancel,
            )
            .await
            .is_err()
        );
        let requests = server.received_requests().await.unwrap_or_default();
        assert!(
            !requests.iter().any(|request| {
                request.url.path() == "/api/v3/repos/a/b/commits"
                    && request
                        .url
                        .query()
                        .is_some_and(|q| q.contains("missing.txt"))
            }),
            "a failed read asked for its timestamp: {requests:?}"
        );
    }

    /// A window covering at least half of a small file returns the whole
    /// file; its match anchors stay. Large files and narrow windows keep
    /// the window.
    #[tokio::test]
    async fn windows_covering_most_of_a_small_file_return_it_whole() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let small: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        let large: String = (1..=300).map(|i| format!("line {i}\n")).collect();
        for (name, body) in [("small.py", &small), ("large.py", &large)] {
            mount_json(&server, format!("/api/v3/repos/a/b/contents/{name}"), 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(body)}),).await;
        }
        let provider = mock_provider(&server, RetryPolicy::default());
        let base = |path: &str| serde_json::json!({"owner":"a","repo":"b","path":path,"ref":sha});
        let mut matched = base("small.py");
        matched["matchString"] = "line 12".into();
        matched["contextLines"] = 10.into();
        let whole = read(&provider, matched).await;
        let file = &whole.files[0].content;
        assert_eq!(file.content.as_deref(), Some(small.as_str()));
        assert_eq!(
            file.source_line_ranges,
            vec![crate::tools::local_fetch::LineRange { start: 1, end: 30 }]
        );
        assert_eq!(file.matched_lines, vec![12]);

        let mut ranged = base("small.py");
        ranged["ranges"] = serde_json::json!(["1-15"]);
        let whole = read(&provider, ranged).await;
        assert_eq!(
            whole.files[0].content.content.as_deref(),
            Some(small.as_str())
        );

        let mut narrow = base("small.py");
        narrow["ranges"] = serde_json::json!(["1-5"]);
        let window = read(&provider, narrow).await;
        assert_eq!(
            window.files[0].content.content.as_deref(),
            Some("line 1\nline 2\nline 3\nline 4\nline 5\n")
        );

        let mut big = base("large.py");
        big["ranges"] = serde_json::json!(["1-200"]);
        let window = read(&provider, big).await;
        assert_eq!(
            window.files[0].content.source_line_ranges,
            vec![crate::tools::local_fetch::LineRange { start: 1, end: 200 }]
        );
    }

    /// Continuations and leads carry only fields that change the read: a
    /// default view (`fullContent` off, `minify: none`) is omitted, a chosen
    /// minify mode is kept, and a whole-file read pages without `fullContent`.
    #[tokio::test]
    async fn continuations_omit_default_view_fields() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let body: String = (1..=6000).map(|i| format!("line of text {i}\n")).collect();
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/big.txt",
            200,
            serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
        )
        .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let base = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
        let mut paged = base.clone();
        paged["unit"] = "lines".into();
        paged["length"] = 100.into();
        let mut standard = paged.clone();
        standard["minify"] = "standard".into();
        let mut whole = base;
        whole["fullContent"] = true.into();
        for (query, minify) in [(paged, None), (standard, Some("standard")), (whole, None)] {
            let result = read(&provider, query).await;
            let next = result.files[0].next.clone().expect("continuation");
            let page = &next["continue"]["query"]["queries"][0];
            assert!(page.get("fullContent").is_none(), "{next}");
            assert_eq!(page.get("minify").and_then(Value::as_str), minify, "{next}");
            assert_eq!(page["ref"], sha, "{next}");
        }
    }

    /// A small file a match window mostly covers comes back whole even when
    /// the window cut a declaration: the whole file holds the declaration's
    /// rest, so no `readBlock` lead remains.
    #[tokio::test]
    async fn small_file_completion_replaces_the_read_block_lead() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let mut body = String::from("fn configure_runtime_defaults() {\n");
        for i in 1..=14 {
            body.push_str(&format!(
                "    let setting_number_{i:02} = compute_value({i});\n"
            ));
        }
        body.push_str("}\n");
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/src%2Fdefaults.rs",
            200,
            serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
        )
        .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let result = read(
            &provider,
            serde_json::json!({"owner":"a","repo":"b","path":"src/defaults.rs","ref":sha,
                "matchString":"setting_number_08","contextLines":4}),
        )
        .await;
        let file = &result.files[0];
        assert_eq!(file.content.content.as_deref(), Some(body.as_str()));
        assert_eq!(file.content.matched_lines, vec![9]);
        assert!(file.next.is_none(), "{:?}", file.next);
    }

    #[tokio::test]
    async fn oversized_full_content_is_partial_not_empty() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        mount_json(&server, "/api/v3/repos/a/b/contents/big.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("line of text\n".repeat(6000))})).await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": "big.txt", "ref": sha,
            "fullContent": true, "mainGoal": "test", "reasoning": "test"
        }))
        .expect("query");
        let result = execute_default_regex(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(2), 1 << 20),
            Some(50_000),
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

    async fn run_default(
        provider: &GitHubProvider<NoCache>,
        query: Value,
    ) -> Result<FileRead, crate::tools::gh_shared::GhFailure> {
        let mut query = query;
        query["mainGoal"] = "test".into();
        query["reasoning"] = "test".into();
        let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
        run(
            provider,
            &query,
            Ok(&fixture_context(Duration::from_secs(5), 1 << 20)),
            None,
            &Safe,
            &NeverCancel,
            &crate::tools::local_fetch::LocalFetchRegex::default(),
        )
        .await
    }

    /// A path whose case differs is answered by one listing walk at the
    /// resolved commit: one case-corrected `read` lead, and the one hint
    /// names where that lead is. No listing at `HEAD` is spent.
    #[tokio::test]
    async fn wrong_case_path_leads_once_to_the_case_corrected_read() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sha))
            .mount(&server)
            .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/src%2FLib.rs",
            404,
            serde_json::json!({"message":"Not Found"}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/src",
            200,
            serde_json::json!([
                {"name":"lib.rs","path":"src/lib.rs","type":"file"},
                {"name":"main.rs","path":"src/main.rs","type":"file"}
            ]),
        )
        .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let failure = match run_default(
            &provider,
            serde_json::json!({"owner":"a","repo":"b","path":"src/Lib.rs"}),
        )
        .await
        {
            Err(failure) => failure,
            Ok(_) => panic!("a missing path is a failure"),
        };
        let next = failure.next.expect("recovery leads");
        assert_eq!(
            next["read"]["query"]["queries"][0]["path"], "src/lib.rs",
            "{next}"
        );
        assert_eq!(
            failure.hints,
            vec!["Only the path's case differs; run hints.read.".to_owned()]
        );
        let requests = server.received_requests().await.unwrap_or_default();
        assert!(
            !requests
                .iter()
                .any(|request| request.url.query().is_some_and(|q| q.contains("ref=HEAD"))),
            "a listing at HEAD was spent: {requests:?}"
        );
    }

    /// A matchString with no hit in an existing file says so with the
    /// file-specific tip and an empty `content` (the contract requires it, as
    /// in localFetch); it carries no zero counters or echo of the request.
    #[tokio::test]
    async fn no_match_read_keeps_its_tip_without_empty_echo() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\ntwo\n")}),).await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let read = run_default(
            &provider,
            serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"absent"}),
        )
        .await
        .expect("read");
        let data = &read.output.data;
        assert_eq!(read.output.status, Some("empty"), "{data}");
        assert_eq!(data["matchNotFound"], true, "{data}");
        assert_eq!(data["content"], "", "{data}");
        // As shipped: the response stage moves prose hints under hints.text.
        let mut shipped = data.clone();
        shipped["hints"] = serde_json::json!({"text": data["hints"]});
        crate::contracts::validate_output(
            "ghGetFileContent",
            &serde_json::json!({"results":[{"index":0,"status":"empty","data":shipped}]}),
        )
        .expect("an empty match row satisfies the output contract");
        for junk in [
            "returnedLines",
            "returnedChars",
            "searchedFor",
            "pagination",
        ] {
            assert!(data.get(junk).is_none(), "{junk}: {data}");
        }
        let hint = data["hints"][0].as_str().unwrap_or_default();
        assert!(hint.starts_with("No line contains this text"), "{data}");
        assert_eq!(data["hints"].as_array().map(Vec::len), Some(1), "{data}");
        // FIX §0 #4: a runnable lead beside the tip, a repository search for
        // a small file.
        let lead = &data["next"]["searchCode"];
        assert_eq!(lead["tool"], "ghSearchCode", "{data}");
        assert_eq!(
            lead["query"]["queries"][0]["keywords"],
            serde_json::json!(["absent"]),
            "{data}"
        );
        // A case-insensitive miss has no ignoreCase lead.
        assert!(data["next"].get("ignoreCase").is_none(), "{data}");
        // QA2: the shared localFetch miss offers a localSearch of the file's
        // directory; it has no GitHub equivalent, so it must not be rewritten
        // into a ghGetFileContent read of that directory (an invalidInput
        // "is a directory" row when run verbatim).
        assert!(data["next"].get("textSearch").is_none(), "{data}");
        for (name, lead) in data["next"].as_object().into_iter().flatten() {
            if lead["tool"] == "ghGetFileContent" {
                assert_eq!(
                    lead["query"]["queries"][0]["path"], "a.txt",
                    "{name} reads another path: {data}"
                );
            }
        }

        // X12: a case-sensitive miss runs the same read, ignoring case, at
        // the read's commit; the tip names the lead, not the field.
        let sensitive = run_default(
            &provider,
            serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"Two"}),
        )
        .await
        .expect("read");
        let data = &sensitive.output.data;
        let hint = data["hints"][0].as_str().unwrap_or_default();
        assert!(
            hint.contains("ignoreCase") && !hint.contains("caseMode"),
            "{data}"
        );
        let row = &data["next"]["ignoreCase"]["query"]["queries"][0];
        assert_eq!(data["next"]["ignoreCase"]["tool"], "ghGetFileContent");
        assert_eq!(row["caseMode"], "insensitive", "{data}");
        assert_eq!(row["matchString"], "Two", "{data}");
        assert_eq!(row["ref"], sha, "{data}");
        let mut shipped = data.clone();
        shipped["hints"] = serde_json::json!({"text": data["hints"]});
        crate::contracts::validate_output(
            "ghGetFileContent",
            &serde_json::json!({"results":[{"index":0,"status":"empty","data":shipped}]}),
        )
        .expect("an ignoreCase lead satisfies the output contract");
    }

    /// FIX §0 #4: a literal missed in a large file leads to a clasify locate
    /// of that file at the read's commit.
    #[tokio::test]
    async fn no_match_in_a_large_file_leads_to_clasify() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let text = (0..1200).map(|n| format!("line {n}\n")).collect::<String>();
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/big.txt",
            200,
            serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&text)}),
        )
        .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let read = run_default(
            &provider,
            serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha,"matchString":"absent"}),
        )
        .await
        .expect("read");
        let data = &read.output.data;
        let lead = &data["next"]["clasify"];
        assert_eq!(lead["tool"], "clasify", "{data}");
        let matrix = &lead["query"]["queries"][0];
        assert_eq!(matrix["resources"][0]["query"]["ref"], sha, "{data}");
        assert_eq!(matrix["resources"][0]["query"]["path"], "big.txt", "{data}");
        assert_eq!(matrix["questions"][0]["type"], "locate", "{data}");
    }
}
