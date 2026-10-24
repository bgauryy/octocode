//! Cursor publication binds saved work to the access identity used by the call.
use super::ChannelRun;
use crate::contracts;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::{continuations, cursor::Cursor, outcome::ToolOutcome};
use crate::runtime::engine::cursors::{Pending, SavedWork};
use serde_json::{Value, json};

impl ChannelRun {
    pub(super) fn pending(&self) -> Result<Pending, ExecutionError> {
        Pending::new(self.access, None).ok_or(ExecutionError::WorkerFailed)
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
        if self.cursors.retain(pending) {
            Ok(())
        } else {
            Err(ExecutionError::WorkerFailed)
        }
    }

    pub(super) fn publish(
        &self,
        structured: &mut Value,
        scope: &continuations::Scope<'_>,
        pending: &mut Pending,
    ) -> Result<(), contracts::ContractValidationError> {
        continuations::finalize(structured, self.id, scope, &mut |target, mut input| {
            if input.get("responseChars").is_none()
                && let Some(chars) = self.response_query.get("responseChars")
            {
                input["responseChars"] = chars.clone();
            }
            pending.insert(target, input, SavedWork::ToolResume)
        })
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
            let token = pending
                .insert(
                    self.id,
                    self.response_query.clone(),
                    SavedWork::ResponsePage(cursor),
                )
                .ok_or(ExecutionError::WorkerFailed)?;
            next["input"] = json!({"cursor":token});
            crate::response::pager::measure_page(
                output,
                self.options
                    .response_chars
                    .or(Some(self.output.auto_page_chars)),
            )
            .map_err(|_| ExecutionError::WorkerFailed)
        })
    }
}
