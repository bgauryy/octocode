//! Every hit line one streamed scan counted, kept with its pass so a later page
//! of the same read need not scan again: a page is cut from them by the very
//! steps a scan takes, so it is the page a fresh scan gives. It holds whatever
//! the scan decided, line numbers only.

use super::scan::Scan;
use crate::source_view::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;

/// How a read's context and ranges turn a hit line into its window.
pub(super) struct Shape {
    before: usize,
    after: usize,
    /// The read's ranges as first and last lines.
    regions: Vec<(usize, usize)>,
    ranged: bool,
}

impl Shape {
    pub(super) fn of(q: &LocalFetchQuery) -> Self {
        let context = local_fetch_query::context_lines(q);
        Self {
            before: octocode_engine::num::saturating_usize(context.before),
            after: octocode_engine::num::saturating_usize(context.after),
            regions: local_fetch_query::line_ranges(q)
                .iter()
                .map(|range| (range.start, range.end))
                .collect(),
            ranged: local_fetch_query::has_ranges(q),
        }
    }

    /// Hit `line`'s context window, within the read's ranges; `None` outside them.
    pub(super) fn window(&self, line: usize) -> Option<(usize, usize)> {
        let window = (
            line.saturating_sub(self.before).max(1),
            line.saturating_add(self.after),
        );
        if !self.ranged {
            return Some(window);
        }
        let region = self
            .regions
            .iter()
            .find(|range| (range.0..=range.1).contains(&line))?;
        Some((window.0.max(region.0), window.1.min(region.1)))
    }
}

/// Hit lines in order, as runs of consecutive lines. Runs that outweigh the
/// whole budget could not be kept, so they are given up rather than held.
#[derive(Default)]
pub(super) struct Runs {
    runs: Vec<(usize, usize)>,
    given_up: bool,
}

impl Runs {
    pub(super) fn push(&mut self, line: usize) {
        if self.given_up {
            return;
        }
        if let Some(run) = self.runs.last_mut()
            && run.1 + 1 == line
        {
            run.1 = line;
            return;
        }
        if Self::bytes(self.runs.len()) >= octocode_engine::memory::MEMORY.limit() {
            self.given_up = true;
            self.runs = Vec::new();
            return;
        }
        self.runs.push((line, line));
    }

    fn bytes(runs: usize) -> usize {
        runs * std::mem::size_of::<(usize, usize)>()
    }
}

/// What a page needs of a finished scan.
pub(super) struct KeptHits {
    runs: Vec<(usize, usize)>,
    /// The scan's match count ([`Scan::count`]).
    count: usize,
    total_lines: usize,
    limited: Option<usize>,
    /// [`Scan::term_hits`].
    term_hits: Vec<bool>,
    line_edges: usize,
    mid_edges: usize,
    overlap: usize,
    redacted: (usize, usize),
    /// The scan's long hit lines and the bytes the source shows
    /// ([`Scan::long`], [`Scan::shown_bytes`]).
    long: std::sync::Arc<Vec<super::walk::LongHit>>,
    shown_bytes: usize,
}

impl KeptHits {
    /// The hits of `found`, unless they were given up.
    pub(super) fn of(found: &mut Scan) -> Option<Self> {
        let runs = std::mem::take(&mut found.runs);
        (!runs.given_up).then_some(Self {
            runs: runs.runs,
            count: found.count,
            total_lines: found.total_lines,
            limited: found.limited,
            term_hits: found.term_hits.clone(),
            line_edges: found.line_edges,
            mid_edges: found.mid_edges,
            overlap: found.overlap,
            redacted: found.redacted,
            long: std::sync::Arc::clone(&found.long),
            shown_bytes: found.shown_bytes,
        })
    }

    /// About how many bytes these hits hold.
    pub(super) fn weight(&self) -> usize {
        Runs::bytes(self.runs.len())
            + super::walk::LongHits::weight_of(&self.long)
            + std::mem::size_of::<Self>()
    }

    /// The scan of `q` resumed after line `resume`, from these hits.
    pub(super) fn replay(&self, q: &LocalFetchQuery, resume: usize) -> Scan {
        let shape = Shape::of(q);
        let mut found = Scan {
            count: self.count,
            limited: self.limited,
            term_hits: self.term_hits.clone(),
            line_edges: self.line_edges,
            mid_edges: self.mid_edges,
            overlap: self.overlap,
            redacted: self.redacted,
            long: std::sync::Arc::clone(&self.long),
            shown_bytes: self.shown_bytes,
            ..Scan::default()
        };
        for line in self.runs.iter().flat_map(|run| run.0..=run.1) {
            if let Some(window) = shape.window(line) {
                found.hit(line, window, resume);
            }
        }
        found.close(self.total_lines);
        found
    }
}
