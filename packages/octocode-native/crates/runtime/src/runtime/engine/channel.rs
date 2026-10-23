//! One admitted call in its blocking worker: execute the rows on the batch
//! budget, shape and settle them once, then respond (and keep a replay seed
//! when the response pages); a replayed page responds from its seed.

mod batch;
mod shaping;
use super::page_replay::{PageReplay, PageReplayMemo, page_expiry, snapshot_expired};
use super::{RuntimeError, runtime_execution_error};
use crate::contracts;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::cursor::Cursor;
use crate::response::outcome::ToolOutcome;
use crate::response::pager::ResponsePageOptions;
use crate::response::result::FailureKind;
use crate::response::{continuations, stage};
use crate::security::{CheckedQuery, Sanitized};
use batch::execute_ordinary_queries;
use shaping::ShapedRows;

use serde_json::{Value, json};
use std::sync::Arc;

type StageResult = Result<Result<ToolOutcome, contracts::ContractValidationError>, ExecutionError>;

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
    pub(super) response_query: Value,
    pub(super) clasify: bool,
    pub(super) dispatcher: crate::runtime::domain_dispatch::DomainDispatcher,
    pub(super) output: OutputSettings,
    pub(super) replayed: Option<Arc<PageReplay>>,
    pub(super) replays: (Arc<PageReplayMemo>, [u8; 32]),
    pub(super) section_sessions: Arc<crate::tools::clasify::sections::Sessions>,
    pub(super) access: [u8; 32],
}

impl ChannelRun {
    pub(super) fn check_replay_access(
        &self,
        context: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        let Some(expected) = self
            .replayed
            .as_ref()
            .and_then(|saved| saved.github_identity.as_ref())
        else {
            return Ok(());
        };
        context.check().map_err(runtime_execution_error)?;
        let identity = self.dispatcher.github_response_identity(context);
        context.check().map_err(runtime_execution_error)?;
        if identity.as_ref().is_ok_and(|identity| identity == expected) {
            Ok(())
        } else {
            Err(snapshot_expired())
        }
    }

    pub(super) fn run(mut self, context: &ExecutionContext) -> StageResult {
        if let Some(replay) = self.replayed.take() {
            let mut stage = self.stage(Sanitized::scrubbed(Value::Null), true);
            stage.options.saved = Some(crate::response::pager::SavedResponse {
                key: self.replays.1,
                expires_at: replay.expires_at.clone(),
                storage: self.replays.0.storage(),
                replayed: true,
            });
            return stage::respond_saved(
                stage::StageInput {
                    failure: replay.failure,
                    ..stage
                },
                &replay.structured,
                &replay.remaining,
                &replay.pages,
                context,
            );
        }
        let rejected = std::mem::take(&mut self.rejected_rows);
        let mcp = self.mcp;
        let available = |target: &str| {
            // An MCP-mode call can never execute a contract `cliOnly` tool
            // (see the admission check in `execute_channel`).
            !(mcp && ToolId::from_name(target).is_some_and(ToolId::is_cli_only))
                && self.dispatcher.availability.is_available(target)
        };
        let scope = continuations::Scope {
            is_available: &available,
        };
        if self.clasify {
            debug_assert!(rejected.is_empty(), "admission refuses clasify as a whole");
            self.run_clasify(&scope, context)
        } else {
            self.run_rows(rejected, &scope, context)
        }
    }

    /// The shared response-stage input for `structured`; the caller sets the
    /// row facts (failure, source digest).
    fn stage(&self, structured: Sanitized, allow_auto_paging: bool) -> stage::StageInput {
        stage::StageInput {
            tool: self.id,
            structured,
            options: ResponsePageOptions {
                saved: Some(crate::response::pager::SavedResponse {
                    key: self.replays.1,
                    expires_at: page_expiry(),
                    storage: self.replays.0.storage(),
                    replayed: false,
                }),
                ..self.options.clone()
            },
            failure: None,
            auto_page_chars: self.output.auto_page_chars,
            allow_auto_paging,
        }
    }

