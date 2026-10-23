//! ghSearchCode leads: the reads a shaped page offers (top hit, capped hit
//! lists, a file the index did not search at the requested ref).
use super::GhSearchCodeQuery;
use super::code_output::{Resolution, requested_ref, row_repo};
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, continuation_row_mut, query_row};
use crate::tools::leads::broaden::{Broaden, broaden};
use serde_json::{Value, json};

/// Code-search fragments carry no line numbers. Point the top hit at an exact
/// ghGetFileContent match read, whose sourceLineRanges carry line numbers.
/// Only a code hit (not docs, changelogs, or tests, which GitHub often ranks
/// first) earns the lead.
pub(super) fn read_top_match(top: &Value) -> Option<Value> {
    let file = top.as_object()?;
    let matched = file.get("matches")?.as_array()?.first()?;
    let anchor = &matched["matchIndices"][0];
    let start = usize::try_from(anchor["start"].as_u64()?).ok()?;
    let end = usize::try_from(anchor["end"].as_u64()?).ok()?;
    let token = crate::content::utf16_slice(matched["value"].as_str()?, start, end)?;
    if token.trim().is_empty() {
        return None;
    }
    let path = file.get("path")?.as_str()?;
    if !code_hit(path) {
        return None;
    }
    // A cross-tool read states its reason in `why`; its query carries only
    // the brief the caller sent, which the response stage copies.
    Some(
        Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": file.get("owner")?,
                "repo": file.get("repo")?,
                "path": file.get("path")?,
                "matchString": token,
                "contextLines": 5,
            }),
        )
        .confidence(Confidence::Medium)
        .build(),
    )
}

/// A source file outside tests: the only hit a top-match read is worth.
pub(super) fn code_hit(path: &str) -> bool {
    matches!(
        crate::content::classify_file_type(path),
        Some(crate::content::FileType::Code)
    ) && !crate::content::is_test_path(path)
}

/// ghGetFileContent read of the keyword lines of one file at the resolved
/// commit, whole and numbered: the lossless continuation of a hit list that
/// was capped or cut to keyword windows. The same case-insensitive literal
/// any-of match as the listing. `resume` (the last shown line and the line
/// count) reads only the lines after the shown hits, so none repeats; without
/// it the read covers every keyword line of the file again.
pub(super) fn hits_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    sha: &str,
    resume: Option<(u32, usize)>,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let mut read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        "ref": sha,
        "contextLines": 0,
    });
    if let Some((after, line_count)) = resume
        && usize::try_from(after).is_ok_and(|after| after < line_count)
    {
        read["ranges"] = json!([format!("{}-{line_count}", after + 1)]);
    }
    keyword_match(&mut read, query)?;
    Some(Continuation::new(ToolId::GhGetFileContent, read).build())
}

/// Sets `matchString` (and `caseMode`, `regex`) on a ghGetFileContent read so
/// it matches the query's keywords the way the listing does. `None` when the
/// query has no keyword.
fn keyword_match(read: &mut Value, query: &GhSearchCodeQuery) -> Option<()> {
    let keywords: Vec<&str> = query
        .keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    let max = crate::contracts::query_limits::gh_get_file_content::MATCH_STRING_MAX_ITEMS;
    // The listing matched case-insensitively; smart case would turn an
    // uppercase keyword case-sensitive and miss counted hits.
    if keywords
        .iter()
        .any(|keyword| keyword.chars().any(char::is_uppercase))
    {
        read["caseMode"] = json!("insensitive");
    }
    match keywords.as_slice() {
        [] => return None,
        [one] => read["matchString"] = json!(one),
        many if many.len() <= max => read["matchString"] = json!(many),
        // More keywords than a matchString list holds: one escaped
        // alternation matches the same lines.
        many => {
            read["matchString"] = json!(
                many.iter()
                    .map(|keyword| regex::escape(keyword))
                    .collect::<Vec<_>>()
                    .join("|")
            );
            read["regex"] = json!("rust");
        }
    }
    Some(())
}

/// The read of a file a `path` scope names: the index matches `path` as a
/// directory prefix, so an empty search under a file's path reads that file
/// for the keywords instead (its text when the search matched file contents).
pub(super) fn file_scope_read(query: &GhSearchCodeQuery, file: &str) -> Option<Value> {
    let repo = query.repo.as_deref()?;
    let mut read = json!({"owner": query.owner.as_str(), "repo": repo, "path": file});
    if let Some(reference) = requested_ref(query) {
        read["ref"] = json!(reference);
    }
    if query.match_in == super::GhSearchCodeQueryMatchIn::File {
        // No keyword is a plain read of the file's start.
        let _ = keyword_match(&mut read, query);
    }
    Some(Continuation::new(ToolId::GhGetFileContent, read).build())
}

