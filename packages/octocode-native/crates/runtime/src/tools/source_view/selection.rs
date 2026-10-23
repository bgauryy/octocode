//! Complete line windows for source-order evidence selection.
//! Windows retain original line anchors and never split a source line.
use crate::security::scan::ContentScan;
use crate::tools::selection::Candidate;
use crate::tools::source_sections::{LineEdges, line_edges};
use serde_json::json;
use std::ops::Range;
use std::path::Path;

const TARGET_CHARS: usize = 2_048;

pub(super) struct Chunk {
    bytes: Range<usize>,
    lines: Vec<usize>,
}

/// The candidate chunks of view `text`. `source_lines` maps each view line
/// to its source line when the view was extracted from `source` (ranges,
/// matches); chunk edges follow the source's declarations either way.
pub(super) fn chunks(
    text: &str,
    source_lines: Option<&[usize]>,
    source: Option<&str>,
    path: &Path,
    provider_path: &str,
    security: &impl ContentScan,
    runtime: Option<&crate::tools::selection::Runtime>,
    prior: Option<&str>,
    unsupported: Option<&'static str>,
) -> (Vec<Candidate>, Vec<Chunk>) {
    if let Some(source_lines) = source_lines
        && source_lines.len() != text.split_inclusive('\n').count()
    {
        return (
            vec![Candidate::new(None)],
            vec![Chunk {
                bytes: 0..text.len(),
                lines: source_lines.to_vec(),
            }],
        );
    }
    // A candidate keeps a bounded declaration and its documentation whole.
    // Oversized containers leave their nested declarations available; other
    // source keeps complete line windows, which end where a declaration
    // starts or ends once half full.
    let outline = if source_lines.is_none() {
        Some(text)
    } else {
        source
    };
    let edges = outline.map_or_else(LineEdges::default, |outline| {
        line_edges(
            outline,
            path,
            runtime.map_or(
                crate::execution::RuntimeLimits::DEFAULT.selection.max_chars,
                |runtime| runtime.limits.max_chars,
            ),
        )
    });
    let declarations = &edges.windows;
    let mut declaration_index = 0;
    let mut previous_declaration = None;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut count = 0usize;
    let mut offset = 0;
    let mut lines = Vec::new();
    let mut last_source_line = None;
    for (index, record) in text.split_inclusive('\n').enumerate() {
        let chars = record.chars().count();
        let line = source_lines.map_or(index + 1, |map| map[index]);
        let contiguous =
            last_source_line.is_none_or(|last: usize| last.checked_add(1) == Some(line));
        while declarations
            .get(declaration_index)
            .is_some_and(|range| range.end <= line)
        {
            declaration_index += 1;
        }
        let declaration = declarations
            .get(declaration_index)
            .filter(|range| range.contains(&line))
            .map(|_| declaration_index);
        let declaration_break = declaration != previous_declaration;
        let outline_edge = declaration.is_none()
            && count >= TARGET_CHARS / 2
            && (edges.starts.contains(&line)
                || last_source_line.is_some_and(|last| edges.ends.contains(&last)));
        if last_source_line.is_some()
            && (line == super::extraction::OMISSION_LINE
                || !contiguous
                || declaration_break
                || outline_edge
                || (declaration.is_none() && count.saturating_add(chars) > TARGET_CHARS))
        {
            chunks.push(Chunk {
                bytes: start..offset,
                lines: std::mem::take(&mut lines),
            });
            start = offset;
            count = 0;
            last_source_line = None;
        }
        count += chars;
        lines.push(line);
        if line != super::extraction::OMISSION_LINE {
            last_source_line = Some(line);
        }
        offset += record.len();
        previous_declaration = declaration;
    }
    if last_source_line.is_some() {
        chunks.push(Chunk {
            bytes: start..text.len(),
            lines,
        });
    } else if let Some(last) = chunks.last_mut() {
        last.bytes.end = text.len();
        last.lines.extend(lines);
    }
    let needs_states =
        crate::tools::selection::needs_candidate_states(runtime, prior, unsupported, chunks.len());
    let candidates = chunks
        .iter()
        .map(|chunk| {
            let record = &text[chunk.bytes.clone()];
            let state = if !needs_states {
                None
            } else {
                security.sanitize(record, path).ok().map(|(safe, _)| {
                    json!({
                        "path": provider_path,
                        "sourceLines": chunk.lines,
                        "content": safe,
                    })
                })
            };
            Candidate::new(state)
        })
        .collect();
    (candidates, chunks)
}