    /// Clasify evaluates once; its settled answers use the same saved-page store.
    fn run_clasify(
        &self,
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> StageResult {
        let mut receipts = crate::tools::clasify::call(
            &self.dispatcher.evaluator,
            &self.queries,
            &self.dispatcher,
            context,
            crate::tools::clasify::sections::Runtime {
                sessions: &self.section_sessions,
                access: self.access,
                can_resume: self.mcp || self.replays.0.persistent,
            },
        )?;
        if let Err(error) = receipts.structured.edit(|structured| {
            continuations::finalize(
                structured,
                self.id,
                &continuations::Sources::Matrices(&self.response_query),
                scope,
            )
        }) {
            debug_assert!(false, "invalid clasify continuation: {:?}", error.issues);
        }
        let mut failure = receipts.failure;
        let remaining = match stage::settle(
            self.id,
            &mut receipts.structured,
            &self.response_query,
            &mut failure,
        ) {
            Ok(remaining) => remaining,
            Err(error) => return Ok(Err(error)),
        };
        let settled = Settled {
            response_query: &self.response_query,
            failure,
        };
        let (mut finished, seed) =
            self.respond_settled(&settled, receipts.structured, remaining, context)?;
        if let (Ok(outcome), Some(seed)) = (&mut finished, seed) {
            self.remember_page(outcome, seed, context)?;
        }
        Ok(finished)
    }

    /// Ordinary rows: execute, shape each row, envelope, continuations, then
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
    ) -> StageResult {
        let ShapedRows {
            mut structured,
            response_query,
            rejected_at,
            mut failure,
        } = self.shape_rows(rejected, scope, context)?;
        let mut remaining =
            match stage::settle(self.id, &mut structured, &response_query, &mut failure) {
                Ok(remaining) => remaining,
                Err(error) => return Ok(Err(error)),
            };
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
        let (mut finished, seed) =
            self.respond_settled(&settled, structured, remaining, context)?;
        if let Ok(outcome) = &mut finished {
            if let Some(seed) = seed {
                self.remember_page(outcome, seed, context)?;
            }
            crate::runtime::row_indices::attach_rejected(outcome, rejected_rows);
        }
        Ok(finished)
    }

    /// Respond over a settled envelope; the stage keeps a copy of it (the
    /// replay seed) only when the response windows.
    fn respond_settled(
        &self,
        settled: &Settled<'_>,
        structured: Sanitized,
        remaining: Vec<Vec<crate::response::pages::Remaining>>,
        context: &ExecutionContext,
    ) -> Result<
        (
            Result<ToolOutcome, contracts::ContractValidationError>,
            Option<PageReplay>,
        ),
        ExecutionError,
    > {
        let pages = crate::response::pager::SavedPages::default();
        let mut kept = None;
        let stage = stage::StageInput {
            failure: settled.failure,
            ..self.stage(structured, true)
        };
        let expires_at = stage
            .options
            .saved
            .as_ref()
            .map(|saved| saved.expires_at.clone())
            .unwrap_or_default();
        let finished = stage::respond(stage, remaining.clone(), Some(&mut kept), context)?;
        let seed = kept.map(|structured| PageReplay {
            access: self.access,
            github_identity: None,
            structured,
            remaining,
            response_query: settled.response_query.clone(),
            failure: settled.failure,
            expires_at,
            pages,
        });
        Ok((finished, seed))
    }

    /// Publish a continuation only after its complete result is retained.
    fn remember_page(
        &self,
        outcome: &mut ToolOutcome,
        mut seed: PageReplay,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        let pagination = &outcome.structured_content["responsePagination"];
        // `next` carries the cursor while pages remain; a page without one
        // names its own page in the block.
        let Some(cursor) = pagination["next"]["query"]["responseSnapshot"]
            .as_str()
            .or_else(|| pagination["snapshot"].as_str())
            .and_then(Cursor::decode)
        else {
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
            self.replays.0.insert(&cursor, seed)
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
                .map_err(|_| ExecutionError::WorkerFailed)?;
        }
        Ok(())
    }
}

/// The settled facts every response of one call shares.
struct Settled<'a> {
    response_query: &'a Value,
    failure: Option<FailureKind>,
}

#[cfg(test)]
mod batch_budget_tests;
