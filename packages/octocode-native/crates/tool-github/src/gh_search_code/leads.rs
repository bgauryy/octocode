//! ghSearchCode leads: required unresolved-source reads and an empty search's recovery.
use super::code_output::{requested_ref, row_repo};
use super::cut::Unlisted;
use octocode_contract_types::{
    CaseMode, ContentView, GhGetFileContentQuery, GhGetFileContentQueryContentRanges,
    GhSearchCodeQuery, GhSearchCodeQueryMatchInItem, RegexEngine, SourceLineRange,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, fresh_query_row, query_row};
use octocode_tools_shared::leads::broaden::{Broaden, broaden};
use octocode_tools_shared::leads::gh_file_read::{Target, Terms, match_read, plain_read};
use serde_json::{Value, json};
use std::num::NonZeroU64;

/// Read source keyword lines when the provider result could not be resolved
/// to a locally scanned blob. A resolved blob lists every hit, or a run of
/// them with the rest in its [`unlisted_read`].
pub(super) fn hits_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let target = Target {
        owner,
        repo,
        path,
        reference: (!sha.is_empty())
            .then_some(sha)
            .or_else(|| requested_ref(query)),
    };
    let read = match_read(&target, keyword_terms(query)?, Some((0, 0)), false)?;
    Some(Continuation::continued_query(read).build())
}

/// The source block around the highest-ranked hit, pinned to the commit read
/// by the search.
pub(super) fn source_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    sha: &str,
    hit_line: u32,
) -> Option<GhGetFileContentQuery> {
    if !octocode_github::is_full_sha(sha) {
        return None;
    }
    let (owner, repo) = row_repo(query, row)?;
    let line = NonZeroU64::new(u64::from(hit_line))?;
    let read = GhGetFileContentQueryContentRanges {
        owner: owner.try_into().ok()?,
        repo: repo.try_into().ok()?,
        path: row.get("path")?.as_str()?.try_into().ok()?,
        ref_: Some(sha.try_into().ok()?),
        ranges: vec![SourceLineRange {
            start_line: line,
            end_line: line,
        }],
        block: Some(true),
        view: ContentView::Content,
        main_goal: None,
        reasoning: None,
        read_progress: None,
    };
    Some(read.into())
}

