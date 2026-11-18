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
mod tests;
