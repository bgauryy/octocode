use super::leads::{declaration_read, hits_read, line_read, read_top_match, scoped_fragment_read};
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchIn};
use crate::security::scan::ContentScan;
use octocode_github::{CodeSearchItem, ProviderError};
use serde_json::{Value, json};
use std::collections::HashMap;

mod empty_scope;
mod hit_commits;
mod index_ref;

pub(super) use empty_scope::{empty_hints, empty_scope, owner_exists, path_mode_given_code};
pub(super) use hit_commits::{
    Resolution, index_commits, line_commit, requested_ref, resolve_lines, row_repo,
};
pub(super) use index_ref::disclose_index_ref;

/// Reads a shaped page carries: the top hit's line range, and one read of
/// every keyword line for each file whose listed hits were capped or cut.
pub(super) struct PageReads {
    pub(super) top: Option<Value>,
    pub(super) hits: Vec<Value>,
}

/// Shape file rows: resolved files list numbered `lines` instead of index
/// fragments; fragment `matchIndices` are verbose (core field class); a repo-scoped
/// page names owner/repo once. The top read is a line range of the top
/// resolved hit, else `fragment_read` kept inside the verified source scope.
/// Content classification needs the normal hit evidence even when the host
/// requests only paths. Ordinary path projections still avoid line reads.
fn reads_lines(query: &GhSearchCodeQuery) -> bool {
    query.match_in == GhSearchCodeQueryMatchIn::File
        && (query.clasify_context.is_some()
            || query.fields.is_empty()
            || picks(query, "lines")
            || picks(query, "matchCount"))
}

/// A `fields` pick names `field`.
fn picks(query: &GhSearchCodeQuery, field: &str) -> bool {
    query.fields.iter().any(|picked| picked.as_str() == field)
}

pub(super) fn shape_files(
    value: &mut Value,
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    resolution: Option<Resolution>,
) -> PageReads {
    let mut hit_reads = Vec::new();
    // One read lead: the top resolved hit's lines; only without them, the
    // index fragment (computed from the top row before it is reshaped).
    let fragment = items.first().cloned();
    let mut top = None;
    if let Some(resolution) = &resolution {
        if let Some(sha) = &resolution.sha {
            value["commitSha"] = json!(sha);
        }
        for (row, (sha, hits)) in items.iter_mut().zip(&resolution.hits) {
            let Some(row) = row.as_object_mut() else {
                continue;
            };
            // Rows read at different commits each name theirs.
            if resolution.sha.is_none()
                && !sha.is_empty()
                && matches!(hits, super::lines::FileHits::Lines { .. })
            {
                row.insert("commitSha".into(), json!(sha));
            }
            match hits {
                super::lines::FileHits::Lines {
                    lines,
                    clipped,
                    first,
                    last,
                    best,
                    total,
                    line_count,
                    declaration,
                    resume_after,
                } => {
                    if !query
                        .extras
                        .contains(&octocode_contract_types::ExtraKind::Details)
                    {
                        row.shift_remove("matches");
                    }
                    row.insert("lines".into(), json!(lines));
                    // A file states its count when it has unlisted hits, or
                    // when a `fields` pick asks for it.
                    if *total > lines.len() || picks(query, "matchCount") {
                        row.insert("matchCount".into(), json!(total));
                    }
                    if (*total > lines.len() || *clipped)
                        && let Some(read) = hits_read(
                            query,
                            row,
                            sha,
                            resume_after.map(|after| (after, *line_count)),
                        )
                    {
                        hit_reads.push(read);
                    }
                    if top.is_none() {
                        top = Some(match declaration {
                            Some(head) => declaration_read(query, row, head, sha),
                            None => line_read(query, row, (*first, *last, *best), *line_count, sha),
                        });
                    }
                }
                super::lines::FileHits::Missing | super::lines::FileHits::Unmatched
                    if resolution.reference.is_some() =>
                {
                    // The path, or its hit, is not at the ref; the
                    // default-branch snippet is not this ref's content.
                    row.shift_remove("matches");
                    row.insert("atRef".into(), json!(false));
                }
                super::lines::FileHits::Unavailable if resolution.reference.is_some() => {
                    // Not read at the ref: drop the default-branch snippet and
                    // lead to the keyword lines at the ref.
                    row.shift_remove("matches");
                    row.insert("lineResolved".into(), json!(false));
                    if let Some(read) = hits_read(query, row, sha, None) {
                        hit_reads.push(read);
                    }
                }
                _ => {
                    row.insert("lineResolved".into(), json!(false));
                }
            }
        }
    }
    for row in items.iter_mut().filter_map(Value::as_object_mut) {
        if query.repo.is_some() {
            row.shift_remove("owner");
            row.shift_remove("repo");
        }
    }
    let top = top.flatten().or_else(|| {
        let read = fragment.as_ref().and_then(read_top_match);
        scoped_fragment_read(read, query, resolution.as_ref())
    });
    PageReads {
        top,
        hits: hit_reads,
    }
}

