//! Reads of the lines and values a page clipped (`next.expandValues`,
//! `next.read`).
use super::types::{Group, Scope};
use crate::id::ToolId;
use serde_json::{Value, json};

/// Complete expansion calls, split only at localFetch's batch limit.
pub(super) fn read_clipped_lines(scope: &Scope, out: &mut Value, page_groups: &[Group]) {
    let limit = crate::response::result::rows_per_call(ToolId::LocalFetch);
    let rows = page_groups
        .iter()
        .filter_map(|group| read_clipped_values(scope, group))
        .flat_map(|read| read.rows)
        .collect::<Vec<_>>();
    for (index, batch) in rows.chunks(limit).enumerate() {
        let name = if index == 0 {
            "expandValues".to_owned()
        } else {
            format!("expandValues{}", index + 1)
        };
        out["calls"][name] = crate::response::result::Continuation::continued_input(
            ToolId::LocalFetch,
            json!({"queries":batch}),
        )
        .build();
    }
}

/// Lines of context around each hit in the `read` lead.
pub(super) const READ_CONTEXT: u64 = 3;
/// Hit windows one `read` lead may name.
pub(super) const READ_MAX_RANGES: usize = 5;
/// Longest match the top read shows whole.
pub(super) const READ_MAX_SPAN: u64 = 400;

/// The file a match row names.
pub(super) fn row_file(scope: &Scope, row: &Value) -> Option<std::path::PathBuf> {
    let shown = row["path"].as_str()?;
    Some(if scope.is_file {
        scope.canonical.clone()
    } else {
        scope
            .canonical
            .parent()
            .unwrap_or(&scope.canonical)
            .join(shown)
    })
}

/// Every clipped source span, grouped into valid reader requests.
pub(super) struct ClippedReads {
    pub(super) rows: Vec<Value>,
}

pub(super) fn read_clipped_values(scope: &Scope, group: &Group) -> Option<ClippedReads> {
    let file = row_file(scope, &group.row).filter(|_| !group.clipped.is_empty())?;
    let rows = crate::tools::line_spans::merge_spans(group.clipped.iter().copied())
        .chunks(crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS)
        .map(|chunk| {
            let ranges = chunk
                .iter()
                .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                .collect::<Vec<_>>();
            json!({"path": file.to_string_lossy(), "ranges": ranges})
        })
        .collect();
    Some(ClippedReads { rows })
}

/// The natural next step after a match: read the top file's hit lines in
/// context. Offered when they fit [`READ_MAX_RANGES`] windows.
pub(super) fn read_top_hits(scope: &Scope, row: &Value) -> Option<Value> {
    let file = row_file(scope, row)?;
    // A row shows a multi-line match only up to its header, so its window
    // spans the whole match; past READ_MAX_SPAN lines, its first line.
    let mut hits = vec![];
    for hit in row["matches"].as_array()? {
        let line = hit["line"].as_u64()?;
        let end = hit["endLine"]
            .as_u64()
            .filter(|end| *end >= line && end - line < READ_MAX_SPAN)
            .unwrap_or(line);
        hits.push((line, end));
    }
    let windows = crate::tools::line_spans::merge_spans(
        hits.into_iter()
            .map(|(line, end)| (line.saturating_sub(READ_CONTEXT).max(1), end + READ_CONTEXT)),
    );
    if windows.is_empty() || windows.len() > READ_MAX_RANGES {
        return None;
    }
    let ranges = windows
        .iter()
        .map(|(start, end)| json!({"startLine":start,"endLine":end}))
        .collect::<Vec<_>>();
    Some(
        crate::response::result::Continuation::related(
            ToolId::LocalFetch,
            json!({"path": file.to_string_lossy(), "ranges": ranges}),
        )
        .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_reads_keep_long_spans_and_every_batch() {
        let scope = Scope {
            canonical: std::path::PathBuf::from("/workspace/source"),
            is_file: false,
            language: None,
            inferred: None,
            extensions: None,
            parsers: Default::default(),
        };
        let count = crate::response::result::rows_per_call(ToolId::LocalFetch) + 2;
        let groups = (0..count)
            .map(|index| Group {
                row: json!({"path":format!("/workspace/source/{index}.rs")}),
                chars: 0,
                out_of_range: false,
                more_matches: false,
                truncated_captures: false,
                clipped: vec![(10, 1_000)],
                priced: None,
            })
            .collect::<Vec<_>>();
        let mut out = json!({});
        read_clipped_lines(&scope, &mut out, &groups);
        let calls = out["calls"].as_object().expect("expansion calls");
        assert_eq!(calls.len(), 2);
        let mut paths = std::collections::BTreeSet::new();
        for call in calls.values() {
            assert_eq!(call["kind"], "continue");
            assert_eq!(call["tool"], "localFetch");
            for row in call["input"]["queries"].as_array().expect("reader rows") {
                assert_eq!(row["ranges"], json!([{"startLine":10,"endLine":1_000}]));
                let _: octocode_contract_types::LocalFetchQuery =
                    serde_json::from_value(row.clone()).expect("canonical reader request");
                assert!(paths.insert(row["path"].as_str().expect("path").to_owned()));
            }
        }
        assert_eq!(paths.len(), count);
    }
}