/// The index-fragment read of the top file, kept inside the verified source
/// scope: pinned to the resolved commit, dropped when the file is absent
/// there, and naming the requested ref when nothing was resolved. It never
/// silently reads the default branch for a requested ref.
pub(super) fn scoped_fragment_read(
    read: Option<Value>,
    query: &GhSearchCodeQuery,
    resolution: Option<&Resolution>,
) -> Option<Value> {
    let mut read = read?;
    let branch = match resolution {
        Some(resolution) => match resolution.hits.first() {
            Some((_, super::lines::FileHits::Missing)) => return None,
            // At a ref, an unmatched or unread top file has no fragment read:
            // its fragment is default-branch text.
            Some((_, super::lines::FileHits::Unmatched | super::lines::FileHits::Unavailable))
                if resolution.reference.is_some() =>
            {
                return None;
            }
            // An owner-wide row without an indexed commit reads the
            // default branch.
            Some((sha, _)) if sha.is_empty() => return Some(read),
            Some((sha, _)) => sha.as_str(),
            None => return Some(read),
        },
        None => match requested_ref(query) {
            Some(reference) => reference,
            None => return Some(read),
        },
    };
    if let Some(row) = continuation_row_mut(&mut read) {
        row["ref"] = json!(branch);
    }
    Some(read)
}

/// ghGetFileContent read of the whole declaration the best hit names: its
/// unique head line as `matchString` with `block`, at the resolved commit.
/// A fixed line window would cut a function longer than the window.
pub(super) fn declaration_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    head: &str,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    if !code_hit(path) {
        return None;
    }
    let read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        "matchString": head,
        "block": true,
        "ref": sha,
    });
    Some(
        Continuation::new(ToolId::GhGetFileContent, read)
            .confidence(Confidence::Medium)
            .build(),
    )
}

/// ghGetFileContent read of a resolved file's best hit for the goal (5 lines
/// before it); a read anchored on the first hit widens to the last hit when
/// that is close.
pub(super) fn line_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    (first, last, best): (u32, u32, u32),
    line_count: usize,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let anchor = if best == 0 { first } else { best };
    let start = anchor.saturating_sub(5).max(1);
    let mut end = anchor.saturating_add(17);
    if anchor == first && last.saturating_sub(first) <= 40 {
        end = end.max(last.saturating_add(3));
    }
    let end = end.min(u32_of_usize(line_count).max(start));
    if !code_hit(path) {
        return None;
    }
    let read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        // The published line-span spelling (startLine/endLine are accepted
        // but not published).
        "ranges": [format!("{start}-{end}")],
        // The commit the lines were read at: a branch push cannot shift
        // the window before the read runs.
        "ref": sha,
    });
    Some(
        Continuation::new(ToolId::GhGetFileContent, read)
            .confidence(Confidence::Medium)
            .build(),
    )
}

/// The read of a file the index did not search at the requested ref: the
/// query's `path` and `filename` name one file of a repo-scoped search.
pub(super) fn unsearched_ref_read(query: &GhSearchCodeQuery) -> Option<Value> {
    let reference = requested_ref(query)?;
    let repo = query.repo.as_deref()?;
    let file = query.filename.as_deref()?;
    let path = match query
        .path
        .as_deref()
        .map(|path| path.as_str().trim_matches('/'))
    {
        Some(dir) if !dir.is_empty() => format!("{dir}/{file}"),
        _ => file.to_owned(),
    };
    Some(
        Continuation::new(
            ToolId::GhGetFileContent,
            json!({"owner": query.owner.as_str(), "repo": repo.as_str(), "path": path, "ref": reference}),
        )
        .confidence(Confidence::Medium)
        .build(),
    )
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
    if alternatives.len() > crate::response::result::rows_per_call(ToolId::GhSearchCode) {
        return None;
    }
    let rows = alternatives
        .into_iter()
        .map(|alternative| {
            let mut row = query_row(query, &[]);
            row["keywords"][index] = json!(alternative);
            row
        })
        .collect::<Vec<_>>();
    Some(
        Continuation::input(ToolId::GhSearchCode, json!({ "queries": rows }))
            .confidence(Confidence::Medium)
            .build(),
    )
}

/// Variants an empty search offers at most: each one is another code search
/// against a window of 10 per minute.
const BROADEN_VARIANTS: usize = 2;

/// An empty search's broader variants ([`broaden`]): one per set filter
/// (`path`, `filename`, `extensions`), then one per dropped keyword. No more
/// are offered than `window_room`, the code searches the window has left.
pub(super) fn broaden_search(query: &GhSearchCodeQuery, window_room: usize) -> Option<Value> {
    broaden(
        ToolId::GhSearchCode,
        &query_row(query, &[]),
        &Broaden {
            // Code search indexes only the default branch: a `ref` adds
            // nothing to a broader search.
            strip: &["fields", "ref"],
            groups: &[&["path"], &["filename"], &["extensions"]],
            split_lone_keyword: false,
        },
        window_room.min(BROADEN_VARIANTS),
    )
}

#[cfg(test)]
mod tests;
