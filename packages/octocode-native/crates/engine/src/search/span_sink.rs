//! Where a range scan puts the matches it finds, and how that sink stops it.
//! A caller that must bound what a scan holds (a memory reservation, a match
//! cap) owns the sink; the scan only offers each match in source order.

/// The receiver of a scan's matches, as byte ranges `[start, end)`.
pub trait SpanSink {
    /// Take the next match; `false` stops the scan, whether this match was
    /// kept or refused. The sink knows which.
    fn accept(&mut self, span: (usize, usize)) -> bool;
}

/// A sink that keeps up to `max` matches and stops the scan at the last.
pub struct CappedSpans {
    spans: Vec<(usize, usize)>,
    max: usize,
}

impl CappedSpans {
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            spans: Vec::new(),
            max,
        }
    }

    #[must_use]
    pub fn into_spans(self) -> Vec<(usize, usize)> {
        self.spans
    }
}

impl SpanSink for CappedSpans {
    fn accept(&mut self, span: (usize, usize)) -> bool {
        if self.spans.len() >= self.max {
            return false;
        }
        self.spans.push(span);
        self.spans.len() < self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capped_sink_keeps_its_max_and_stops_the_scan_there() {
        let mut sink = CappedSpans::new(2);
        assert!(sink.accept((0, 1)));
        assert!(!sink.accept((2, 3)), "the last kept match stops the scan");
        assert!(!sink.accept((4, 5)), "nothing is kept past the max");
        assert_eq!(sink.into_spans(), vec![(0, 1), (2, 3)]);
        assert!(!CappedSpans::new(0).accept((0, 1)));
    }
}
