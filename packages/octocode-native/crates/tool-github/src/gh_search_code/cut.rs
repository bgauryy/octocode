//! A row's hit lines cut to a run of its hits, when its whole listing takes
//! more than its share of the response page ([`super::part`]). The row lists
//! the run and states its `matchCount`; its `readHits` read, a
//! ghGetFileContent match read of the line ranges the run leaves, lists
//! every other hit line and its context exactly once: there, only the hits
//! inside a range count, and each hit's context stays inside its range.
use super::lines::{FileHits, Window};
use octocode_runtime_base::response::numbered::SEPARATOR;

/// The run of a file's hits a cut row lists: hits `start..end`, indexes
/// into its hit lines in source order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cut {
    pub(super) start: usize,
    pub(super) end: usize,
}

/// The source lines a cut row leaves to its `readHits` read: `1..=before`
/// and `after..=line_count`, each set when the run leaves lines there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Unlisted {
    before: Option<u32>,
    after: Option<u32>,
    line_count: u32,
}

impl Unlisted {
    /// The widest ranges a file of `line_count` lines can leave: what a
    /// read lead costs at most, measured before the run is known.
    pub(super) fn widest(line_count: u32) -> Self {
        Self {
            before: Some(line_count),
            after: Some(line_count),
            line_count,
        }
    }

    /// The read's inclusive line ranges, in file order.
    pub(super) fn ranges(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.before
            .map(|end| (1, end))
            .into_iter()
            .chain(self.after.map(|start| (start, self.line_count)))
    }
}

impl Cut {
    /// The run of `count` hits (at least one, at most all of `matched`) a
    /// row lists: the first `count`, or, when the anchor hit `best` lies
    /// past them, `count` hits from the anchor on (the last `count` when
    /// fewer follow it), so the hit a read anchors on stays listed.
    pub(super) fn around(matched: &[u32], best: u32, count: usize) -> Self {
        let total = matched.len();
        let count = count.clamp(1, total.max(1));
        let anchor = matched.iter().position(|&hit| hit == best).unwrap_or(0);
        let start = if anchor < count {
            0
        } else {
            anchor.min(total.saturating_sub(count))
        };
        Self {
            start,
            end: (start + count).min(total),
        }
    }

    /// Whether the run holds every hit: the row is listed whole.
    pub(super) fn whole(self, matched: &[u32]) -> bool {
        self.start == 0 && self.end >= matched.len()
    }

    /// The source lines the run lists, `first..end` (exclusive end): from
    /// its first hit's window, less the lines the previous hit's window
    /// holds, up to its last hit's window end or the next hit's line,
    /// whichever comes first. A line before `first` lies in an earlier
    /// hit's window and a line from `end` on in a later hit's window, each
    /// clipped to the range left to it, so the read lists each exactly once.
    fn lines(self, matched: &[u32], window: Window, line_count: u32) -> (u32, u32) {
        let after = u32::try_from(window.after).unwrap_or(u32::MAX);
        let before = u32::try_from(window.before).unwrap_or(u32::MAX);
        let window_end = |hit: usize| matched[hit].saturating_add(after).min(line_count);
        let first = match self.start {
            0 => 1,
            start => matched[start].min(
                window_end(start - 1)
                    .saturating_add(1)
                    .max(matched[start].saturating_sub(before)),
            ),
        };
        let end = if self.end >= matched.len() {
            line_count.saturating_add(1)
        } else {
            matched[self.end].min(window_end(self.end - 1).saturating_add(1))
        };
        (first, end)
    }

    /// The lines the run leaves to the read; `None` for a whole run.
    pub(super) fn unlisted(
        self,
        matched: &[u32],
        window: Window,
        line_count: u32,
    ) -> Option<Unlisted> {
        if self.whole(matched) {
            return None;
        }
        let (first, end) = self.lines(matched, window, line_count);
        Some(Unlisted {
            before: (self.start > 0).then(|| first.saturating_sub(1)),
            after: (self.end < matched.len()).then_some(end),
            line_count,
        })
    }

    /// The indexes into `lines` (numbered `"<line>\t<text>"`, ascending) the
    /// run lists.
    pub(super) fn listed(
        self,
        lines: &[String],
        matched: &[u32],
        window: Window,
        line_count: u32,
    ) -> std::ops::Range<usize> {
        let (first, end) = self.lines(matched, window, line_count);
        let from = lines.partition_point(|line| number(line) < first);
        let to = lines.partition_point(|line| number(line) < end);
        from..to.max(from)
    }

    /// `hits` listing only this run, and the lines it leaves to the read;
    /// `None` when `hits` lists no lines or the run holds all of them.
    pub(super) fn apply(self, hits: &FileHits, window: Window) -> Option<(FileHits, Unlisted)> {
        let FileHits::Lines {
            lines,
            matched,
            best,
            total,
            line_count,
            redacted,
        } = hits
        else {
            return None;
        };
        let count = u32::try_from(*line_count).unwrap_or(u32::MAX);
        let unlisted = self.unlisted(matched, window, count)?;
        let listed = self.listed(lines, matched, window, count);
        let lines = lines[listed].to_vec();
        // The redactions the listed lines still show.
        let shown = lines
            .iter()
            .map(|line| octocode_runtime_base::security::placeholders(line))
            .sum::<usize>()
            .min(*redacted);
        Some((
            FileHits::Lines {
                lines,
                matched: matched[self.start..self.end].to_vec(),
                best: *best,
                total: *total,
                line_count: *line_count,
                redacted: shown,
            },
            unlisted,
        ))
    }
}

/// The source line a numbered entry lists.
pub(super) fn number(line: &str) -> u32 {
    line.split_once(SEPARATOR)
        .and_then(|(number, _)| number.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
