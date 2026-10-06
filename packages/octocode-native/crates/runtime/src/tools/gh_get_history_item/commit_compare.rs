//! `operation: "commit"` and `operation: "compare"`: commit metadata or a
//! ref comparison, each with one page of changed files.
use super::filter::{PathScope, in_path_scope, scope_files};
use super::patch::{attach_patch_cursor, clamp_warning, push_warning, shape_files};
use super::patch_hop::{DiffCursors, attach_diff_continuations};
use super::pr_menu::{ChangeSides, change_reads};
use super::util::{array, compare_identity, str_at, string, usize_at};
use super::window::{
    MAX_FILE_BATCHES, WindowSpec, commit_file_items, load_window_with, mark_capped,
    paginate_collection, paginate_window,
};
use super::{HistoryItemRequest, ItemOperation, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, remove_nulls};
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
    let scope = PathScope::from_query(query).map_err(|message| validation(&message))?;
    let path = scope.as_ref();
    // File batches are derived from filePage×pageSize; a path scope hides
    // files, so it scans from the first batch.
    let loaded = load_window_with(
        transport,
        &["repos", query.owner(), query.repo(), "commits", reference],
        WindowSpec {
            max_batches: MAX_FILE_BATCHES,
            page: query.file_page().unwrap_or(1),
            page_size: query.collection_page_size(),
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
    let headline = message.lines().next().unwrap_or(message);
    // A patch read is the diff's evidence: it names the commit by its
    // headline once; the summary read (`hints.readCommit`) holds the rest.
    let patches = query.include_diff();
    let mut out = json!({
        "owner":query.owner(),"repo":query.repo(),"ref":reference,"sha":sha,
        "message":(!patches).then_some(message),"messageHeadline":headline,
        "author":identity(raw,"author"),"committer":identity(raw,"committer"),
        "parents":raw.get("parents").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/sha").map(str::to_owned)).collect::<Vec<_>>(),
        "additions":raw.pointer("/stats/additions"),"deletions":raw.pointer("/stats/deletions"),
        "changedFilesCount":state.skipped + scoped.len(),
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
            totals.insert("changedFilesCount".into(), json!(listed));
        }
        if !totals.is_empty() {
            fields.insert("commitTotals".into(), Value::Object(totals));
        }
    }
    let (files, mut page) = paginate_window(
        scoped,
        state.skipped,
        state.exhausted,
        query.file_page(),
        query.page_size(),
    );
    mark_capped(&mut page, state.capped);
    // A file page's first patch window reads its top file at both sides of
    // the numbered diff: the commit and its first parent.
    let parent = raw
        .pointer("/parents/0/sha")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let side_reads = if patches && query.char_offset().is_none_or(|offset| offset == 0) {
        change_reads(
            query.owner(),
            query.repo(),
            &files,
            &ChangeSides {
                new_ref: Some(sha.as_str()),
                old_ref: parent.as_deref(),
                old_confidence: "high",
            },
            |_| true,
        )
    } else {
        Vec::new()
    };
    // Without the patches section the page still lists paths and line stats (no
    // patches) so the agent can pick files before paying for diffs.
    let (files, cursor) = shape_files(files, query.include_diff(), query);
    attach_patch_cursor(&mut page, cursor);
    let cursors = DiffCursors::of_file_page(
        &page,
        files.as_array().is_some_and(|files| !files.is_empty()),
    );
    out["files"] = files;
    out["filePagination"] = page;
    if query.include_diff()
        && let Some(warning) = clamp_warning(query)
    {
        push_warning(&mut out, warning);
    }
    attach_diff_continuations(
        &mut out,
        query,
        ItemOperation::Commit,
        Some(&sha),
        false,
        cursors,
    );
    // Leads in rank order: the rest of the message, then both sides of the
    // diff; the response keeps the first ones its lead cap allows.
    let mut leads = Vec::new();
    if patches && message.trim_end() != headline {
        leads.push((
            "readCommit",
            Continuation::new(
                ToolId::GhGetHistoryItem,
                json!({"operation":"commit","owner":query.owner(),"repo":query.repo(),"ref":sha}),
            )
            .why("Read the full commit message.")
            .confidence("exact")
            .build(),
        ));
    }
    leads.extend(side_reads);
    lead_first(&mut out, leads);
    remove_nulls(&mut out);
    Ok(out)
}

/// Put `leads` ahead of the row's other leads, in order (pages keep their
/// places: the response caps leads only).
fn lead_first(out: &mut Value, leads: Vec<(&'static str, Value)>) {
    if leads.is_empty() {
        return;
    }
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    if let Some(next) = out["next"].as_object_mut() {
        for (name, lead) in leads.into_iter().rev() {
            next.shift_insert(0, name.to_owned(), lead);
        }
    }
}

pub(super) async fn compare<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let page = query.page().unwrap_or(1);
    let per = query.collection_page_size();
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
    // A later file page or patch window carries files only: the commit list
    // is paged by `page` and was delivered with the first window.
    let file_page =
        query.file_page().unwrap_or(1) > 1 || query.char_offset().is_some_and(|offset| offset > 0);
    let all_files = array(raw.get("files").cloned().unwrap_or(json!([])));
    let file_limit = all_files.len() >= COMPARE_FILE_LIMIT;
    let scope = PathScope::from_query(query).map_err(|message| validation(&message))?;
    let scoped = scope_files(all_files, scope.as_ref());
    let mut out = json!({"owner":query.owner(),"repo":query.repo(),"base":base,"head":head,
        "compareStatus": raw.get("status"),
        "aheadBy":usize_at(&raw,"/ahead_by"),"behindBy":usize_at(&raw,"/behind_by"),"totalCommits":total,
        "isPartial":((more && !file_page)||file_limit).then_some(true)});
    if !file_page {
        // A path-scoped comparison lists every commit of the range, not
        // only the path's: headlines name them, and `hints.narrowScope`
        // lists the path's own commits.
        let scoped = query.path().is_some();
        out["commits"] = json!(array(raw.get("commits").cloned().unwrap_or(json!([]))).into_iter().map(|v|{
            let message = str_at(&v,"/commit/message").unwrap_or("");
            let mut row = json!({
                "sha":v["sha"],
                "author":str_at(&v,"/commit/author/name").or_else(||str_at(&v,"/author/login")).unwrap_or("unknown"),"date":super::util::utc_date(str_at(&v,"/commit/author/date"))
            });
            if scoped {
                row["messageHeadline"] = json!(message.lines().next().unwrap_or(message));
            } else {
                row["message"] = json!(message);
            }
            row
        }).collect::<Vec<_>>());
        if scoped {
            out["commitsScope"] = json!("range");
        }
        // The last commit page past the first needs no page object.
        if more || page == 1 {
            out["pagination"] = json!({"currentPage":page,"pageSize":per,"hasMore":more,"nextPage":more.then_some(page+1)});
        }
    }
    if file_limit {
        out["terminalLimit"] = json!(true);
        out["partialReasons"] = json!(["providerFileLimit"]);
        out["providerLimit"] = json!({"reason":"providerFileLimit","maxFiles":COMPARE_FILE_LIMIT});
    }
    let mut cursors = DiffCursors {
        commit_page: (more && !file_page).then(|| u64::try_from(page + 1).unwrap_or(u64::MAX)),
        ..DiffCursors::default()
    };
    let mut side_reads = Vec::new();
    if page == 1 {
        let include_diff = query.include_diff();
        if !include_diff {
            out["changedFilesCount"] = json!(scoped.len());
            if file_limit {
                // GitHub stops listing at 300: the count is a floor.
                out["changedFilesCountScope"] = json!("partial");
            }
        }
        // Without the patches section the page lists paths and line stats only.
        let (files, mut page) = paginate_collection(scoped, query.file_page(), query.page_size());
        if file_limit {
            page["countScope"] = json!("partial");
        }
        // The old side of a three-dot comparison is the merge base.
        if include_diff && query.char_offset().is_none_or(|offset| offset == 0) {
            side_reads = change_reads(
                query.owner(),
                query.repo(),
                &files,
                &ChangeSides {
                    new_ref: out["head"].as_str(),
                    old_ref: str_at(&raw, "/merge_base_commit/sha"),
                    old_confidence: "high",
                },
                |_| true,
            );
        }
        let (files, cursor) = shape_files(files, include_diff, query);
        attach_patch_cursor(&mut page, cursor);
        cursors = DiffCursors {
            commit_page: cursors.commit_page,
            ..DiffCursors::of_file_page(
                &page,
                files.as_array().is_some_and(|files| !files.is_empty()),
            )
        };
        out["files"] = files;
        out["filePagination"] = page;
        if include_diff && let Some(warning) = clamp_warning(query) {
            push_warning(&mut out, warning);
        }
    }
    // Continuations read the same two commits.
    let pinned = query.with_compare_refs(
        out["base"].as_str().unwrap_or_default(),
        out["head"].as_str().unwrap_or_default(),
    );
    attach_diff_continuations(
        &mut out,
        &pinned,
        ItemOperation::Compare,
        None,
        false,
        cursors,
    );
    // Past GitHub's file list a scoped path looks unchanged when it may not
    // be: its commits up to the pinned head say what changed there. A
    // `path` always offers that history; an `include` scope only past the
    // cap (its literal prefix: a glob's directory, or the exact path).
    let scope_path = query.path().map(str::to_owned).or_else(|| {
        file_limit
            .then(|| {
                query
                    .file_scope
                    .iter()
                    .find_map(|pattern| literal_prefix(pattern))
            })
            .flatten()
    });
    let scoped_request = query.path().is_some() || !query.file_scope.is_empty();
    if file_limit && scoped_request {
        let named = query
            .path()
            .map(str::to_owned)
            .unwrap_or_else(|| query.file_scope.join(", "));
        let lead = if scope_path.is_some() {
            "; run hints.narrowScope"
        } else {
            "; list them with ghSearchHistory operation:\"commit\" and a path"
        };
        push_warning(
            &mut out,
            format!(
                "GitHub lists at most {COMPARE_FILE_LIMIT} changed files, so changes to {named} may be missing{lead}."
            ),
        );
    }
    if let Some(path) = scope_path {
        let mut lead = vec![(
            "narrowScope",
            Continuation::new(
                ToolId::GhSearchHistory,
                json!({"operation":"commit","owner":query.owner(),"repo":query.repo(),"path":path,"ref":out["head"]}),
            )
            .why("List this path's commits up to head.")
            .confidence("high")
            .build(),
        )];
        // Past the cap the path history is the only complete answer: it
        // leads; otherwise the change's own reads come first.
        if file_limit {
            lead.extend(side_reads);
            side_reads = lead;
        } else {
            side_reads.extend(lead);
        }
    }
    lead_first(&mut out, side_reads);
    Ok(out)
}

/// The literal path prefix of an `include` pattern: the pattern itself
/// without glob characters, else the directory before its first glob
/// (`packages/react/**` → `packages/react/`); `None` for a bare glob.
fn literal_prefix(pattern: &str) -> Option<String> {
    let glob = pattern.find(['*', '?', '[', '{']);
    let prefix = match glob {
        None => pattern,
        Some(at) => pattern[..at].rsplit_once('/').map_or("", |(dir, _)| dir),
    };
    let prefix = prefix.trim_start_matches("./");
    if prefix.is_empty() {
        return None;
    }
    Some(if glob.is_some() {
        format!("{prefix}/")
    } else {
        prefix.to_owned()
    })
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn identity(raw: &Value, kind: &str) -> Value {
    let p = format!("/commit/{kind}");
    let login = format!("/{kind}/login");
    let mut out = json!({"name":str_at(raw,&format!("{p}/name")).unwrap_or("unknown"),"email":str_at(raw,&format!("{p}/email")).unwrap_or(""),"login":str_at(raw,&login),"date":str_at(raw,&format!("{p}/date")).map(|date| super::util::utc_date(Some(date)))});
    remove_nulls(&mut out);
    out
}