/// Fields an owner-wide page names once, on the row, when every file row
/// carries the same value: the repository and the commit its lines were
/// read at.
const HOISTED_FIELDS: [&str; 3] = ["owner", "repo", "commitSha"];

/// Move each of [`HOISTED_FIELDS`] that every file row shares to the row
/// itself (a repo-scoped page names owner/repo there already).
pub(super) fn hoist_shared(value: &mut Value, items: &mut [Value]) {
    for field in HOISTED_FIELDS {
        let mut rows = items.iter().map(|row| row.get(field));
        let Some(Some(first)) = rows.next() else {
            continue;
        };
        if value.get(field).is_some() || !rows.all(|other| other == Some(first)) {
            continue;
        }
        let first = first.clone();
        for row in items.iter_mut().filter_map(Value::as_object_mut) {
            row.shift_remove(field);
        }
        value[field] = first;
    }
}

/// Rows whose listed evidence (everything but `path`) is identical, such as
/// the same README in several packages, are listed once: the first keeps the
/// evidence and names every other path in `alsoAt`, in page order.
pub(super) fn merge_identical(items: &mut Vec<Value>) {
    let mut first_of = std::collections::HashMap::new();
    let mut kept: Vec<Value> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let evidence = item
            .as_object()
            .filter(|row| row.contains_key("lines") || row.contains_key("matches"))
            .and_then(|row| {
                let mut evidence = row.clone();
                evidence.shift_remove("path")?;
                serde_json::to_string(&evidence).ok()
            });
        let Some(evidence) = evidence else {
            kept.push(item);
            continue;
        };
        match first_of.get(&evidence) {
            Some(&index) => {
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
                first_of.insert(evidence, kept.len());
                kept.push(item);
            }
        }
    }
    *items = kept;
}

pub(super) fn files(
    items: &[CodeSearchItem],
    query: &GhSearchCodeQuery,
    security: &impl ContentScan,
) -> Result<Vec<Value>, ProviderError> {
    let GhSearchCodeQuery {
        match_in, keywords, ..
    } = query;
    let path_only = *match_in == GhSearchCodeQueryMatchIn::Path;
    let terms = super::ranking::terms(keywords)?;
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
                if let Some(value) = super::fragments::project(fragment, &item.path, security)? {
                    matches.push(value);
                }
            }
            if matches.is_empty() {
                matches.push(json!({"pathOnly":true}));
            }
        } else {
            matches.push(json!({}));
        }
        for value in matches {
            let score = super::ranking::score(
                &item.path,
                value["value"].as_str().unwrap_or_default(),
                &terms,
            );
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
                    .and_then(|v: &mut Value| v["matches"].as_array_mut())
                {
                    existing.extend(values);
                }
            } else {
                indices.insert(key, files.len());
                let mut row = json!({"owner":owner,"repo":repo,"path":matched.path});
                // Path matches carry no snippets.
                if !path_only {
                    row["matches"] = json!(values);
                }
                files.push(row);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
