//! A held window sealed: the pass's recorded redactions applied through the
//! window's runs, and its place restated in the sealed view's bytes.
use super::super::masks::{Judged, cover_every_value, with_placeholders};
use super::super::sealed::{Edit, keyed_offset, shift};
use super::Held;
use std::ops::Range;
use std::sync::Arc;

/// What a pass recorded of its source, to place sealed offsets on its bytes.
#[derive(Clone)]
pub(in crate::tools::local_fetch::large_source) struct SealMap {
    pub(in crate::tools::local_fetch::large_source) edits: Arc<[Edit]>,
    pub(in crate::tools::local_fetch::large_source) key_lines: Arc<[(Range<usize>, usize)]>,
}

impl SealMap {
    /// The keyed offset of sealed offset `sealed` of the lines from source byte
    /// `range_start` on; `None` when the pass's record breaks its order.
    pub(in crate::tools::local_fetch::large_source) fn keyed(
        &self,
        range_start: usize,
        sealed: usize,
    ) -> Option<usize> {
        keyed_offset(&self.edits, &self.key_lines, range_start, sealed)
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
    pub(super) fn seal(&mut self, edits: &[Edit], judged: &Judged) -> Result<(), String> {
        if self.unplaced {
            return Err(UNPLACED.to_owned());
        }
        let Some(range) = self.range.clone() else {
            return Ok(());
        };
        self.view_pos = shift(edits, range.clone())
            .apply(self.view_pos)
            .ok_or(UNPLACED)?;
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
                let first = edits.partition_point(|edit| edit.range.end <= source);
                for edit in edits[first..]
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
            start.base = start
                .base
                .checked_sub(lead_back)
                .and_then(|base| {
                    shift(edits, range.start..window_source.checked_sub(lead_back)?).apply(base)
                })
                .ok_or(UNPLACED)?;
            self.window = out;
        }
        if let Judged::EveryValue(named) = judged {
            // The rare fallback: no walk placed the secret values, so every
            // value this window shows is hidden, on this window alone. Its
            // view is no other window's, so no byte page continues past it.
            let cut = self.start.is_some_and(|start| start.cut);
            self.every_value = true;
            if cover_every_value(&mut self.window, cut, *named) {
                let text = String::from_utf8_lossy(&self.window);
                self.window = with_placeholders(&text).into_owned().into_bytes();
            }
        }
        Ok(())
    }
}

/// Why a window whose offsets the recorded edits cannot place is not served.
pub(in crate::tools::local_fetch::large_source) const UNPLACED: &str = "the source's recorded redactions do not fit its bytes, so this window's offsets cannot be placed; read it again";
