//! Cursor publication binds saved work to the access identity used by the call.
use super::ChannelRun;
use crate::runtime::engine::cursors::{Pending, SavedWork};
use crate::runtime::row_indices::RowOrigin;
use octocode_runtime_base::contracts;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::response::{
    continuations, cursor::Cursor, outcome::ToolOutcome, read_share,
};
use octocode_runtime_base::security::Sanitized;
use serde_json::{Value, json};

impl ChannelRun {
    pub(super) fn pending(&self) -> Pending {
        self.cursors.pending(self.access, None)
    }

    pub(super) fn retain_cursors(
        &self,
        pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if !self.can_resume && !pending.is_empty() {
            return Err(ExecutionError::ContinuationLifetime);
        }
        self.cursors.retain(pending, context)
    }

    pub(super) fn retain_pending(
        &self,
        mut pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if pending.is_empty() {
            return Ok(());
        }
        let identity = self
            .dispatcher
            .github_access
            .identity()
            .map_err(|()| ExecutionError::WorkerFailed)?;
        if let Some(expected) = &identity {
            let current = self.dispatcher.github_response_identity(context);
            context.check()?;
            if !current.as_ref().is_ok_and(|current| current == expected) {
                return Err(ExecutionError::WorkerFailed);
            }
        }
        pending.bind_github_identity(identity);
        self.retain_cursors(pending, context)
    }

    pub(super) fn publish(
        &self,
        structured: &mut Value,
        scope: &continuations::Scope<'_>,
        pending: &mut Pending,
    ) -> Result<(), contracts::ContractValidationError> {
        continuations::finalize_with(
            structured,
            self.id,
            scope,
            &crate::runtime::engine::admission::checked::checked_input,
            &mut |target, mut input, origin| {
                if input.get("responseChars").is_none()
                    && let Some(chars) = self.response_query.get("responseChars")
                {
                    input["responseChars"] = chars.clone();
                }
                // The call's own tool continuations own what its rows
                // observed, taken once when the first is issued.
                if target == self.id && self.id.is_github() && !pending.owns_evidence() {
                    let access = &self.dispatcher.github_access;
                    match access.retained_digest() {
                        Some(digest) => pending.keep_evidence(self.id, digest),
                        None => pending.own_evidence(self.id, access.observed()),
                    }
                }
                Some(pending.insert(
                    target,
                    input,
                    SavedWork::ToolResume {
                        index: self.resumes(origin),
                    },
                ))
            },
        )
    }

    /// The query a continuation issued at row position `origin` resumes.
    fn resumes(&self, origin: Option<usize>) -> Option<usize> {
        match &self.origin {
            RowOrigin::Query(index) => Some(*index),
            origin_of_row => origin.map(|position| origin_of_row.of(position)),
        }
    }

    /// What a saved page keeps of its rows' queries: a page re-executes only
    /// the admitted rows, so positions that are not their indices are kept.
    fn page_origin(&self) -> (Option<usize>, Option<Vec<usize>>) {
        match &self.origin {
            RowOrigin::Query(index) => (Some(*index), None),
            RowOrigin::Rows(indices) => (None, Some(indices.clone())),
            RowOrigin::Request => {
                let identity = self
                    .row_indices
                    .iter()
                    .copied()
                    .eq(0..self.row_indices.len());
                (None, (!identity).then(|| self.row_indices.clone()))
            }
        }
    }

    /// State the rows' walks as the reply's one continuation, the envelope's
    /// `next`: one call continues every row that reads on (`next.nextPage`
    /// leaves the rows), and fills its page again, so a reply that carries
    /// `next` is unfinished and one without it is read to its end. When the
    /// response itself pages, that call follows its last page (`row_pages`
    /// keeps it as the walk past the pages), so one loop through `next`
    /// reaches everything. A section's own walk (`next.<section>`) continues
    /// that section alone: it stays on its row, and a split row places it on
    /// the row's last part (`row_pages`, `take_enclosing_walks`).
    ///
    /// `true` when it measured the envelope and left it as it is because it
    /// fits one page, so the stage need not measure it again
    /// ([`octocode_runtime_base::response::page_model::ResponsePageOptions::fits_page`]).
    pub(super) fn hoist_walks(
        &self,
        structured: &mut Sanitized,
        pending: &mut Pending,
        continues_page: bool,
    ) -> Result<bool, contracts::ContractValidationError> {
        let mut walks = continuations::linear_walks(self.id, structured)?;
        walks.retain(|walk| {
            walk.last()
                .is_some_and(|name| name == continuations::LINEAR_WALK)
        });
        let together = read_share::fill_together(self.id) && walks.len() > 1;
        let handles = continuations::walk_handles(structured, &walks);
        if !handles.is_empty()
            && let Some(token) = pending.walk_handle(self.id, &handles)
        {
            structured.edit(|output| {
                continuations::hoist(output, &walks, self.id, &token);
            });
        }
        if continues_page || together {
            return Ok(false);
        }
        let budget = self
            .options
            .response_chars
            .unwrap_or(self.output.auto_page_chars);
        if budget == 0 {
            return Ok(false);
        }
        Ok(!self.options.pages(structured, self.output.auto_page_chars))
    }

    /// Every read of a section plan restarts its idle lifetime: a page read
    /// from a saved clasify response refreshes each plan it continues, named
    /// in its rows or in the walk handle a split batch's next page is hoisted
    /// to (the envelope's `next`), so a reader walking the pages still
    /// reaches the next batch. Opening the handle restarts its own lifetime,
    /// as a read of it does.
    pub(super) fn refresh_plans(&self, saved: &Value) {
        let walk = saved
            .pointer("/next/input/cursor")
            .and_then(Value::as_str)
            .and_then(|token| self.cursors.open(self.id, token, self.access).ok());
        let inputs = std::iter::once(saved).chain(walk.as_ref().map(|walk| &walk.member().input));
        octocode_tool_clasify::clasify::sections::refresh_continued(
            &self.section_sessions,
            &self.access,
            inputs,
        );
    }

    pub(super) fn register_page(
        &self,
        outcome: &mut ToolOutcome,
        cursor: Option<Cursor>,
        pending: &mut Pending,
    ) -> Result<(), ExecutionError> {
        let Some(cursor) = cursor else {
            return Ok(());
        };
        outcome.structured_content.edit(|output| {
            let next = &mut output[continuations::RESPONSE_NEXT];
            let (index, rows) = self.page_origin();
            let token = pending.insert(
                self.id,
                self.response_query.clone(),
                SavedWork::ResponsePage {
                    cursor,
                    shape: self.page_shape.clone(),
                    index,
                    rows,
                },
            );
            // The page was measured with the placeholder handle, which has
            // the issued handle's exact length (`cursor::issued`), so the
            // page size and its oversized flag stand.
            next["input"] = json!({"cursor":token});
            Ok(())
        })
    }
}
