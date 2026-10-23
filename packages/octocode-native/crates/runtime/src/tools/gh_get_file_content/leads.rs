//! The runnable next steps of a GitHub file read: its own pages rewritten
//! from the shared localFetch read, and the leads of a match that found
//! nothing.
use crate::response::result::Confidence;
use crate::tools::source_view::types::local_fetch_query;
use serde_json::{Value, json};

use super::GhGetFileContentQuery;
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use crate::tools::source_view::LocalFetchQuery;

/// The runnable next step of a match that found nothing: a ghSearchCode of
/// the repository for the literal (it may live in another file). A regex
/// has no literal to search.
pub(super) fn empty_match_lead(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
) -> Option<(&'static str, Value)> {
    let literals = local_fetch_query::match_strings(local);
    let first = literals.first()?.trim();
    if local_fetch_query::is_regex(local) || first.is_empty() {
        return None;
    }
    let mut row =
        json!({"owner": query.owner.as_str(), "repo": query.repo.as_str(), "keywords": [first]});
    // The search reads its hits at the read's ref, not the default branch.
    if let Some(reference) = query
        .ref_
        .as_deref()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        row["ref"] = json!(reference);
    }
    Some((
        "findCode",
        Continuation::new(ToolId::GhSearchCode, row)
            .confidence(Confidence::Medium)
            .build(),
    ))
}

/// The runnable repair of a `ranges` read whose every range starts past the
/// file end: the same read of the file's last window, as wide as the widest
/// requested range, at the read commit. `None` when any range reaches into
/// the file (that read is served, clamped) or the file has no line.
pub(super) fn last_window_repair(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    total_lines: usize,
    sha: &str,
) -> Option<Value> {
    let (start, end) = local_fetch_query::last_window_span(local, total_lines)?;
    let mut row = query_row(query, &[]);
    row["ref"] = json!(sha);
    row["ranges"] = json!([format!("{start}-{end}")]);
    Some(
        Continuation::new(ToolId::GhGetFileContent, row)
            .confidence(Confidence::High)
            .build(),
    )
}

/// One localFetch continuation row as the same read of the GitHub file at
/// `resolved_ref`, in the view the shared read chose for it (the caller's,
/// or none for a lead that reads the text as is). A window (offset, length or unit)
/// never reads the whole file; a whole-view read (the unfiltered view of a
/// `fullContent` read) keeps `fullContent`.
fn github_row(
    mut query: serde_json::Map<String, Value>,
    source: &GhGetFileContentQuery,
    resolved_ref: &str,
) -> Value {
    query.insert("owner".into(), Value::String(source.owner.to_string()));
    query.insert("repo".into(), Value::String(source.repo.to_string()));
    query.insert("ref".into(), Value::String(resolved_ref.to_owned()));
    query.remove("snapshot");
    if ["offset", "length", "unit"]
        .iter()
        .any(|field| query.contains_key(*field))
    {
        query.remove("fullContent");
    }
    if let Some(force) = source.force_refresh {
        query.insert("forceRefresh".into(), Value::Bool(force));
    }
    Value::Object(query)
}

pub(super) fn rewrite_continuations(
    result: &mut crate::tools::source_view::LocalFetchResult,
    source: &GhGetFileContentQuery,
    resolved_ref: &str,
) -> Option<Value> {
    let mut value = serde_json::to_value(result.next.take()?).ok()?;
    let Value::Object(object) = &mut value else {
        return None;
    };
    // Each localFetch call becomes the same read of the GitHub file. A lead
    // to another local tool (the miss's `findText` localSearch of the
    // file's directory) has no GitHub form here and is dropped; the GitHub
    // miss adds its own repository-level lead.
    object.retain(|_, continuation| continuation["tool"] == ToolId::LocalFetch.as_str());
    for continuation in object.values_mut() {
        let Some(rows) = continuation
            .pointer("/query/queries")
            .and_then(Value::as_array)
        else {
            continue;
        };
        let rows = rows
            .iter()
            .filter_map(Value::as_object)
            .map(|row| github_row(row.clone(), source, resolved_ref))
            .collect::<Vec<_>>();
        let mut call = Continuation::input(ToolId::GhGetFileContent, json!({"queries": rows}));
        if let Some(confidence) = continuation["confidence"]
            .as_str()
            .and_then(Confidence::from_wire)
        {
            call = call.confidence(confidence);
        }
        *continuation = call.build();
    }
    (!object.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::source_view::{FetchCall, LocalFetchResult, NextCalls, ReadStatus};

    fn read(fields: Value) -> FetchCall {
        let mut row = json!({"path":"README.md"});
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        FetchCall {
            query: serde_json::from_value(row).expect("read"),
        }
    }

    /// The whole-view recovery of a `fullContent` read stays a whole read
    /// on GitHub; a window (offset/length) never reads the whole file, and
    /// every row of a multi-row call becomes a GitHub read.
    #[test]
    fn a_whole_view_lead_keeps_full_content_and_every_row_is_rewritten() {
        let source: GhGetFileContentQuery =
            serde_json::from_value(json!({"owner":"o","repo":"r","path":"README.md"}))
                .expect("query");
        let mut rest = serde_json::Map::new();
        rest.insert(
            "unfilteredUnknown2".into(),
            Continuation::input(
                ToolId::LocalFetch,
                json!({"queries":[{"path":"README.md","ranges":["1-2"]},
                    {"path":"README.md","ranges":["9-9"]}]}),
            )
            .build(),
        );
        let mut result = LocalFetchResult::blank(ReadStatus::Success);
        result.next = Some(NextCalls {
            unfiltered: Some(read(json!({"fullContent":true}))),
            unfiltered_unknown: Some(read(json!({"ranges":["3-4"]}))),
            continue_window: Some(read(json!({"offset":50,"length":50,"unit":"lines"}))),
            unfiltered_unknown_rest: rest,
            ..NextCalls::default()
        });
        let next = rewrite_continuations(&mut result, &source, "abc123").expect("next");
        let whole = &next["unfiltered"];
        assert_eq!(whole["tool"], "ghGetFileContent");
        assert_eq!(whole["query"]["queries"][0]["fullContent"], true, "{next}");
        let window = &next["continueWindow"]["query"]["queries"][0];
        assert!(window.get("fullContent").is_none() && window["offset"] == 50);
        let rows = next["unfilteredUnknown2"]["query"]["queries"]
            .as_array()
            .expect("rows");
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| row["owner"] == "o" && row["ref"] == "abc123")
        );
    }
}
