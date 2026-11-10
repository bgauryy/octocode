//! A held window sealed: the pass's recorded redactions applied through the
//! window's runs, and its place restated in the sealed view's bytes.
use super::super::edit_map::EditMap;
use super::super::error::ReadError;
use super::super::masks::Judged;
use super::Held;
use std::ops::Range;
use std::sync::Arc;

/// What a pass recorded of its source, to place sealed offsets on its bytes.
#[derive(Clone)]
pub(in crate::tools::local_fetch::large_source) struct SealMap {
    pub(in crate::tools::local_fetch::large_source) edits: EditMap,
    pub(in crate::tools::local_fetch::large_source) key_lines: Arc<[(Range<usize>, usize)]>,
    /// The source, which a block-kept map reads its edits from.
    pub(in crate::tools::local_fetch::large_source) file: Arc<std::fs::File>,
}

impl SealMap {
    /// The keyed offset of sealed offset `sealed` of the lines from source byte
    /// `range_start` on; `None` when the pass's record breaks its order.
    pub(in crate::tools::local_fetch::large_source) fn keyed(
        &self,
        range_start: usize,
        sealed: usize,
    ) -> Result<Option<usize>, ReadError> {
        self.edits
            .keyed(&self.file, &self.key_lines, range_start, sealed)
    }
}

/// Placeholders one edit shows.
fn placeholders(text: &str) -> usize {
    octocode_engine::security::placeholder_regex()
        .find_iter(text)
        .count()
        .max(1)
}

impl Held {
    /// Show `edits` in place of the bytes they hide, and hide what `judged`
    /// leaves to every window. An edit the window's end cuts shows whole, so no
    /// window ends inside a redaction. A sealed offset the edits cannot place
    /// fails the read: its pages would gap or repeat.
    pub(super) fn seal(
        &mut self,
        edits: &EditMap,
        file: &std::fs::File,
        judged: &Judged,
    ) -> Result<(), ReadError> {
        if let Some(error) = self.failed.take() {
            return Err(error);
        }
        if self.unplaced {
            return Err(UNPLACED.into());
        }
        let Some(range) = self.range.clone() else {
            return Ok(());
        };
        self.view_pos = edits
            .shift(file, range.clone())?
            .apply(self.view_pos)
            .ok_or(UNPLACED)?;
        let mut view = edits.view(file);
        if let (Some(start), Some(window_source)) = (self.start.as_mut(), self.window_source) {
            let len = self.window.len();
            let mut out = Vec::with_capacity(len);
            let (mut lead_back, mut line, mut counted) = (0, 1, 0);
            for (index, run) in self.runs.iter().enumerate() {
                let end = self
                    .runs
                    .get(index + 1)
                    .map_or(len, |next| next.at)
                    .min(len);
                if run.at >= end {
                    continue;
                }
                let bytes = &self.window[run.at..end];
                let Some(source) = run.source else {
                    out.extend_from_slice(bytes);
                    continue;
                };
                let to = source + bytes.len();
                let mut pos = source;
                for edit in view
                    .over(source..to)?
                    .iter()
                    .take_while(|edit| edit.range.start < to)
                {
                    if edit.range.start < pos {
                        if !out.is_empty() {
                            // Shown already, before this run.
                            pos = edit.range.end.min(to);
                            continue;
                        }
                        // The window starts inside it: it shows from its start.
                        lead_back = pos - edit.range.start;
                    } else {
                        out.extend_from_slice(&bytes[pos - source..edit.range.start - source]);
                    }
                    line += out[counted..].iter().filter(|byte| **byte == b'\n').count();
                    counted = out.len();
                    let shown = placeholders(&edit.text);
                    match self.redacted.last_mut() {
                        Some((at, count)) if *at == line => *count += shown,
                        _ => self.redacted.push((line, shown)),
                    }
                    out.extend_from_slice(edit.text.as_bytes());
                    pos = edit.range.end.min(to);
                }
                out.extend_from_slice(&bytes[pos - source..]);
            }
            let before = window_source.checked_sub(lead_back).ok_or(UNPLACED)?;
            let skipped = edits.shift(file, range.start..before)?;
            start.base = start
                .base
                .checked_sub(lead_back)
                .and_then(|base| skipped.apply(base))
                .ok_or(UNPLACED)?;
            self.window = out;
        }
        // Hidden values are edits of the one view; the page still says why every
        // value of the source is hidden, secret or not.
        self.every_value = matches!(judged, Judged::EveryValue);
        Ok(())
    }
}

/// Why a window whose offsets the recorded edits cannot place is not served.
pub(in crate::tools::local_fetch::large_source) const UNPLACED: &str = "the source's recorded redactions do not fit its bytes, so this window's offsets cannot be placed; read it again";
