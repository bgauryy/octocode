use super::*;
use crate::execution::cancel::CancellationCheck;
use std::ops::Range;

pub(super) fn check(cancel: &dyn CancellationCheck) -> Result<(), GraphOutput> {
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))
}

/// Count the entire population, projecting only the requested positions.
/// A budgeted window also stops projecting once its rows exceed the page
/// budget: [`Graph::fit_page`] would cut them, so they are only counted.
pub(super) struct Window<T = Value> {
    range: Range<usize>,
    page: Page<T>,
    budget: Option<usize>,
    used: usize,
}

impl<T: serde::Serialize> Window<T> {
    pub(super) fn new(range: Range<usize>) -> Self {
        Self::budgeted(range, None)
    }

    pub(super) fn budgeted(range: Range<usize>, budget: Option<usize>) -> Self {
        Self {
            page: Page {
                rows: Vec::new(),
                total: 0,
                start: range.start,
            },
            range,
            budget,
            used: 0,
        }
    }

    pub(super) fn try_push(
        &mut self,
        project: impl FnOnce() -> Result<T, GraphOutput>,
    ) -> Result<(), GraphOutput> {
        let position = self.page.total;
        self.page.total += 1;
        if self.range.contains(&position) && self.budget.is_none_or(|budget| self.used <= budget) {
            let row = project()?;
            if self.budget.is_some() {
                self.used = self
                    .used
                    .saturating_add(octocode_engine::json_size::utf16_units_or(&row, usize::MAX));
            }
            self.page.rows.push(row);
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Page<T> {
        self.page
    }
}

impl Query<'_> {
    pub(super) fn window(&self) -> Range<usize> {
        let start = self.options.offset.unwrap_or(0);
        start..start.saturating_add(self.options.limit.unwrap_or_else(default_limit))
    }
}
