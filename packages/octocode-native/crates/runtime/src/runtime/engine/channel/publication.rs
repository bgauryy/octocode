//! Cursor publication binds saved work to the access identity used by the call.
use super::ChannelRun;
use crate::contracts;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::{continuations, cursor::Cursor, outcome::ToolOutcome};
use crate::runtime::engine::cursors::{Pending, SavedWork};
use crate::runtime::row_indices::RowOrigin;
use crate::security::Sanitized;
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
        continuations::finalize(
            structured,
            self.id,
            scope,
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

    /// Move the rows' linear walks to `responsePagination.next`, so one loop
    /// reaches everything the request asked for: when the response pages (a
    /// page break would sit between a row and its walk), when more than one
    /// walk remains, or when a walk continues the page its row answered
    /// (`continues_page`: the rest of that page is the rest of this
    /// response). One walk to a later page on an unpaged response stays
    /// where it is. A section's own walk (`next.<section>`) continues that
    /// section alone: it stays on its row, and a split row places it on the
    /// row's last part (`row_pages`, `take_enclosing_walks`).
    pub(super) fn hoist_walks(
        &self,
        structured: &mut Sanitized,
        pending: &mut Pending,
        continues_page: bool,
    ) -> Result<(), contracts::ContractValidationError> {
        let mut walks = continuations::linear_walks(self.id, structured)?;
        walks.retain(|walk| {
            walk.last()
                .is_some_and(|name| name == continuations::LINEAR_WALK)
        });
        let splits = self.options.pages(structured, self.output.auto_page_chars);
        let handles = continuations::walk_handles(structured, &walks);
        if handles.is_empty() || (handles.len() == 1 && !splits && !continues_page) {
            return Ok(());
        }
        let Some(token) = pending.walk_handle(self.id, &handles) else {
            return Ok(());
        };
        structured.edit(|output| continuations::hoist(output, &walks, self.id, &token));
        Ok(())
    }

    /// Every read of a section plan restarts its idle lifetime: a page read
    /// from a saved clasify response refreshes each plan it continues, named
    /// in its rows or in the walk handle a split batch's next page is hoisted
    /// to (`responsePagination.next`), so a reader walking the pages still
    /// reaches the next batch. Opening the handle restarts its own lifetime,
    /// as a read of it does.
    pub(super) fn refresh_plans(&self, saved: &Value) {
        let walk = saved
            .pointer("/responsePagination/next/input/cursor")
            .and_then(Value::as_str)
            .and_then(|token| self.cursors.open(self.id, token, self.access).ok());
        let inputs = std::iter::once(saved).chain(walk.as_ref().map(|walk| &walk.member().input));
        crate::tools::clasify::sections::refresh_continued(
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
            let next = &mut output["responsePagination"]["next"];
            let (index, rows) = self.page_origin();
            let token = pending.insert(
                self.id,
                self.response_query.clone(),
                SavedWork::ResponsePage {
                    cursor,
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
