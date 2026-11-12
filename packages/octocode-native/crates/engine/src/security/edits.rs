//! Which bytes of a text a redaction replaced. A redaction runs in passes, each over
//! the text the previous one left, so a later pass's positions are composed back
//! onto the input ([`Edits::then`]). A reader decides whether a cut of the input
//! shows a secret by position ([`Edits::overlaps`]), never by text: a short cut
//! whose text also occurs elsewhere is still part of the secret it was cut from.

use std::ops::Range;

/// One replacement: the input bytes it took and the output bytes it left.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct Edit {
    input: Range<usize>,
    output: Range<usize>,
}

/// The replacements that turned an input into an output, ascending and disjoint
/// on both sides.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Edits(Vec<Edit>);

impl Edits {
    /// The edits of one pass: each input range of `replaced` (ascending, disjoint)
    /// became that many output bytes.
    #[must_use]
    pub fn of_pass(replaced: impl IntoIterator<Item = (Range<usize>, usize)>) -> Self {
        let mut edits = Vec::new();
        // Where the last edit ended, in the input and in the output.
        let (mut input_at, mut output_at) = (0, 0);
        for (input, len) in replaced {
            debug_assert!(
                edits
                    .last()
                    .is_none_or(|last: &Edit| last.input.end <= input.start),
                "a pass's replacements are ascending and disjoint"
            );
            let start = output_at + (input.start - input_at);
            (input_at, output_at) = (input.end, start + len);
            edits.push(Edit {
                output: start..start + len,
                input,
            });
        }
        Self(edits)
    }

    /// No byte was replaced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The input ranges replaced, ascending and disjoint.
    pub fn redacted(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.0.iter().map(|edit| edit.input.clone())
    }

