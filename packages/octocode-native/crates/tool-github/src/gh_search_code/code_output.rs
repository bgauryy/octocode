use super::leads::{hits_read, source_read, unlisted_read};
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem, Withholds};
use octocode_github::{CodeSearchItem, ProviderError};
use octocode_runtime_base::security::scan::ContentScan;
use serde_json::{Value, json};
use std::collections::HashMap;

mod empty_scope;
mod hit_commits;
mod index_ref;
mod line_plan;

pub(super) use empty_scope::{
    empty_hints, empty_scope, owner_exists, path_mode_given_code, qualifier_keyword_hints,
    unsearched_ref,
};
pub(super) use hit_commits::{
    index_commits, line_commit, plan_lines, requested_ref, row_repo, verify_at_ref,
};
pub(super) use index_ref::{disclose_index_ref, index_head};
pub(super) use line_plan::{LinePlan, Resolution};

/// The page's required reads of unresolved source, and what its lines show.
pub(super) struct PageReads {
    pub(super) hits: Vec<Value>,
    /// Source blocks at verified commits for completely listed hits.
    pub(super) sources: Vec<octocode_contract_types::GhGetFileContentQuery>,
    /// Secret-shaped values the page's lines show as placeholders.
    pub(super) redacted: usize,
    /// Rows the path policy withholds ([`withheld_row`]): path only.
    pub(super) withheld: usize,
    /// Rows whose blob is over [`super::lines::MAX_SCAN_BYTES`]: not scanned.
    pub(super) oversized: usize,
    /// Rows whose scan panicked: a defect to report, not a missing blob.
    pub(super) panicked: usize,
    /// Required reads without an immutable index commit to verify against.
    pub(super) unanchored: usize,
    /// Path-only candidates whose lookup could not confirm ref membership.
    pub(super) unverified_at_ref: usize,
}

/// Whether source lines must be read before projecting the returned fields.
/// Content classification needs the normal hit evidence even when the host
/// requests only paths. Ordinary path projections still avoid line reads.
fn reads_lines(query: &GhSearchCodeQuery) -> bool {
    query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Content)
        && (query.fields.is_empty()
            || picks(query, "matches")
            || picks(query, "matchCount")
            || picks(query, "matchedLines"))
}

/// Whether this row's file stays unread, its lines and fragments unshown:
/// the path policy withholds it under the current settings (`withholds`).
/// The path is known before the read that would fetch or cache a copy.
fn withheld_row(withholds: Withholds<'_>, row: &Value) -> bool {
    row.get("path")
        .and_then(Value::as_str)
        .is_some_and(withholds)
}

/// A `fields` pick names `field`.
fn picks(query: &GhSearchCodeQuery, field: &str) -> bool {
    query
        .fields
        .iter()
        .any(|picked| picked.to_string() == field)
}

/// A resolved listing as the row's `matches`: each listed line in the
/// numbered-line form `"<line>\t<text>"`, as the scan holds it.
fn numbered_matches(lines: &[String]) -> Vec<Value> {
    lines.iter().cloned().map(Value::String).collect()
}

