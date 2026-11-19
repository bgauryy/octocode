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
use batch::execute_ordinary_queries;
use octocode_runtime_base::contracts;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::cursor::Cursor;
use octocode_runtime_base::response::outcome::ToolOutcome;
use octocode_runtime_base::response::page_model::ResponsePageOptions;
use octocode_runtime_base::response::result::ErrorClass;
use octocode_runtime_base::response::{continuations, stage};
use octocode_runtime_base::security::{CheckedQuery, Sanitized};
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
    pub(super) redact_uuid: bool,
    pub(super) auto_page_chars: usize,
}

/// One admitted call, moved into its blocking worker: admit → run → shape
/// → check → respond.
pub(super) struct ChannelRun {
    pub(super) id: ToolId,
    pub(super) mcp: bool,
    /// The host stays alive for another call, or saved work survives its exit.
    pub(super) can_resume: bool,
    pub(super) options: ResponsePageOptions,
    /// The admitted rows, each passed by the input gate.
    pub(super) queries: Vec<CheckedQuery>,
    pub(super) rejected_rows: Vec<(usize, Value)>,
    /// Original input index of each admitted row (`row_indices` module).
    pub(super) row_indices: Vec<usize>,
    /// The query each row answers; fresh requests use their own indices.
    pub(super) origin: crate::runtime::row_indices::RowOrigin,
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
    pub(super) page_shape: super::cursors::PageShape,
    pub(super) replays: (Arc<PageReplayMemo>, [u8; 32]),
    pub(super) section_sessions: Arc<octocode_tool_clasify::clasify::sections::Sessions>,
    pub(super) access: [u8; 32],
    pub(super) cursors: Arc<Registry>,
    pub(super) resume_identity: Option<String>,
    /// Sent fields read as declared ones ([`octocode_runtime_base::contracts::normalized_input`]):
    /// the response states each, so the caller learns the field.
    pub(super) repairs: Vec<String>,
}

