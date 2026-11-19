//! Complete line windows for source-order evidence selection.
//! Windows retain original line anchors and never split a source line.
use crate::selection::Candidate;
use crate::source_sections::{LineEdges, line_edges};
use octocode_runtime_base::security::scan::ContentScan;
use serde_json::json;
use std::ops::Range;
use std::path::Path;

mod hits;
pub(super) use hits::{HitView, hit_chunks, project_lines};

const TARGET_CHARS: usize = 2_048;

pub(super) struct Chunk {
    bytes: Range<usize>,
    lines: Vec<usize>,
}

impl Chunk {
    pub(super) fn lines(&self) -> &[usize] {
        &self.lines
    }
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
    runtime: Option<&crate::selection::Runtime>,
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
                octocode_runtime_base::execution::RuntimeLimits::DEFAULT
                    .selection
                    .max_chars,
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
        crate::selection::needs_candidate_states(runtime, prior, unsupported, chunks.len());
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

/// A selected view's plan and the provider's failure, when it had one.
pub(crate) struct Chosen {
    pub(crate) outcome: crate::selection::Outcome,
    /// The provider's error when it could not answer for this page: the
    /// view is shown unfiltered under an `unavailable` receipt.
    provider: Option<crate::selection::ProviderError>,
}

impl Chosen {
    pub(super) fn new(
        outcome: crate::selection::Outcome,
        provider: Option<crate::selection::ProviderError>,
    ) -> Self {
        Self { outcome, provider }
    }

    /// The token of `next.nextCandidatePage`, when the budget or a provider
    /// that could not answer left units unjudged.
    pub(super) fn next_candidates(&self) -> Option<&str> {
        self.outcome.next_candidates()
    }

    /// The `selection` token replaying this page as exactly the units its
    /// walk left undecided (`next.unfilteredUnknown`), unbilled.
    pub(super) fn undecided_view(&self) -> Option<String> {
        self.outcome.undecided_view()
    }

    /// The provider's error when the view is returned unfiltered; an
    /// undecided view's own note
    /// ([`crate::selection::undecided_note`]); else,
    /// when classification left units out, what it kept
    /// ([`crate::selection::kept_note_with`]).
    ///
    /// `display_continues`: the read has more display pages, which own its
    /// progress, so `next.nextCandidatePage` is not on this row yet
    /// (`super::view::classified::attach_unfiltered`); the note then says
    /// the candidates are judged from the last page.
    pub(super) fn disclosure(&self, display_continues: bool) -> Vec<String> {
        if let Some(error) = &self.provider {
            return vec![crate::selection::unclassified_notice(
                error,
                crate::selection::Unclassified::Read,
            )];
        }
        if self.outcome.lists_undecided() {
            return vec![crate::selection::undecided_note(
                self.outcome.selected.len(),
            )];
        }
        crate::selection::kept_note_with(
            &self.outcome,
            self.outcome.selected.len(),
            !display_continues,
        )
        .into_iter()
        .chain(self.outcome.long_context_notice.clone())
        .collect()
    }

    /// Chars a classified page carries beside its text: the plan token its
    /// next page names, the receipt, the unfiltered and candidate-page
    /// leads (each a copy of `query`). The page's text leaves room for
    /// them, so the row stays within the response window it was cut for.
    pub(super) fn row_chars(&self, query: &serde_json::Value) -> usize {
        let size =
            |value: &serde_json::Value| octocode_engine::json_size::utf16_units(value).unwrap_or(0);
        // `"selection":` and the like around the values.
        const KEYS: usize = 100;
        let token = size(&json!(self.outcome.token));
        if !self.leads() {
            // No unfiltered or candidate-page lead: the view leaves nothing
            // out, or is itself the undecided list, which carries no receipt.
            let receipt = if self.outcome.lists_undecided() {
                0
            } else {
                serde_json::to_value(&self.outcome.receipt)
                    .as_ref()
                    .map_or(0, size)
            };
            return token + receipt + KEYS;
        }
        let receipt = serde_json::to_value(&self.outcome.receipt)
            .as_ref()
            .map_or(0, size);
        // Each replayed lead is a copy of the query holding its token.
        let replay = |token: &str| size(query) + size(&json!(token));
        let undecided = self.undecided_view().map_or(0, |token| replay(&token));
        let candidates = self.next_candidates().map_or(0, replay);
        let whole = crate::selection::unfiltered_row(query, &[]);
        // Typed unfiltered and replayed calls, including their saved inputs.
        let calls = 2 + usize::from(undecided > 0) + usize::from(candidates > 0);
        token + receipt + undecided + candidates + size(&whole) + KEYS * calls
    }

    /// Whether the page leads to what it left out (`next.unfiltered` and its
    /// replays): an undecided view is that list, so it leads nowhere.
    pub(super) fn leads(&self) -> bool {
        self.outcome.leaves_out() && !self.outcome.lists_undecided()
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
pub(super) fn unselected_marker(start: usize, end: usize) -> String {
    octocode_runtime_base::response::numbered::omission_marker(
        start,
        end,
        octocode_runtime_base::response::numbered::Omitted::NotSelected,
    )
}

#[cfg(test)]
mod tests;
