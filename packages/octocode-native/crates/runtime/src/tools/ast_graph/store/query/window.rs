use super::*;
use crate::execution::cancel::CancellationCheck;
use std::ops::Range;

pub(super) fn check(cancel: &dyn CancellationCheck) -> Result<(), GraphOutput> {
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))
}

/// Count the entire population, projecting only the requested positions.
pub(super) struct Window<T = Value> {
    range: Range<usize>,
    page: Page<T>,
}

impl<T> Window<T> {
    pub(super) fn new(range: Range<usize>) -> Self {
        Self {
            page: Page {
                rows: Vec::new(),
                total: 0,
                start: range.start,
            },
            range,
        }
    }

    pub(super) fn try_push(
        &mut self,
        project: impl FnOnce() -> Result<T, GraphOutput>,
    ) -> Result<(), GraphOutput> {
        let position = self.page.total;
        self.page.total += 1;
        if self.range.contains(&position) {
            self.page.rows.push(project()?);
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
        start..start.saturating_add(self.options.limit.unwrap_or(DEFAULT_LIMIT))
    }
}
