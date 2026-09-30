//! `operation: "commit"` and `operation: "compare"`: commit metadata or a
//! ref comparison, each with one page of changed files.
use super::continuations::attach_diff_continuations;
use super::files::{in_path_scope, scope_files, shape_files};
use super::util::{array, compare_identity, str_at, string, usize_at};
use super::window::{
    MAX_FILE_BATCHES, WindowSpec, commit_file_items, commit_files_pagination, load_window_with,
    mark_capped, paginate_collection, paginate_window,
};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use crate::tools::result::remove_nulls;
use serde_json::{Value, json};

/// GitHub's compare endpoint lists at most this many changed files.
const COMPARE_FILE_LIMIT: usize = 300;

pub(super) async fn commit<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let reference = query
        .reference()
        .ok_or_else(|| validation("ref is required"))?;
    let path = query.path();
    // File batches are derived from filePage×pageSize; a path scope hides
    // files, so it scans from the first batch.
    let loaded = load_window_with(
        transport,
        &["repos", query.owner(), query.repo(), "commits", reference],
        WindowSpec {
            max_batches: MAX_FILE_BATCHES,
            page: query.file_page().unwrap_or(1),
            page_size: query.page_size().unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100),
            filtered: path.is_some(),
            provider_total: None,
        },
        |file| in_path_scope(file, path),
        commit_file_items,
        context,
    )
    .await?;
    let state = loaded.state;
    let raw = &loaded.first;
    let listed = state.skipped + loaded.items.len();
    let scoped = scope_files(loaded.items, path);
    let sha = string(raw.get("sha"));
    let message = str_at(raw, "/commit/message").unwrap_or("");
    let mut out = json!({
        "type":"commit","owner":query.owner(),"repo":query.repo(),"ref":reference,"sha":sha,
        "message":message,"messageHeadline":message.lines().next().unwrap_or(message),
        "author":identity(raw,"author"),"committer":identity(raw,"committer"),
        "parents":raw.get("parents").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/sha").map(str::to_owned)).collect::<Vec<_>>(),
        "additions":raw.pointer("/stats/additions"),"deletions":raw.pointer("/stats/deletions"),
        "changedFiles":state.skipped + scoped.len(),
        // A scan stopped at the batch cap never saw the remaining files.
        "changedFilesCountScope":if state.capped {"partial"} else if state.exhausted {"complete"} else {"loaded"}
    });
    // A caller-supplied SHA is not restated as the ref it already names.
    if reference.eq_ignore_ascii_case(&sha)
        && let Some(fields) = out.as_object_mut()
    {
        fields.remove("ref");
    }
    // A path scope counts only its files, while GitHub's line stats cover the
    // whole commit: name those totals as the commit's, never the scope's.
    if path.is_some()
        && let Some(fields) = out.as_object_mut()
    {
        let mut totals = serde_json::Map::new();
        for key in ["additions", "deletions"] {
            if let Some(value) = fields.remove(key).filter(|value| !value.is_null()) {
                totals.insert(key.into(), value);
            }
        }
        if state.exhausted && !state.capped {
            totals.insert("changedFiles".into(), json!(listed));
        }
        if !totals.is_empty() {
            fields.insert("commitTotals".into(), Value::Object(totals));
        }
    }
    let (files, page) = paginate_window(
        scoped,
        state.skipped,
        state.exhausted,
        query.file_page(),
        query.page_size(),
    );
    let mut page = commit_files_pagination(page);
    mark_capped(&mut page, state.capped);
    // Without includeDiff the page still lists paths and line stats (no
    // patches) so the agent can pick files before paying for diffs.
    out["files"] = shape_files(files, query.include_diff(), query);
    out["filesPagination"] = page;
    attach_diff_continuations(&mut out, query, ItemOperation::Commit, Some(&sha), false);
    Ok(out)
}

