//! Charge each retained match span to its search.
use super::{AtomicBool, Charge, Ordering, TextSearchMatch, match_weight};

/// Keep `span` and hold its bytes; past the search's bound raise `stop`:
/// whether the search still fits (a `find_iter` callback goes on while it
/// does).
pub(in super::super) fn keep_span(
    (om, charge, stop): (&mut Vec<TextSearchMatch>, &mut Charge, &AtomicBool),
    span: TextSearchMatch,
) -> bool {
    let fits = charge.add(match_weight(&span));
    om.push(span);
    if !fits {
        stop.store(true, Ordering::Relaxed);
    }
    fits
}