/// The read of the hit lines a cut row left unlisted ([`Unlisted`]): its
/// keywords as the listing matched them, with the query's context, inside
/// the line ranges the row's run left, at the commit the lines were read
/// at. Only hits inside a range count there and their context stays inside
/// it, so it lists each line the row left exactly once.
pub(super) fn unlisted_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    sha: &str,
    unlisted: &Unlisted,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let target = Target {
        owner,
        repo,
        path,
        reference: (!sha.is_empty())
            .then_some(sha)
            .or_else(|| requested_ref(query)),
    };
    let context = query
        .context_lines
        .as_ref()
        .map(|lines| (lines.before, lines.after));
    let mut read = match_read(&target, keyword_terms(query)?, context, false)?;
    let GhGetFileContentQuery::ContentMatchString(matched) = &mut read else {
        return None;
    };
    matched.ranges = unlisted
        .ranges()
        .map(|(start, end)| {
            Some(SourceLineRange {
                start_line: NonZeroU64::new(u64::from(start))?,
                end_line: NonZeroU64::new(u64::from(end))?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Continuation::continued_query(read).build())
}

/// The query's keywords as the terms of a ghGetFileContent read, matching
/// the way the listing does. `None` when the query has no keyword.
fn keyword_terms(query: &GhSearchCodeQuery) -> Option<Terms> {
    let keywords: Vec<&str> = query
        .keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    let max =
        octocode_runtime_base::contracts::query_limits::gh_get_file_content::MATCH_STRING_MAX_ITEMS;
    // The listing matched case-insensitively; smart case would turn an
    // uppercase keyword case-sensitive and miss counted hits.
    let case_mode = if keywords
        .iter()
        .any(|keyword| keyword.chars().any(char::is_uppercase))
    {
        CaseMode::Insensitive
    } else {
        CaseMode::Smart
    };
    let (terms, regex_engine) = match keywords.as_slice() {
        [] => return None,
        many if many.len() <= max => (
            many.iter().map(|keyword| (*keyword).to_owned()).collect(),
            RegexEngine::Literal,
        ),
        // More keywords than a matchString list holds: one escaped
        // alternation matches the same lines.
        many => (
            vec![
                many.iter()
                    .map(|keyword| regex::escape(keyword))
                    .collect::<Vec<_>>()
                    .join("|"),
            ],
            RegexEngine::Rust,
        ),
    };
    Some(Terms {
        terms,
        case_mode,
        regex_engine,
    })
}

/// The read of a file a `path` scope names: an empty search of that file
/// reads it for the keywords instead, since the index may not hold it (its
/// text when the search matched file contents).
pub(super) fn file_scope_read(query: &GhSearchCodeQuery, file: &str) -> Option<Value> {
    let target = Target {
        owner: query.owner.as_str(),
        repo: query.repo.as_deref()?,
        path: file,
        reference: requested_ref(query),
    };
    let terms = query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Content)
        .then(|| keyword_terms(query))
        .flatten();
    // No keyword is a plain read of the file's start.
    let read = match terms {
        Some(terms) => match_read(&target, terms, None, false)?,
        None => plain_read(&target)?,
    };
    Some(Continuation::related_query(read).build())
}

/// The read of a file the index did not search at the requested ref: the
/// query's `path` and `filename` name one file of a repo-scoped search.
pub(super) fn unsearched_ref_read(query: &GhSearchCodeQuery) -> Option<Value> {
    let reference = requested_ref(query)?;
    let repo = query.repo.as_deref()?;
    let file = query.name_contains.as_deref()?;
    let path = match query
        .path
        .as_deref()
        .map(|path| path.as_str().trim_matches('/'))
    {
        Some(dir) if !dir.is_empty() => format!("{dir}/{file}"),
        _ => file.to_owned(),
    };
    let target = Target {
        owner: query.owner.as_str(),
        repo: repo.as_str(),
        path: &path,
        reference: Some(reference),
    };
    Some(Continuation::related_query(plain_read(&target)?).build())
}

/// Keywords are ANDed literals, so an item holding a standalone `OR` ran as
/// one phrase. The same search once per alternative, in one call; `None`
/// when no item has one or the alternatives exceed one call's rows.
pub(super) fn or_split(query: &GhSearchCodeQuery) -> Option<Value> {
    let (index, alternatives) = query
        .keywords
        .iter()
        .enumerate()
        .find_map(|(index, item)| {
            let words = item.split_whitespace().collect::<Vec<_>>();
            let alternatives = words
                .split(|word| *word == "OR")
                .filter(|group| !group.is_empty())
                .map(|group| group.join(" "))
                .collect::<Vec<_>>();
            (alternatives.len() > 1).then_some((index, alternatives))
        })?;
    if alternatives.len()
        > octocode_runtime_base::response::result::rows_per_call(ToolId::GhSearchCode)
    {
        return None;
    }
    let rows = alternatives
        .into_iter()
        .map(|alternative| {
            let mut row = fresh_query_row(ToolId::GhSearchCode, &query_row(query, &[]));
            row["keywords"][index] = json!(alternative);
            row
        })
        .collect::<Vec<_>>();
    Some(Continuation::related_input(ToolId::GhSearchCode, json!({ "queries": rows })).build())
}

/// Variants an empty search offers at most: each one is another code search
/// against a quota of 10 per minute.
const BROADEN_VARIANTS: usize = 2;

/// An empty search's broader variants ([`broaden`]): one per set filter
/// (`path`, `filename`, `extensions`), then one per dropped keyword. No more
/// are offered than `quota_room`, the code searches the quota still covers.
pub(super) fn broaden_search(query: &GhSearchCodeQuery, quota_room: usize) -> Option<Value> {
    broaden(
        ToolId::GhSearchCode,
        &query_row(query, &[]),
        &Broaden {
            // Code search indexes only the default branch: a `ref` adds
            // nothing to a broader search.
            strip: &["fields", "ref"],
            groups: &[&["path"], &["nameContains"], &["extensions"]],
            split_lone_keyword: false,
        },
        quota_room.min(BROADEN_VARIANTS),
    )
}

#[cfg(test)]
mod tests;