pub(super) async fn compare<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let page = query.page().unwrap_or(1);
    let per = query.page_size().unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100);
    let base = query.base().ok_or_else(|| validation("base is required"))?;
    let head = query.head().ok_or_else(|| validation("head is required"))?;
    // The head commit is the last of the comparison, often past this commit
    // page: resolve a movable head (branch, tag) to the commit read so the
    // echo and every continuation name one comparison. A cross-repository
    // `owner:ref` head stays as written.
    let head = if head.contains(':') || is_full_sha(head) {
        head.to_owned()
    } else {
        transport
            .commit_sha(query.owner(), query.repo(), head, context)
            .await?
    };
    let refs = format!("{base}...{head}");
    let (raw, link_more) = fetch(
        transport,
        &["repos", query.owner(), query.repo(), "compare", &refs],
        &[("page", page.to_string()), ("per_page", per.to_string())],
        context,
    )
    .await?;
    let total = usize_at(&raw, "/total_commits");
    let more = link_more || page.saturating_mul(per) < total;
    let (base, head) = compare_identity(&raw, base, &head);
    // A file page (filePage > 1) carries files only: the commit list is
    // paged by `page` and was delivered with the first file page.
    let file_page = query.file_page().unwrap_or(1) > 1;
    let all_files = array(raw.get("files").cloned().unwrap_or(json!([])));
    let file_limit = all_files.len() >= COMPARE_FILE_LIMIT;
    let scoped = scope_files(all_files, query.path());
    let mut out = json!({"type":"compare","owner":query.owner(),"repo":query.repo(),"base":base,"head":head,
        "status": raw.get("status"),
        "aheadBy":usize_at(&raw,"/ahead_by"),"behindBy":usize_at(&raw,"/behind_by"),"totalCommits":total,
        "isPartial":(more||file_limit).then_some(true)});
    if !file_page {
        out["commits"] = json!(array(raw.get("commits").cloned().unwrap_or(json!([]))).into_iter().map(|v|json!({
            "sha":v["sha"],"messageHeadline":str_at(&v,"/commit/message").unwrap_or("").lines().next().unwrap_or(""),
            "author":str_at(&v,"/commit/author/name").or_else(||str_at(&v,"/author/login")).unwrap_or("unknown"),"date":str_at(&v,"/commit/author/date").unwrap_or("")
        })).collect::<Vec<_>>());
        // The last commit page past the first needs no page object.
        if more || page == 1 {
            out["pagination"] = json!({"currentPage":page,"perPage":per,"hasMore":more,"nextPage":more.then_some(page+1)});
        }
    }
    if file_limit {
        out["terminalLimit"] = json!(true);
        out["partialReasons"] = json!(["providerFileLimit"]);
        out["providerLimit"] = json!({"reason":"providerFileLimit","maxFiles":COMPARE_FILE_LIMIT});
    }
    if page == 1 {
        let include_diff = query.include_diff();
        if !include_diff {
            out["changedFiles"] = json!(scoped.len());
            if file_limit {
                // GitHub stops listing at 300: the count is a floor.
                out["changedFilesCountScope"] = json!("partial");
            }
        }
        // Without includeDiff the page lists paths and line stats only.
        let (files, page) = paginate_collection(scoped, query.file_page(), query.page_size());
        let mut page = commit_files_pagination(page);
        if file_limit {
            page["countScope"] = json!("partial");
        }
        out["files"] = shape_files(files, include_diff, query);
        out["filesPagination"] = page;
    }
    // Continuations read the same two commits.
    let pinned = query.with_compare_refs(
        out["base"].as_str().unwrap_or_default(),
        out["head"].as_str().unwrap_or_default(),
    );
    attach_diff_continuations(&mut out, &pinned, ItemOperation::Compare, None, false);
    Ok(out)
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn identity(raw: &Value, kind: &str) -> Value {
    let p = format!("/commit/{kind}");
    let login = format!("/{kind}/login");
    let mut out = json!({"name":str_at(raw,&format!("{p}/name")).unwrap_or("unknown"),"email":str_at(raw,&format!("{p}/email")).unwrap_or(""),"login":str_at(raw,&login),"date":str_at(raw,&format!("{p}/date"))});
    remove_nulls(&mut out);
    out
}
