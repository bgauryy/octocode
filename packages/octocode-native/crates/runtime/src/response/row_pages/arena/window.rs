use super::DependencyError;
use octocode_io::cancellation::Cancellation;

#[derive(Clone, Copy)]
pub(in crate::response::row_pages) struct Window<'a> {
    pub(in crate::response::row_pages) head: usize,
    pub(in crate::response::row_pages) budget: usize,
    /// Actual finished-page allowance; never enlarged for oversized context.
    pub(in crate::response::row_pages) terminal_budget: Option<usize>,
    pub(in crate::response::row_pages) cancelled: &'a Cancellation,
    pub(in crate::response::row_pages) continuation_chars: usize,
    /// Room the fragment that ends the row keeps for the continuations
    /// only the row's last part carries ([`super::super::PartShare`]); zero
    /// inside an item that does not end its split.
    pub(in crate::response::row_pages) tail: usize,
    /// Characters each page spends outside its rows (envelope, banner and
    /// pagination block): a fixed cost every extra part pays again.
    pub(in crate::response::row_pages) envelope: usize,
}

impl Window<'_> {
    pub(in crate::response::row_pages) fn check(self) -> Result<(), DependencyError> {
        if self.cancelled.is_cancelled() {
            Err(DependencyError::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(in crate::response::row_pages) fn with_head(self, head: usize) -> Self {
        Self { head, ..self }
    }

    /// This window for item `index` of `len`: only the item that ends the
    /// split keeps the row's tail room.
    pub(in crate::response::row_pages) fn for_item(self, index: usize, len: usize) -> Self {
        Self {
            tail: if index + 1 == len { self.tail } else { 0 },
            terminal_budget: self.terminal_budget.filter(|_| index + 1 == len),
            ..self
        }
    }

    /// The tail room item `index` of `len` keeps.
    pub(in crate::response::row_pages) fn tail_of(self, index: usize, len: usize) -> usize {
        self.for_item(index, len).tail
    }

    /// An indivisible context already exceeds the bound. Avoid multiplying
    /// that same oversized context for each tiny child record.
    pub(in crate::response::row_pages) fn with_continuation(self, chars: usize) -> Self {
        Self {
            continuation_chars: self.continuation_chars.max(chars),
            ..self
        }
    }

    pub(in crate::response::row_pages) fn later_limit(self, marker: usize) -> usize {
        self.budget
            .saturating_sub(self.continuation_chars.max(marker))
            .max(1)
    }

    /// Fit a split's contexts: `first`, the whole context its first part
    /// carries, and `later`, the leaner one each later part repeats with its
    /// continuation `marker`. A context that fits the page keeps the
    /// requested budget, so first-only context never enlarges later parts.
    /// A context that cannot fit makes its part outgrow the page anyway; that
    /// part then carries evidence of at least its fixed cost. The first part
    /// pays only its context: any response pays one page. Each later part is
    /// an extra page and pays its context, its marker and the page envelope
    /// again. Overhead stays at most half of each enlarged page, so a walk of
    /// an oversized context costs about twice its evidence however lean the
    /// later parts are, instead of one envelope per item.
    pub(in crate::response::row_pages) fn with_context(
        self,
        first: usize,
        later: usize,
        marker: usize,
    ) -> Self {
        let marker = self.continuation_chars.max(marker);
        // An extra part carrying `context`, with evidence equal to its fixed cost.
        let balanced = |context: usize| context.saturating_mul(2).saturating_add(self.envelope);
        Self {
            head: if first > self.budget {
                self.head.max(first.saturating_mul(2))
            } else {
                self.head
            },
            budget: if later > self.later_limit(marker) {
                self.budget.max(balanced(later.saturating_add(marker)))
            } else {
                self.budget
            },
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
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
}