pub(super) fn shape_files(
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    resolution: Option<Resolution>,
) -> PageReads {
    let mut hit_reads = Vec::new();
    let mut sources = Vec::new();
    let mut redacted = 0;
    // A withheld row lists its path only, whether or not lines were resolved:
    // no lines, no index fragment, no read lead.
    let mut withheld = 0;
    let mut oversized = 0;
    let mut panicked = 0;
    let mut unanchored = 0;
    let mut unverified_at_ref = 0;
    let reads_lines = reads_lines(query);
    // Each row's path is judged once; shaping never changes a path.
    let unread = items
        .iter()
        .map(|row| withheld_row(withholds, row))
        .collect::<Vec<_>>();
    for (row, &unread) in items.iter_mut().zip(&unread) {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        if unread {
            withheld += 1;
            row.insert("lineResolved".into(), json!(false));
        } else if resolution.is_none() && reads_lines {
            row.insert("lineResolved".into(), json!(false));
            if let Some(read) = hits_read(query, row, "") {
                hit_reads.push(read);
                unanchored += 1;
            }
        }
    }
    if let Some(resolution) = &resolution {
        let rows = items.iter_mut().zip(&resolution.hits).zip(&unread);
        for (at, ((row, (sha, hits)), &unread)) in rows.enumerate() {
            let Some(row) = row.as_object_mut() else {
                continue;
            };
            if unread {
                row.insert("lineResolved".into(), json!(false));
                continue;
            }
            if !sha.is_empty() {
                row.insert("commitSha".into(), json!(sha));
            }
            match hits {
                super::lines::FileHits::Lines {
                    lines,
                    matched,
                    best,
                    total,
                    redacted: shown_redacted,
                    ..
                } if reads_lines => {
                    redacted += shown_redacted;
                    row.insert("matches".into(), Value::Array(numbered_matches(lines)));
                    // Only a listing with context needs to say which lines are
                    // hits; a `matchedLines` pick states them anyway, as a
                    // `matchCount` pick states the count.
                    if lines.len() > matched.len() || picks(query, "matchedLines") {
                        row.insert("matchedLines".into(), json!(matched));
                    }
                    // A cut row states how many hits its file holds, and its
                    // read lists the rest; complete lines already carry the
                    // count unless explicitly selected.
                    let unlisted = resolution.unlisted.get(at).copied().flatten();
                    if unlisted.is_some() || picks(query, "matchCount") {
                        row.insert("matchCount".into(), json!(total));
                    }
                    if let Some(read) = unlisted
                        .as_ref()
                        .and_then(|unlisted| unlisted_read(query, row, sha, unlisted))
                    {
                        hit_reads.push(read);
                    } else if unlisted.is_none()
                        && query.fields.is_empty()
                        && !lines.is_empty()
                        && let Some(read) = source_read(query, row, sha, *best)
                    {
                        sources.push(read);
                    }
                }
                // A link to a withheld file: its path only, no read lead.
                super::lines::FileHits::Withheld => {
                    withheld += 1;
                    row.insert("lineResolved".into(), json!(false));
                }
                // The path is not at the ref.
                super::lines::FileHits::Missing if resolution.reference.is_some() => {
                    row.insert("atRef".into(), json!(false));
                }
                // The file is at the ref, and no line of it holds a keyword.
                super::lines::FileHits::Unmatched
                    if resolution.reference.is_some() && reads_lines =>
                {
                    row.insert("matchCount".into(), json!(0));
                }
                super::lines::FileHits::Unavailable | super::lines::FileHits::Panicked
                    if !reads_lines =>
                {
                    unverified_at_ref += 1;
                    row.insert("lineResolved".into(), json!(false));
                }
                super::lines::FileHits::Unavailable
                | super::lines::FileHits::Oversized
                | super::lines::FileHits::Panicked => {
                    oversized += usize::from(matches!(hits, super::lines::FileHits::Oversized));
                    panicked += usize::from(matches!(hits, super::lines::FileHits::Panicked));
                    // Not read here (over the scan limit, binary, a failed
                    // read): lead to the keyword lines at the commit, on the
                    // default branch as at a ref. Without a commit, the
                    // read stays reachable and its mutable identity is disclosed.
                    row.insert("lineResolved".into(), json!(false));
                    if let Some(read) = hits_read(query, row, sha) {
                        hit_reads.push(read);
                        unanchored += usize::from(sha.is_empty());
                    }
                }
                _ if reads_lines => {
                    row.insert("lineResolved".into(), json!(false));
                }
                _ => {}
            }
        }
    }
    PageReads {
        hits: hit_reads,
        sources,
        redacted,
        withheld,
        oversized,
        panicked,
        unanchored,
        unverified_at_ref,
    }
}