    /// Each replaced input range with what replaced it in `output`, the text
    /// these edits produced: to show a cut of the input with each run it
    /// overlaps as the redaction left it. An edit `output` cannot hold yields
    /// an empty replacement.
    pub fn runs<'a>(
        &'a self,
        output: &'a str,
    ) -> impl Iterator<Item = (Range<usize>, &'a str)> + 'a {
        self.0.iter().map(move |edit| {
            (
                edit.input.clone(),
                output.get(edit.output.clone()).unwrap_or_default(),
            )
        })
    }

    /// The output ranges the replacements left, ascending and disjoint.
    pub fn outputs(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.0.iter().map(|edit| edit.output.clone())
    }

    /// Where output byte `at` came from in the input: its own place outside
    /// every replacement, the start of what a replacement took inside one.
    #[must_use]
    pub(crate) fn input_at(&self, at: usize) -> usize {
        let after = self.0.partition_point(|edit| edit.output.start <= at);
        match after.checked_sub(1).and_then(|last| self.0.get(last)) {
            None => at,
            Some(edit) if at < edit.output.end => edit.input.start,
            Some(edit) => at - edit.output.end + edit.input.end,
        }
    }

    /// The input bytes output bytes `range` came from: a replacement the range
    /// touches counts whole.
    #[must_use]
    pub(crate) fn input_range(&self, range: &Range<usize>) -> Range<usize> {
        let after = self.0.partition_point(|edit| edit.output.start < range.end);
        let end = match after.checked_sub(1).and_then(|last| self.0.get(last)) {
            None => range.end,
            Some(edit) if range.end <= edit.output.end => edit.input.end,
            Some(edit) => range.end - edit.output.end + edit.input.end,
        };
        let start = self.input_at(range.start);
        start..end.max(start)
    }

    /// Where input byte `at` is in the output; `None` when a replacement took it.
    #[must_use]
    pub(crate) fn output_at(&self, at: usize) -> Option<usize> {
        let after = self.0.partition_point(|edit| edit.input.start <= at);
        match after.checked_sub(1).and_then(|last| self.0.get(last)) {
            None => Some(at),
            Some(edit) if at < edit.input.end => None,
            Some(edit) => Some(at - edit.input.end + edit.output.end),
        }
    }

    /// Where a range of the input that ends at `at` ends in the output: past
    /// the replacement of an input range the end lies in or closes.
    #[must_use]
    pub(crate) fn output_end(&self, at: usize) -> usize {
        let after = self.0.partition_point(|edit| edit.input.start < at);
        match after.checked_sub(1).and_then(|last| self.0.get(last)) {
            None => at,
            Some(edit) if at <= edit.input.end => edit.output.end,
            Some(edit) => at - edit.input.end + edit.output.end,
        }
    }

    /// Heap bytes these edits hold, for a cache that weighs what it keeps.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<Edit>()
    }

    /// Whether input bytes `span` overlap any replaced range. An empty span
    /// overlaps a range it lies within.
    #[must_use]
    pub fn overlaps(&self, span: &Range<usize>) -> bool {
        let first = self.0.partition_point(|edit| edit.input.end <= span.start);
        self.0.get(first).is_some_and(|edit| {
            edit.input.start < span.end.max(span.start + 1) && span.start < edit.input.end
        })
    }

    /// These edits followed by `later`, a pass over their output: the edits that
    /// turn this input into `later`'s output. A later range that touches or
    /// straddles an earlier replacement takes that replacement's whole input
    /// range with it, so composing can widen a redaction but never narrow one.
    #[must_use]
    pub fn then(&self, later: &Self) -> Self {
        if later.is_empty() {
            return self.clone();
        }
        if self.is_empty() {
            return later.clone();
        }
        // Each later edit's input lies in this output. Sweep both lists by
        // their position in this output, grouping every edit that touches the
        // group so far; a group becomes one edit.
        let mut out = Vec::with_capacity(self.0.len() + later.0.len());
        let (mut a, mut b) = (self.0.iter().peekable(), later.0.iter().peekable());
        // Where the last swept edit of each list ended: of these edits in
        // this output and the input, of `later`'s in this output and its own.
        // A position past it maps across by that edit's end.
        let (mut a_output, mut a_input) = (0, 0);
        let (mut b_input, mut b_output) = (0, 0);
        loop {
            let next_a = a.peek().map(|edit| edit.output.start);
            let next_b = b.peek().map(|edit| edit.input.start);
            let start = match (next_a, next_b) {
                (None, None) => break,
                (Some(x), None) => x,
                (None, Some(y)) => y,
                (Some(x), Some(y)) => x.min(y),
            };
            // The group: in this output, `start..end`.
            let mut end = start;
            // A group boundary off every earlier edit maps onto the input by
            // the earlier edits before it.
            let mut input_start = start - a_output + a_input;
            let output_start = start - b_input + b_output;
            let mut first = true;
            loop {
                if let Some(edit) = a.next_if(|edit| edit.output.start <= end) {
                    if first && edit.output.start == start {
                        input_start = edit.input.start;
                    }
                    end = end.max(edit.output.end);
                    (a_output, a_input) = (edit.output.end, edit.input.end);
                    first = false;
                    continue;
                }
                if let Some(edit) = b.next_if(|edit| edit.input.start <= end) {
                    end = end.max(edit.input.end);
                    (b_input, b_output) = (edit.input.end, edit.output.end);
                    first = false;
                    continue;
                }
                break;
            }
            let input_end = end - a_output + a_input;
            let output_end = end - b_input + b_output;
            out.push(Edit {
                input: input_start..input_end,
                output: output_start..output_end,
            });
        }
        Self(out)
    }

    /// Whether these are the edits that turn `input` into `output`: every byte
    /// outside them is the input's own, at its place.
    #[cfg(test)]
    pub(crate) fn maps(&self, input: &str, output: &str) -> bool {
        let (input, output) = (input.as_bytes(), output.as_bytes());
        let (mut i, mut o) = (0, 0);
        for edit in &self.0 {
            if edit.input.start < i
                || edit.output.start < o
                || edit.input.end > input.len()
                || edit.output.end > output.len()
                || input[i..edit.input.start] != output[o..edit.output.start]
            {
                return false;
            }
            (i, o) = (edit.input.end, edit.output.end);
        }
        input[i..] == output[o..]
    }
}

#[cfg(test)]
mod tests;