/// A saved response whose content is no longer the one the cursor's earlier
/// pages were cut from (its stored bytes changed), or whose page is past
/// its end: the page it named no longer exists, so the walk restarts.
fn saved_page_changed() -> RuntimeError {
    RuntimeError::new(
        octocode_contract_types::ErrorCode::ResponseSnapshotExpired,
        "The saved response no longer matches the page this cursor named. No source or provider was called. Rerun the original request without cursor.",
    )
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
                    tool: self.id,
                },
            ))
        }
    }

    pub(super) fn run(mut self, context: &ExecutionContext) -> RunResult {
        let id = self.id;
        // Both branches answer through here, once the response is final:
        // its calls were routed, published and validated with their `kind`,
        // which a served call no longer states.
        let contract = |result: StageResult| {
            result.map(|result| {
                result
                    .and_then(|mut outcome| {
                        outcome
                            .structured_content
                            .edit(|output| continuations::drop_call_kinds(id, output))?;
                        Ok(outcome)
                    })
                    .map_err(|error| super::admission::output_contract_error(id.as_str(), error))
            })
        };
        if let Some(replay) = self.replayed.take() {
            if self.clasify {
                self.refresh_plans(&replay.structured);
            }
            let mut stage = self.stage(Sanitized::scrubbed(Value::Null), true);
            if let Some(saved) = stage.options.saved.as_mut() {
                saved.replayed = true;
            }
            let stage::Staged {
                mut result, next, ..
            } = match stage::respond_saved(
                stage::StageInput {
                    failure: replay.failure,
                    ..stage
                },
                &replay.structured,
                &replay.pages,
                context,
            ) {
                Ok(staged) => staged,
                Err(stage::SavedPageError::Changed) => return Ok(Err(saved_page_changed())),
                Err(stage::SavedPageError::Failed(error)) => return Err(error),
            };
            if let Ok(outcome) = &mut result {
                // The next saved page is bound to the identity the opened
                // one was issued under.
                let mut pending = self.pending();
                pending.bind_github_identity(self.resume_identity.clone());
                self.register_page(outcome, next, &mut pending)?;
                self.retain_cursors(pending, context)?;
                self.note_cursor_call(outcome);
            }
            return contract(Ok(result));
        }
        let mut rejected = std::mem::take(&mut self.rejected_rows);
        self.prepare_page_shape(&mut rejected);
        let mcp = self.mcp;
        let available = |target: &str| {
            self.dispatcher.availability.is_available(target)
                && (!mcp
                    || ToolId::from_name(target).is_none_or(|id| {
                        octocode_runtime_base::execution::availability::mcp_restriction(
                            &self.dispatcher.config,
                            id,
                        )
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
                saved: Some(octocode_runtime_base::response::page_model::SavedResponse {
                    key: self.replays.1,
                    expires_at: page_expiry(self.id.as_str()),
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
        // What the rows say for the result (a cache write refused) is this
        // call's: stated in its `warnings`, never on stderr.
        let said = octocode_io::notice::ResultNotes::default();
        let noting = said.enter();
        let ShapedRows {
            mut structured,
            response_query,
            rejected_at,
            mut failure,
            continues_page,
        } = self.shape_rows(rejected, context)?;
        // Original repair notes belong to the response being reproduced;
        // notes about this cursor call are added outside its page plan.
        if !self.page_shape.repairs.is_empty() {
            structured.edit(|structured| {
                if let Some(warnings) = structured.as_object_mut().and_then(|map| {
                    map.entry("warnings")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                }) {
                    warnings.extend(self.page_shape.repairs.iter().map(|note| json!(note)));
                }
            });
        }
        let mut pending = self.pending();
        if let Err(error) =
            structured.edit(|structured| self.publish(structured, scope, &mut pending))
        {
            return Ok(Rows::Answered(Err(error)));
        }
        if let Err(error) = stage::settle(self.id, &mut structured, &response_query, &mut failure) {
            return Ok(Rows::Answered(Err(error)));
        }
        drop(noting);
        if self.pinned.is_none() {
            self.dispatcher.warm_served_leads(self.id, &structured);
        }
        let call_notes = said.take();
        structured.edit(|output| crate::runtime::row_indices::inherit(output, &self.origin));
        let fits_page = match self.hoist_walks(&mut structured, &mut pending, continues_page) {
            Ok(fits_page) => fits_page,
            Err(error) => return Ok(Rows::Answered(Err(error))),
        };
        if let Some(cursor) = &self.pinned
            && octocode_runtime_base::response::pager::content_snapshot(
                &structured,
                cursor.length,
                self.options.build_identity,
            ) != Some(cursor.content_snapshot(self.options.build_identity))
        {
            return Ok(Rows::Changed);
        }
        let settled = Settled {
            response_query: &response_query,
            failure,
        };
        let mut rejected_rows = Vec::new();
        if !rejected_at.is_empty()
            && !fits_page
            && self.options.pages(&structured, self.output.auto_page_chars)
        {
            rejected_rows = structured.edit(|structured| {
                crate::runtime::row_indices::split_rejected(structured, &rejected_at)
            });
        }
        let (
            stage::Staged {
                result: mut finished,
                mut next,
                paging,
            },
            seed,
        ) = self.respond_settled(&settled, structured, fits_page, context)?;
        if let Ok(outcome) = &mut finished {
            if let Some(seed) = seed {
                self.remember_page(outcome, &mut next, seed, !pending.is_empty(), context)?;
            }
            self.register_page(outcome, next, &mut pending)?;
            self.retain_pending(pending, context)?;
            // Only the first response to a fresh request drops the fields
            // that repeat its queries; a page read through a cursor keeps
            // them, because its request names none of them.
            if self.fresh {
                outcome.structured_content.edit(|output| {
                    octocode_runtime_base::response::pages::drop_echoes(
                        self.id == octocode_runtime_base::id::ToolId::GhGetFileContent,
                        output,
                        response_query["queries"]
                            .as_array()
                            .map_or(&[], Vec::as_slice),
                        &self.row_indices,
                    );
                    // The cut measured the echoes just dropped: the flag
                    // states the page delivered (before rejected rows join).
                    octocode_runtime_base::response::pager::unflag_fitting(
                        output,
                        self.id,
                        self.options
                            .response_chars
                            .unwrap_or(self.output.auto_page_chars),
                        paging,
                    );
                });
            }
            // Rejected rows ride page 1 whole, outside the plan and the
            // measure of that page: call metadata, like the volatile
            // provider facts, never moves an edge or makes it `oversized`.
            crate::runtime::row_indices::attach_rejected(outcome, rejected_rows);
            // What these rows measured of their own execution is said by
            // the call that asked for them. A pinned run executes them only
            // to serve a page of an earlier response again.
            if self.pinned.is_none() {
                stage::state_call_notes(outcome, &call_notes);
            }
            self.note_cursor_call(outcome);
        }
        Ok(Rows::Answered(finished))
    }

    /// The notes of a cursor call ride its page outside the plan and the
    /// measure: the page is the one the cursor names, cut from the content
    /// its walk was issued over, and a note about this call is no part of
    /// that content.
    fn note_cursor_call(&self, outcome: &mut ToolOutcome) {
        if self.fresh || self.repairs.is_empty() {
            return;
        }
        outcome.structured_content.edit(|output| {
            if let Some(warnings) = output.as_object_mut().and_then(|map| {
                map.entry("warnings")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
            }) {
                warnings.extend(self.repairs.iter().map(|note| json!(note)));
            }
        });
    }

    /// Respond over a settled envelope; the stage keeps a copy of it (the
    /// replay seed) only when the response windows. `fits_page`: the
    /// envelope was measured as it is and fits one page.
    fn respond_settled(
        &self,
        settled: &Settled<'_>,
        structured: Sanitized,
        fits_page: bool,
        context: &ExecutionContext,
    ) -> Result<(stage::Staged, Option<PageReplay>), ExecutionError> {
        let pages = octocode_runtime_base::response::pager::SavedPages::default();
        let mut kept = None;
        let mut stage = stage::StageInput {
            failure: settled.failure,
            ..self.stage(structured, true)
        };
        stage.options.fits_page = fits_page;
        let finished = stage::respond(stage, Some(&mut kept), context)?;
        let seed = kept.map(|structured| PageReplay {
            access: self.access,
            github_identity: None,
            structured,
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
        tool_cursors: bool,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        let Some(cursor) = next.as_ref() else {
            return Ok(());
        };
        // A host that keeps no saved work cannot serve a display cursor;
        // only a tool continuation makes the call unservable there.
        let keeps_pages = self.can_resume || tool_cursors;
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
        let (saved, retained) = if auth_retained && keeps_pages {
            self.replays.0.insert(
                self.id.as_str(),
                cursor,
                seed,
                octocode_runtime_base::execution::cancel::deadline_of(context),
            )
        } else {
            (Arc::new(seed), false)
        };
        if !retained {
            let cause = if keeps_pages {
                "The response could not be retained with a stable access identity or within snapshot storage limits"
            } else {
                "This command exits after one call and keeps no saved pages"
            };
            let warning =
                format!("{cause}; the complete result is returned without display pagination.");
            *outcome = match &self.pinned {
                Some(opened) if opened.offset > 0 => {
                    self.unpaged_tail(&saved, opened, warning, context)?
                }
                _ => {
                    let mut stage = self.stage(saved.structured.clone(), false);
                    stage.options = ResponsePageOptions::default();
                    stage.failure = saved.failure;
                    stage
                        .structured
                        .edit(|structured| push_warning(structured, &warning));
                    stage::respond(stage, None, context)?
                        .result
                        .map_err(|_| ExecutionError::WorkerFailed)?
                }
            };
            *next = None;
        }
        Ok(())
    }

    /// The rest of a saved response from the page `opened` named to its
    /// end, in one result: a reader that asked for a later page already
    /// holds the pages before it, and no cursor is left to walk on.
    fn unpaged_tail(
        &self,
        saved: &PageReplay,
        opened: &Cursor,
        warning: String,
        context: &ExecutionContext,
    ) -> Result<ToolOutcome, ExecutionError> {
        let mut tail: Option<ToolOutcome> = None;
        let mut offset = opened.offset;
        let onward = loop {
            let mut stage = self.stage(Sanitized::scrubbed(Value::Null), true);
            stage.options.response_offset = Some(offset);
            let staged = stage::respond_saved(
                stage::StageInput {
                    failure: saved.failure,
                    ..stage
                },
                &saved.structured,
                &saved.pages,
                context,
            )
            .map_err(|error| match error {
                stage::SavedPageError::Changed => ExecutionError::WorkerFailed,
                stage::SavedPageError::Failed(error) => error,
            })?;
            let page = staged.result.map_err(|_| ExecutionError::WorkerFailed)?;
            let more = staged.next.is_some();
            // The last page's own walk (rows' tool continuations) is the
            // only continuation left; a display cursor never is.
            let onward = (!more)
                .then(|| {
                    page.structured_content
                        .pointer("/responsePagination/next")
                        .cloned()
                })
                .flatten();
            tail = Some(match tail.take() {
                None => page,
                Some(mut whole) => {
                    let rows = page.structured_content["results"].clone();
                    whole.structured_content.edit(|output| {
                        if let Some(Value::Array(held)) = output.get_mut("results")
                            && let Value::Array(more_rows) = rows
                        {
                            held.extend(more_rows);
                        }
                    });
                    whole
                }
            });
            if !more {
                break onward;
            }
            offset += 1;
        };
        let mut whole = tail.ok_or(ExecutionError::WorkerFailed)?;
        whole.structured_content.edit(|output| {
            if let Some(map) = output.as_object_mut() {
                map.shift_remove("responsePagination");
            }
            if let Some(next) = onward {
                output["responsePagination"] = json!({ "next": next });
            }
            push_warning(output, &warning);
        });
        Ok(whole)
    }
}

fn push_warning(structured: &mut Value, warning: &str) {
    let warnings = structured.as_object_mut().and_then(|map| {
        map.entry("warnings")
            .or_insert_with(|| json!([]))
            .as_array_mut()
    });
    if let Some(warnings) = warnings {
        warnings.push(json!(warning));
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
    failure: Option<ErrorClass>,
}

#[cfg(test)]
mod batch_budget_tests;