/// A selected view's plan and the source lines of its unknown chunks.
pub(crate) struct Chosen {
    pub(crate) outcome: crate::tools::selection::Outcome,
    /// `ranges` entries (`"a-b"`, merged) covering every unknown chunk.
    unknown_ranges: Vec<String>,
    /// The ranges read fewer source lines than the whole view holds.
    narrower: bool,
    /// The provider's error when it could not answer for this page: the
    /// view is shown unfiltered under an `unavailable` receipt.
    provider: Option<crate::tools::selection::ProviderError>,
}

impl Chosen {
    pub(super) fn new(
        outcome: crate::tools::selection::Outcome,
        provider: Option<crate::tools::selection::ProviderError>,
        chunks: &[Chunk],
    ) -> Self {
        let judged = outcome
            .selected
            .iter()
            .chain(&outcome.related)
            .chain(&outcome.excluded)
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let mut view_lines = std::collections::BTreeSet::new();
        let mut unknown = std::collections::BTreeSet::new();
        for (ordinal, chunk) in chunks.iter().enumerate() {
            let lines = chunk
                .lines
                .iter()
                .copied()
                .filter(|&line| line != super::extraction::OMISSION_LINE);
            if !outcome.considers(ordinal) || judged.contains(&ordinal) {
                view_lines.extend(lines);
            } else {
                for line in lines {
                    view_lines.insert(line);
                    unknown.insert(line);
                }
            }
        }
        let unknown_lines = unknown.len();
        Self {
            outcome,
            unknown_ranges: crate::tools::line_spans::runs(unknown)
                .into_iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect(),
            narrower: unknown_lines < view_lines.len(),
            provider,
        }
    }

    /// The token of `next.nextCandidatePage`, when the budget left chunks
    /// unjudged.
    pub(super) fn next_candidates(&self) -> Option<&str> {
        self.outcome.next_candidates(crate::id::ToolId::LocalFetch)
    }

    /// The provider's error when the view is returned unfiltered.
    pub(super) fn disclosure(&self) -> Option<String> {
        let error = self.provider.as_ref()?;
        Some(crate::tools::selection::unclassified_warning(
            error,
            "this read",
            "its content is",
        ))
    }

    /// Exactly the unknown chunks as plain `ranges` reads of `query`'s file,
    /// each row as many ranges as a row takes, when some chunks were judged
    /// and the ranges read less than the whole view. Empty otherwise.
    pub(super) fn unknown_reads(&self, query: &serde_json::Value) -> Vec<serde_json::Value> {
        let receipt = &self.outcome.receipt;
        let cap = crate::contracts::query_schema_number(
            crate::id::ToolId::LocalFetch,
            None,
            "ranges",
            "maxItems",
        )
        .and_then(|n| usize::try_from(n).ok())
        .filter(|cap| *cap > 0);
        let Some(cap) = cap else {
            return Vec::new();
        };
        if self.unknown_ranges.is_empty()
            || !self.narrower
            || receipt.unknown_items >= receipt.total_items
        {
            return Vec::new();
        }
        let mut row = crate::tools::selection::unfiltered_row(query, &[]);
        let Some(fields) = row.as_object_mut() else {
            return Vec::new();
        };
        // The unknown chunks are source lines: read them as lines, not as
        // the selection (hits, context, declarations, whole file) they came from.
        for field in [
            "matchString",
            "contextLines",
            "regex",
            "caseMode",
            "block",
            "fullContent",
        ] {
            fields.remove(field);
        }
        self.unknown_ranges
            .chunks(cap)
            .map(|ranges| {
                let mut read = row.clone();
                read["ranges"] = json!(ranges);
                read
            })
            .collect()
    }