/// Rows whose listed evidence (everything but `path`) is identical, such as
/// the same README in several packages, are listed once: the first keeps the
/// evidence and names every other path in `alsoAt`, in page order. Identical
/// evidence needs identical `matches`, so a row's evidence is rendered only
/// once an earlier row lists the same `matches`, and a kept row's as it was
/// first listed, before any `alsoAt`.
pub(super) fn merge_identical(items: &mut Vec<Value>) {
    // The rows that may take others' paths: each one's index in `kept`, and
    // its evidence once rendered.
    let mut takers: Vec<(usize, Option<String>)> = Vec::new();
    let mut kept: Vec<Value> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let listed = item
            .as_object()
            .is_some_and(|row| row.contains_key("matches") && row.contains_key("path"));
        if !listed {
            kept.push(item);
            continue;
        }
        let mut own = None;
        let mut taker = None;
        for (index, theirs) in &mut takers {
            if kept[*index].get("matches") != item.get("matches") {
                continue;
            }
            let theirs = theirs.get_or_insert_with(|| evidence(&kept[*index]).unwrap_or_default());
            let own = own.get_or_insert_with(|| evidence(&item));
            if own.as_deref() == Some(theirs.as_str()) {
                taker = Some(*index);
                break;
            }
        }
        match taker {
            Some(index) => {
                let row: &mut Value = &mut kept[index];
                let path = item["path"].clone();
                match row.get_mut("alsoAt").and_then(Value::as_array_mut) {
                    Some(paths) => paths.push(path),
                    None => {
                        if let Value::Object(fields) = row {
                            let mut rest = std::mem::take(fields);
                            let own = rest.shift_remove("path");
                            fields.extend(own.map(|own| ("path".to_owned(), own)));
                            fields.insert("alsoAt".into(), json!([path]));
                            fields.extend(rest);
                        }
                    }
                }
            }
            None => {
                // A row whose evidence does not render takes no other.
                if own.as_ref().is_none_or(Option::is_some) {
                    takers.push((kept.len(), own.flatten()));
                }
                kept.push(item);
            }
        }
    }
    *items = kept;
}

/// A listed row's evidence, rendered ([`Evidence`]).
fn evidence(row: &Value) -> Option<String> {
    row.as_object()
        .and_then(|row| serde_json::to_string(&Evidence(row)).ok())
}

/// A row's listed evidence: its fields but `path`, rendered exactly as the
/// row would be without that entry, so no copy of the row is made.
struct Evidence<'a>(&'a serde_json::Map<String, Value>);

impl serde::Serialize for Evidence<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().filter(|(key, _)| *key != "path"))
    }
}

pub(super) fn files(
    items: &[CodeSearchItem],
    query: &GhSearchCodeQuery,
    security: &impl ContentScan,
) -> Result<Vec<Value>, ProviderError> {
    let GhSearchCodeQuery {
        match_in, keywords, ..
    } = query;
    let path_only = !match_in.contains(&GhSearchCodeQueryMatchInItem::Content);
    let terms =
        super::ranking::terms(&keywords.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    let mut groups: Vec<super::ranking::Group> = Vec::new();
    let mut group_indices = HashMap::new();
    for item in items {
        let group_index = *group_indices
            .entry(item.repository.full_name.clone())
            .or_insert_with(|| {
                groups.push(super::ranking::Group {
                    id: item.repository.full_name.clone(),
                    matches: Vec::new(),
                });
                groups.len() - 1
            });
        let mut scores = Vec::new();
        if !path_only {
            for fragment in &item.text_matches {
                scores.extend(super::fragments::score(
                    fragment, &item.path, &terms, security,
                )?);
            }
        }
        if scores.is_empty() {
            scores.push(super::ranking::score(&item.path, "", &terms));
        }
        for score in scores {
            groups[group_index].matches.push(super::ranking::Match {
                path: item.path.clone(),
                score,
            });
        }
    }
    super::ranking::sort(&mut groups);
    let mut files: Vec<Value> = Vec::new();
    let mut listed = std::collections::HashSet::new();
    for group in groups {
        let (owner, repo) = group.id.split_once('/').unwrap_or(("", &group.id));
        for matched in group.matches {
            // One row per file, at its best-scoring match's rank.
            let key = (group.id.clone(), matched.path.clone());
            if listed.insert(key) {
                files.push(json!({"owner":owner,"repo":repo,"path":matched.path}));
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
