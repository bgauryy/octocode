use super::Window;
use octocode_io::cancellation::Cancellation;

const BUDGETS: [usize; 5] = [1, 60, 150, 400, 2_000];
const ENVELOPES: [usize; 2] = [0, 340];
const MARKERS: [usize; 2] = [0, 30];

fn window(budget: usize, envelope: usize, cancelled: &Cancellation) -> Window<'_> {
    Window {
        head: budget,
        budget,
        terminal_budget: None,
        cancelled,
        continuation_chars: 0,
        tail: 0,
        envelope,
    }
}

/// The evidence a later part holds beside its context and marker.
fn later_room(window: Window<'_>, first: usize, later: usize, marker: usize) -> usize {
    window
        .with_context(first, later, marker)
        .later_limit(marker)
        .saturating_sub(later)
}

fn cases() -> impl Iterator<Item = (usize, usize, usize, usize)> {
    BUDGETS.into_iter().flat_map(|budget| {
        ENVELOPES.into_iter().flat_map(move |envelope| {
            MARKERS.into_iter().flat_map(move |marker| {
                (0..=900)
                    .step_by(10)
                    .map(move |context| (budget, envelope, marker, context))
            })
        })
    })
}

/// Every extra part pays its page envelope, context and marker again.
/// A part whose context outgrows the page keeps evidence room of at
/// least that fixed cost, so overhead is at most half of it and a leaner
/// context cannot multiply pages that mostly pay the envelope.
#[test]
fn an_outgrown_context_keeps_evidence_room_for_its_fixed_cost() {
    let cancelled = Cancellation::new();
    for (budget, envelope, marker, context) in cases() {
        let window = window(budget, envelope, &cancelled);
        let fitted = window.with_context(context, context, marker);
        if context > budget {
            assert!(
                fitted.head - context >= context,
                "budget {budget} envelope {envelope} first {context}: head {}",
                fitted.head
            );
        }
        if context > window.later_limit(marker) {
            assert!(
                later_room(window, context, context, marker) >= context + marker + envelope,
                "budget {budget} envelope {envelope} later {context} marker {marker}"
            );
        }
    }
}

/// A later context that fits keeps the requested budget, however large
/// the first part's context is.
#[test]
fn a_fitting_later_context_keeps_the_budget() {
    let cancelled = Cancellation::new();
    for (budget, envelope, marker, later) in cases() {
        let window = window(budget, envelope, &cancelled);
        if later <= window.later_limit(marker) {
            let fitted = window.with_context(later + 10_000, later, marker);
            assert_eq!(fitted.budget, budget, "budget {budget} later {later}");
        }
    }
}
