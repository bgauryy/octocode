//! One admitted call in its blocking worker: execute the rows on the batch
//! budget, shape and settle them once, then respond (and keep a replay seed
//! when the response pages); a replayed page responds from its seed.

mod batch;
mod publication;
mod shaping;
use super::cursors::Registry;
use super::page_replay::{
    PageReplay, PageReplayMemo, Refusal, RefusalScope, page_expiry, snapshot_expired,
    source_changed,
};
use super::{RuntimeError, runtime_execution_error};
use crate::contracts;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::cursor::Cursor;
use crate::response::outcome::ToolOutcome;
use crate::response::page_model::ResponsePageOptions;
use crate::response::result::FailureKind;
use crate::response::{continuations, stage};
use crate::security::{CheckedQuery, Sanitized};
use batch::execute_ordinary_queries;
use shaping::ShapedRows;

use serde_json::{Value, json};
use std::sync::Arc;

type StageResult = Result<Result<ToolOutcome, contracts::ContractValidationError>, ExecutionError>;

/// A call's answer: its outcome, or the typed error a caller receives.
type RunResult = Result<Result<ToolOutcome, RuntimeError>, ExecutionError>;

/// Output settings one call reads from the resolved config.
#[derive(Clone, Copy)]
pub(super) struct OutputSettings {
    pub(super) redact_emails: bool,
    pub(super) auto_page_chars: usize,
}

/// One admitted call, moved into its blocking worker: admit → run → shape
/// → check → respond.
pub(super) struct ChannelRun {
    pub(super) id: ToolId,
    pub(super) mcp: bool,
    pub(super) options: ResponsePageOptions,
    /// The admitted rows, each passed by the input gate.
    pub(super) queries: Vec<CheckedQuery>,
    pub(super) rejected_rows: Vec<(usize, Value)>,
    /// Original input index of each admitted row (`row_indices` module).
    pub(super) row_indices: Vec<usize>,
    /// Query that issued this saved work; fresh requests use their own indices.
    pub(super) origin_index: Option<usize>,
    /// The request states its queries itself, rather than resuming saved work.
    pub(super) fresh: bool,
    pub(super) response_query: Value,
    pub(super) clasify: bool,
    pub(super) dispatcher: crate::runtime::domain_dispatch::DomainDispatcher,
    pub(super) output: OutputSettings,
    pub(super) replayed: Option<Arc<PageReplay>>,
    /// The saved page this run re-executes because its response is no
    /// longer retained: the run answers it only when its sources reproduce
    /// the content that page was cut from.
    pub(super) pinned: Option<Cursor>,
    pub(super) replays: (Arc<PageReplayMemo>, [u8; 32]),
    pub(super) section_sessions: Arc<crate::tools::clasify::sections::Sessions>,
    pub(super) access: [u8; 32],
    pub(super) cursors: Arc<Registry>,
    pub(super) resume_identity: Option<String>,
}

impl ChannelRun {
    pub(super) fn check_replay_access(
        &self,
        context: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        let Some(expected) = self.resume_identity.as_ref() else {
            return Ok(());
        };
        context.check().map_err(runtime_execution_error)?;
        let identity = self.dispatcher.github_response_identity(context);
        context.check().map_err(runtime_execution_error)?;
        if identity.as_ref().is_ok_and(|identity| identity == expected) {
            Ok(())
        } else {
            Err(snapshot_expired(
                Refusal::IdentityChanged,
                RefusalScope {
                    persistent: self.replays.0.persistent,
                    mcp: self.mcp,
                },
            ))
        }
    }

