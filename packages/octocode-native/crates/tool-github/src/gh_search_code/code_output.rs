use super::leads::{declaration_read, hits_read, line_read, scoped_fragment_read, unlisted_read};
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
    empty_hints, empty_scope, owner_exists, path_mode_given_code, unsearched_ref,
};
pub(super) use hit_commits::{index_commits, line_commit, plan_lines, requested_ref, row_repo};
pub(super) use index_ref::{disclose_index_ref, index_head};
pub(super) use line_plan::{LinePlan, Resolution};

/// Optional top-hit exploration and required reads of unresolved source.
pub(super) struct PageReads {
    pub(super) top: Option<Value>,
    pub(super) hits: Vec<Value>,
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
}

/// Whether source lines must be read before projecting the returned fields.
/// Content classification needs the normal hit evidence even when the host
/// requests only paths. Ordinary path projections still avoid line reads.
fn reads_lines(query: &GhSearchCodeQuery) -> bool {
    query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Content)
        && (query.classification.is_some()
            || query.fields.is_empty()
            || picks(query, "matches")
            || picks(query, "matchCount"))
}

/// Whether this row's file stays unread, its lines and fragments unshown:
/// the path policy withholds it under the current settings (`withholds`), or,
/// on a classified page, the egress withholds its name whatever the setting,
/// so nothing of it may become candidate evidence. The path is known before
/// the read that would fetch or cache a copy.
fn withheld_row(query: &GhSearchCodeQuery, withholds: Withholds<'_>, row: &Value) -> bool {
    row.get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| withheld_path(query, withholds, path))
}

/// [`withheld_row`] for one path: a row's own, or the target of a symlink it
/// reads through.
pub(super) fn withheld_path(
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    path: &str,
) -> bool {
    if query.classification.is_some() || query.selection.is_some() {
        octocode_runtime_base::policy::discovery::is_sensitive_repository_file(path)
    } else {
        withholds(path)
    }
}

/// A `fields` pick names `field`.
fn picks(query: &GhSearchCodeQuery, field: &str) -> bool {
    query.fields.iter().any(|picked| picked.as_str() == field)
}

/// The provider's index fragments of a row: they rank rows and feed the
/// fallback read lead, and [`shape_files`] removes them before any row is
/// listed.
pub(super) const FRAGMENTS: &str = "fragments";

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
    shape_rows(items, query, withholds, resolution, true)
}

/// [`shape_files`], with the page's top read lead only when `top_lead`: a
/// row shaped alone to be measured ([`super::part`]) needs it only as the
/// page's first row, and building a lead is most of a row's shaping.
pub(super) fn shape_rows(
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    resolution: Option<Resolution>,
    top_lead: bool,
) -> PageReads {
    let mut hit_reads = Vec::new();
    let mut redacted = 0;
    // A withheld row lists its path only, whether or not lines were resolved:
    // no lines, no index fragment, no read lead.
    let mut withheld = 0;
    let mut oversized = 0;
    let mut panicked = 0;
    let mut unanchored = 0;
    // Each row's path is judged once; shaping never changes a path.
    let unread = items
        .iter()
        .map(|row| withheld_row(query, withholds, row))
        .collect::<Vec<_>>();
    for (row, &unread) in items.iter_mut().zip(&unread) {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        if unread {
            withheld += 1;
            row.insert("lineResolved".into(), json!(false));
        } else if resolution.is_none() && reads_lines(query) {
            row.insert("lineResolved".into(), json!(false));
            if let Some(read) = hits_read(query, row, "") {
                hit_reads.push(read);
                unanchored += 1;
            }
        }
    }
    // One read lead: the top resolved hit's lines; only without them, the
    // index fragment (computed from the top row before it is reshaped), never
    // a withheld file's.
    let fragment = items
        .first()
        .filter(|_| top_lead && unread.first() == Some(&false))
        .cloned();
    let mut top = None;
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
                    first,
                    last,
                    best,
                    total,
                    line_count,
                    declaration,
                    redacted: shown_redacted,
                } => {
                    redacted += shown_redacted;
                    row.insert("matches".into(), Value::Array(numbered_matches(lines)));
                    // Only a listing with context needs to say which lines are hits.
                    if lines.len() > matched.len() {
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
                    }
                    if top_lead && top.is_none() {
                        top = Some(match declaration {
                            Some(head) => declaration_read(query, row, head, sha),
                            None => line_read(query, row, (*first, *last, *best), *line_count, sha),
                        });
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
                super::lines::FileHits::Unmatched if resolution.reference.is_some() => {
                    row.insert("matchCount".into(), json!(0));
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
                _ => {
                    row.insert("lineResolved".into(), json!(false));
                }
            }
        }
    }
    // The index fragments only feed the fallback lead below; no row lists them.
    for row in items.iter_mut().filter_map(Value::as_object_mut) {
        row.shift_remove(FRAGMENTS);
    }
    let top = top.flatten().or_else(|| {
        top_lead
            .then(|| scoped_fragment_read(fragment.as_ref(), query, resolution.as_ref()))
            .flatten()
    });
    PageReads {
        top,
        hits: hit_reads,
        redacted,
        withheld,
        oversized,
        panicked,
        unanchored,
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
        let mut matches = Vec::new();
        if !path_only {
            for fragment in &item.text_matches {
                if let Some(projected) =
                    super::fragments::project(fragment, &item.path, &terms, security)?
                {
                    matches.push(projected);
                }
            }
            if matches.is_empty() {
                matches.push((
                    json!({"pathOnly":true}),
                    super::ranking::score(&item.path, "", &terms),
                ));
            }
        } else {
            matches.push((json!({}), super::ranking::score(&item.path, "", &terms)));
        }
        for (value, score) in matches {
            groups[group_index].matches.push(super::ranking::Match {
                path: item.path.clone(),
                value,
                score,
            });
        }
    }
    super::ranking::sort(&mut groups);
    let mut files: Vec<Value> = Vec::new();
    let mut indices = HashMap::new();
    for group in groups {
        let (owner, repo) = group.id.split_once('/').unwrap_or(("", &group.id));
        for matched in group.matches {
            let key = (group.id.clone(), matched.path.clone());
            let values = if path_only {
                Vec::new()
            } else {
                vec![matched.value]
            };
            if let Some(&index) = indices.get(&key) {
                if let Some(existing) = files
                    .get_mut(index)
                    .and_then(|v: &mut Value| v[FRAGMENTS].as_array_mut())
                {
                    existing.extend(values);
                }
            } else {
                indices.insert(key, files.len());
                let mut row = json!({"owner":owner,"repo":repo,"path":matched.path});
                // Path matches carry no snippets.
                if !path_only {
                    row[FRAGMENTS] = json!(values);
                }
                files.push(row);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