    /// Chars a classified page carries beside its text: the plan token its
    /// next page names, the receipt, the unfiltered and candidate-page
    /// leads (each a copy of `query`). The page's text leaves room for
    /// them, so the row stays within the response window it was cut for.
    pub(super) fn row_chars(&self, query: &serde_json::Value) -> usize {
        let receipt = serde_json::to_value(&self.outcome.receipt)
            .as_ref()
            .map_or(0, |value| {
                octocode_engine::json_size::utf16_units(value).unwrap_or(0)
            });
        // `"selection":` and the like around the values.
        const KEYS: usize = 100;
        if !self.outcome.leaves_out() {
            // No unfiltered or candidate-page lead: the view leaves nothing out.
            return octocode_engine::json_size::utf16_units(&json!(self.outcome.token))
                .unwrap_or(0)
                + receipt
                + KEYS;
        }
        let whole = crate::tools::selection::unfiltered_row(query, &[]);
        let unknown = self
            .unknown_reads(query)
            .iter()
            .map(|value| octocode_engine::json_size::utf16_units(value).unwrap_or(0))
            .sum::<usize>();
        let candidates = self.next_candidates().map_or(0, |token| {
            octocode_engine::json_size::utf16_units(query).unwrap_or(0)
                + octocode_engine::json_size::utf16_units(&json!(token)).unwrap_or(0)
        });
        // `"unfiltered":{"tool":…,"query":{"queries":[…]}}` and the like.
        let calls = 2 + usize::from(candidates > 0) + self.unknown_reads(query).len();
        octocode_engine::json_size::utf16_units(&json!(self.outcome.token)).unwrap_or(0)
            + receipt
            + unknown
            + candidates
            + octocode_engine::json_size::utf16_units(&whole).unwrap_or(0)
            + KEYS * calls
    }
}

/// The selected chunks in `order` as one view, and the source line of each
/// view line. Selected chunks that are not adjacent in the source are
/// separated by one marker naming the lines between them (mapped to
/// [`super::extraction::OMISSION_LINE`]), as disjoint ranges are; a
/// chunk's own leading marker described a gap beside a chunk not shown, so
/// it gives way to that one.
pub(super) fn project(text: &str, chunks: &[Chunk], order: &[usize]) -> (String, Vec<usize>) {
    use super::extraction::OMISSION_LINE;
    let mut selected = String::with_capacity(text.len());
    let mut lines = Vec::new();
    let mut last: Option<usize> = None;
    let mut previous: Option<usize> = None;
    for &ordinal in order {
        let chunk = &chunks[ordinal];
        let body = &text[chunk.bytes.clone()];
        let mut records = body.split_inclusive('\n').zip(&chunk.lines).peekable();
        // A leading marker names the gap after the chunk before this one:
        // it stays when that chunk is shown too.
        if ordinal > 0 && previous != Some(ordinal - 1) {
            while records
                .next_if(|(_, line)| **line == OMISSION_LINE)
                .is_some()
            {}
        }
        previous = Some(ordinal);
        let first = records.peek().map(|(_, line)| **line);
        let kept_marker = first == Some(OMISSION_LINE);
        let first = first.filter(|_| !kept_marker);
        if let (Some(last), Some(first)) = (last, first)
            && first > last + 1
        {
            if !selected.is_empty() && !selected.ends_with('\n') {
                selected.push('\n');
            }
            selected.push_str(&unselected_marker(last + 1, first - 1));
            lines.push(OMISSION_LINE);
        }
        for (record, &line) in records {
            selected.push_str(record);
            lines.push(line);
            if line != OMISSION_LINE {
                last = Some(line);
            }
        }
    }
    (selected, lines)
}

/// Separates selected chunks that are not adjacent in the source.
fn unselected_marker(start: usize, end: usize) -> String {
    crate::response::numbered::omission_marker(
        start,
        end,
        crate::response::numbered::Omitted::NotSelected,
    )
}

#[cfg(test)]
mod tests;