    pub(super) fn run(mut self, context: &ExecutionContext) -> RunResult {
        let id = self.id;
        let contract = |result: StageResult| {
            result.map(|result| {
                result.map_err(|error| super::admission::output_contract_error(id.as_str(), error))
            })
        };
        if let Some(replay) = self.replayed.take() {
            let mut stage = self.stage(Sanitized::scrubbed(Value::Null), true);
            if let Some(saved) = stage.options.saved.as_mut() {
                saved.replayed = true;
            }
            let stage::Staged { mut result, next } = stage::respond_saved(
                stage::StageInput {
                    failure: replay.failure,
                    ..stage
                },
                &replay.structured,
                &replay.remaining,
                &replay.pages,
                context,
            )?;
            if let Ok(outcome) = &mut result {
                // The next saved page is bound to the identity the opened
                // one was issued under.
                let mut pending = self.pending();
                pending.bind_github_identity(self.resume_identity.clone());
                self.register_page(outcome, next, &mut pending)?;
                self.cursors.retain(pending, context)?;
            }
            return contract(Ok(result));
        }
        let rejected = std::mem::take(&mut self.rejected_rows);
        let mcp = self.mcp;
        let available = |target: &str| {
            self.dispatcher.availability.is_available(target)
                && (!mcp
                    || ToolId::from_name(target).is_none_or(|id| {
                        crate::execution::availability::mcp_restriction(&self.dispatcher.config, id)
                            .is_none()
                    }))
        };
        let scope = continuations::Scope {
            is_available: &available,
        };
        match self.run_rows(rejected, &scope, context)? {
            Rows::Answered(result) => contract(Ok(result)),
            Rows::Changed => Ok(Err(source_changed())),
        }
    }

    /// The shared response-stage input for `structured`; the caller sets the
    /// row facts (failure, source digest).
    fn stage(&self, structured: Sanitized, allow_auto_paging: bool) -> stage::StageInput {
        stage::StageInput {
            tool: self.id,
            structured,
            options: ResponsePageOptions {
                // Every read of saved work restarts its lifetime, so each
                // page states the expiry its own read set. A later page is a
                // replay whether it is cut from the retained response or
                // from a re-execution that reproduced it.
                saved: Some(crate::response::page_model::SavedResponse {
                    key: self.replays.1,
                    expires_at: page_expiry(),
                    replayed: self.pinned.is_some(),
                }),
                ..self.options.clone()
            },
            failure: None,
            auto_page_chars: self.output.auto_page_chars,
            allow_auto_paging: allow_auto_paging && self.id != ToolId::GhCloneRepo,
        }
    }

