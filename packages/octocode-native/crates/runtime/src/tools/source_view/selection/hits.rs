//! A matchString read's candidates: every hit with the window that informs it,
//! and the view of the windows kept.
use super::{Chunk, unselected_marker};
use crate::security::scan::ContentScan;
use crate::tools::selection::Candidate;
use serde_json::json;
use std::path::Path;

/// What a matchString read judges: each hit, its window (the requested
/// context lines, or the declaration a `block` read widened it to) informing
/// the verdict. `windows` maps a hit line to its inclusive source lines.
pub(in crate::tools::source_view) fn hit_chunks(
    text: &str,
    source_lines: &[usize],
    hits: &[usize],
    windows: impl Fn(usize) -> (usize, usize),
    path: &Path,
    provider_path: &str,
    security: &impl ContentScan,
    runtime: Option<&crate::tools::selection::Runtime>,
    prior: Option<&str>,
) -> (Vec<Candidate>, Vec<Chunk>) {
    let needs_states =
        crate::tools::selection::needs_candidate_states(runtime, prior, None, hits.len());
    // The view was scanned as a whole; a redaction that changes its geometry
    // cannot be sliced by line, so its hits stay unknown.
    let safe = needs_states
        .then(|| security.sanitize(text, path).ok().map(|(safe, _)| safe))
        .flatten();
    let records = safe.as_deref().map(|safe| {
        safe.split_inclusive('\n')
            .map(|record| record.trim_end_matches(['\n', '\r']))
            .zip(source_lines.iter().copied())
            .filter(|(_, line)| *line != super::super::extraction::OMISSION_LINE)
            .collect::<Vec<_>>()
    });
    let records = records.filter(|_| {
        safe.as_deref()
            .is_some_and(|safe| safe.split_inclusive('\n').count() == source_lines.len())
    });
    let mut candidates = Vec::with_capacity(hits.len());
    let mut chunks = Vec::with_capacity(hits.len());
    for &hit in hits {
        let (first, last) = windows(hit);
        let lines = source_lines
            .iter()
            .copied()
            .filter(|line| {
                *line != super::super::extraction::OMISSION_LINE && (first..=last).contains(line)
            })
            .collect::<Vec<_>>();
        let state = records.as_ref().and_then(|records| {
            let text = records.iter().find(|(_, line)| *line == hit)?.0;
            let window = records
                .iter()
                .filter(|(_, line)| (first..=last).contains(line))
                .map(|(text, line)| json!([line, text]))
                .collect::<Vec<_>>();
            Some(json!({"path":provider_path, "target":{"line":hit, "text":text}, "lines":window}))
        });
        candidates.push(Candidate::new(state));
        chunks.push(Chunk { bytes: 0..0, lines });
    }
    (candidates, chunks)
}

/// The view's lines that `keep` names, with one marker per gap; a kept hit's
/// window shows whole even where it covers another hit.
pub(in crate::tools::source_view) fn project_lines(
    text: &str,
    source_lines: &[usize],
    keep: &std::collections::BTreeSet<usize>,
) -> (String, Vec<usize>) {
    let mut selected = String::new();
    let mut lines = Vec::new();
    let mut last: Option<usize> = None;
    for (record, &line) in text.split_inclusive('\n').zip(source_lines) {
        if !keep.contains(&line) {
            continue;
        }
        if let Some(last) = last
            && line > last + 1
        {
            if !selected.ends_with('\n') {
                selected.push('\n');
            }
            selected.push_str(&unselected_marker(last + 1, line - 1));
            lines.push(super::super::extraction::OMISSION_LINE);
        }
        selected.push_str(record);
        lines.push(line);
        last = Some(line);
    }
    (selected, lines)
}