    /// Execute, shape each row, envelope, continuations, then
    /// the shared response stage (and a replay seed when the response pages).
    /// The rows are shaped and settled once, rejected rows included, so
    /// every row a caller reads is published and validated alike. A batch
    /// with rejected rows that pages pages only its admitted rows (the body a
    /// continuation reproduces); the rejected rows ride page 1. The response
    /// stage runs once either way.
    fn run_rows(
        &self,
        rejected: Vec<(usize, Value)>,
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> Result<Rows, ExecutionError> {
        let ShapedRows {
            mut structured,
            response_query,
            rejected_at,
            mut failure,
        } = self.shape_rows(rejected, context)?;
        let mut pending = self.pending();
        if let Err(error) =
            structured.edit(|structured| self.publish(structured, scope, &mut pending))
        {
            return Ok(Rows::Answered(Err(error)));
        }
        let mut remaining =
            match stage::settle(self.id, &mut structured, &response_query, &mut failure) {
                Ok(remaining) => remaining,
                Err(error) => return Ok(Rows::Answered(Err(error))),
            };
        structured.edit(|output| crate::runtime::row_indices::inherit(output, self.origin_index));
        if let Some(cursor) = &self.pinned
            && crate::response::pager::content_snapshot(&structured, cursor.length)
                != Some(cursor.content_snapshot())
        {
            return Ok(Rows::Changed);
        }
        let settled = Settled {
            response_query: &response_query,
            failure,
        };
        let mut rejected_rows = Vec::new();
        if !rejected_at.is_empty() && self.options.pages(&structured, self.output.auto_page_chars) {
            (remaining, rejected_rows) = structured.edit(|structured| {
                crate::runtime::row_indices::split_rejected(structured, remaining, &rejected_at)
            });
        }
        let (
            stage::Staged {
                result: mut finished,
                mut next,
            },
            seed,
        ) = self.respond_settled(&settled, structured, remaining, context)?;
        if let Ok(outcome) = &mut finished {
            if let Some(seed) = seed {
                self.remember_page(outcome, &mut next, seed, context)?;
            }
            self.register_page(outcome, next, &mut pending)?;
            self.retain_pending(pending, context)?;
            if self.fresh {
                outcome.structured_content.edit(|output| {
                    crate::response::pages::drop_echoes(
                        output,
                        response_query["queries"]
                            .as_array()
                            .map_or(&[], Vec::as_slice),
                        &self.row_indices,
                    );
                });
            }
            // Rejected rows ride page 1 outside the plan that measured it,
            // so that page is measured again with them.
            if !rejected_rows.is_empty() {
                crate::runtime::row_indices::attach_rejected(outcome, rejected_rows);
                outcome
                    .structured_content
                    .edit(|output| {
                        crate::response::pager::measure_page(
                            output,
                            self.options
                                .response_chars
                                .or(Some(self.output.auto_page_chars)),
                        )
                    })
                    .map_err(|_| ExecutionError::WorkerFailed)?;
            }
        }
        Ok(Rows::Answered(finished))
    }

    /// Respond over a settled envelope; the stage keeps a copy of it (the
    /// replay seed) only when the response windows.
    fn respond_settled(
        &self,
        settled: &Settled<'_>,
        structured: Sanitized,
        remaining: Vec<Vec<crate::response::result::Remaining>>,
        context: &ExecutionContext,
    ) -> Result<(stage::Staged, Option<PageReplay>), ExecutionError> {
        let pages = crate::response::pager::SavedPages::default();
        let mut kept = None;
        let stage = stage::StageInput {
            failure: settled.failure,
            ..self.stage(structured, true)
        };
        let finished = stage::respond(stage, remaining.clone(), Some(&mut kept), context)?;
        let seed = kept.map(|structured| PageReplay {
            access: self.access,
            github_identity: None,
            structured,
            remaining,
            response_query: settled.response_query.clone(),
            failure: settled.failure,
            pages,
        });
        Ok((finished, seed))
    }

    /// Publish a continuation only after its complete result is retained.
    fn remember_page(
        &self,
        outcome: &mut ToolOutcome,
        next: &mut Option<Cursor>,
        mut seed: PageReplay,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        let Some(cursor) = next.as_ref() else {
            return Ok(());
        };
        let auth_retained = match self.dispatcher.github_access.identity() {
            Ok(Some(observed)) => {
                let current = self.dispatcher.github_response_identity(context);
                context.check()?;
                if current.as_ref().is_ok_and(|identity| identity == &observed) {
                    seed.github_identity = Some(observed);
                    true
                } else {
                    false
                }
            }
            Ok(None) => true,
            Err(()) => false,
        };
        let (saved, retained) = if auth_retained {
            self.replays.0.insert(cursor, seed)
        } else {
            (Arc::new(seed), false)
        };
        if !retained {
            let mut stage = self.stage(saved.structured.clone(), false);
            stage.options = ResponsePageOptions::default();
            stage.failure = saved.failure;
            stage.structured.edit(|structured| {
                let warnings = structured.as_object_mut().and_then(|map| {
                    map.entry("warnings")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                });
                if let Some(warnings) = warnings {
                    warnings.push(json!("The response could not be retained with a stable access identity or within snapshot storage limits; the complete result is returned without display pagination."));
                }
            });
            *outcome = stage::respond(stage, saved.remaining.clone(), None, context)?
                .result
                .map_err(|_| ExecutionError::WorkerFailed)?;
            *next = None;
        }
        Ok(())
    }
}

/// What executing the rows answered: the call's outcome, or, for a
/// [`ChannelRun::pinned`] page, that its sources no longer reproduce it.
enum Rows {
    Answered(Result<ToolOutcome, contracts::ContractValidationError>),
    Changed,
}

/// The settled facts every response of one call shares.
struct Settled<'a> {
    response_query: &'a Value,
    failure: Option<FailureKind>,
}

#[cfg(test)]
mod batch_budget_tests;
