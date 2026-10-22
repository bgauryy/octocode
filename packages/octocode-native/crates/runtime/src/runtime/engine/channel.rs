//! One admitted call in its blocking worker: execute the rows on the batch
//! budget, shape and settle them once, then respond (and keep a replay seed
//! when the response pages); a replayed page responds from its seed.

use super::page_replay::{PageReplay, PageReplayMemo, page_expiry, snapshot_expired};
use super::{RuntimeError, runtime_execution_error};
use crate::contracts;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::outcome::ToolOutcome;
use crate::response::pager::ResponsePageOptions;
use crate::response::result::FailureKind;
use crate::response::{continuations, extras, read_share, render, rows, stage};

use serde_json::{Value, json};
use std::sync::Arc;

/// The batch parallelism budget: how many threads one batch runs its
/// queries on, and how many cores each query's parallel directory walk may
/// use. Read-only rows fan out one thread per query; mutating and
/// self-scheduled tools stay ordered. Each localSearch text walk also runs
/// its own worker pool, which defaults to every core, so concurrent walks
/// split the cores instead of stacking full pools. Measured on a 5-query
/// localSearch batch over the native crates (12 cores, optimized build): one
/// thread per query with full pools 114 ms and 86 peak threads, three workers
/// 116 ms / 58, two 118 ms / 44, one walk at a time 130 ms / 29, so the rows
/// stay concurrent and only the per-walk pools shrink. Splitting the cores
/// (2 walkers each) against the same build with full pools: 708 vs 710 ms
/// median, 22 vs 72 peak threads; rounding the share up changed nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchBudget {
    /// Queries run at once.
    width: usize,
    /// Walk workers per query; `None` keeps the engine default (all cores).
    walk_threads: Option<u32>,
}

impl BatchBudget {
    fn new(id: ToolId, queries: usize, cores: usize) -> Self {
        let width = if id.supports_concurrent_queries() {
            queries.max(1)
        } else {
            1
        };
        let walk_threads = (width > 1).then(|| u32_of_usize((cores / width).max(1)));
        Self {
            width,
            walk_threads,
        }
    }

    fn for_host(id: ToolId, queries: usize) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        Self::new(id, queries, cores)
    }
}

fn execute_ordinary_queries(
    tool: ToolId,
    queries: &[Value],
    response_page: Option<usize>,
    dispatcher: &crate::runtime::domain_dispatch::DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Vec<crate::response::outcome::DomainResult>, ExecutionError> {
    // Each row's share of the response page its output must fit.
    let context = &ExecutionContext {
        response_window: response_page
            .filter(|page| *page > 0)
            .map(|page| read_share::row_window(tool, page, queries.len())),
        ..context.clone()
    };
    let budget = BatchBudget::for_host(tool, queries.len());
    if budget.width <= 1 {
        return queries
            .iter()
            .map(|query| dispatcher.execute(tool, query, context))
            .collect();
    }

    let context = &ExecutionContext {
        walk_threads: budget.walk_threads,
        ..context.clone()
    };
    std::thread::scope(|scope| {
        let tasks = queries
            .iter()
            .map(|query| {
                scope.spawn(octocode_io::notice::carried(move || {
                    dispatcher.execute(tool, query, context)
                }))
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed)?)
            .collect()
    })
}

type StageResult = Result<Result<ToolOutcome, contracts::ContractValidationError>, ExecutionError>;

/// Output settings one call reads from the resolved config.
#[derive(Clone, Copy)]
pub(super) struct OutputSettings {
    pub(super) redact_emails: bool,
    pub(super) auto_page_chars: usize,
    pub(super) text_format: render::TextFormat,
}

/// One admitted call, moved into its blocking worker: admit → run → shape
/// → check → respond.
pub(super) struct ChannelRun {
    pub(super) id: ToolId,
    pub(super) mcp: bool,
    pub(super) options: ResponsePageOptions,
    pub(super) queries: Vec<Value>,
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
            let mut stage = self.stage(Value::Null, replay.response_query.clone(), true);
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
    fn stage(
        &self,
        structured: Value,
        response_query: Value,
        allow_auto_paging: bool,
    ) -> stage::StageInput {
        stage::StageInput {
            tool: self.id,
            structured,
            response_query,
            options: ResponsePageOptions {
                saved: Some(crate::response::pager::SavedResponse {
                    key: self.replays.1,
                    expires_at: page_expiry(),
                    storage: self.replays.0.storage(),
                    replayed: false,
                }),
                ..self.options.clone()
            },
            mcp: self.mcp,
            failure: None,
            auto_page_chars: self.output.auto_page_chars,
            text_format: self.output.text_format,
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
        if let Err(error) = continuations::finalize(
            &mut receipts.structured,
            self.id,
            &continuations::Sources::Matrices(&self.response_query),
            scope,
        ) {
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
    /// every row a caller reads is published and validated alike. A paged
    /// batch with rejected rows pages only its admitted rows (the body a
    /// continuation reproduces); the rejected rows ride page 1.
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
        let remaining = match stage::settle(self.id, &mut structured, &response_query, &mut failure)
        {
            Ok(remaining) => remaining,
            Err(error) => return Ok(Err(error)),
        };
        let settled = Settled {
            response_query: &response_query,
            failure,
        };
        let (finished, seed) = self.respond_settled(&settled, structured, remaining, context)?;
        let (mut finished, seed) = match (finished, seed) {
            // Page 1 of a paged batch: page the admitted rows alone, from
            // the settled envelope the stage kept.
            (Ok(outcome), Some(seed))
                if !rejected_at.is_empty() && crate::runtime::row_indices::is_paged(&outcome) =>
            {
                let (body, remaining, rejected_rows) = crate::runtime::row_indices::split_rejected(
                    seed.structured,
                    seed.remaining,
                    &rejected_at,
                );
                let (mut finished, seed) =
                    self.respond_settled(&settled, body, remaining, context)?;
                if let Ok(outcome) = &mut finished {
                    if let Some(seed) = seed {
                        self.remember_page(outcome, seed, context)?;
                    }
                    crate::runtime::row_indices::attach_rejected(
                        outcome,
                        rejected_rows,
                        self.id,
                        &self.response_query,
                        self.output.text_format,
                    );
                }
                return Ok(finished);
            }
            unpaged => unpaged,
        };
        if let (Ok(outcome), Some(seed)) = (&mut finished, seed) {
            self.remember_page(outcome, seed, context)?;
        }
        Ok(finished)
    }

    /// Respond over a settled envelope; the stage keeps a copy of it (the
    /// replay seed) only when the response windows.
    fn respond_settled(
        &self,
        settled: &Settled<'_>,
        structured: Value,
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
            ..self.stage(structured, settled.response_query.clone(), true)
        };
        let expires_at = stage
            .options
            .saved
            .as_ref()
            .map(|saved| saved.expires_at.clone())
            .unwrap_or_default();
        let finished = stage::respond(
            stage,
            remaining.clone(),
            Some(&pages),
            Some(&mut kept),
            context,
        )?;
        let seed = kept.map(|structured| PageReplay {
            access: self.access,
            github_identity: None,
            structured,
            remaining,
            response_query: settled.response_query.clone(),
            failure: settled.failure,
            expires_at,
            page_length: 0,
            page_scope: String::new(),
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
        let Some(snapshot) = pagination["snapshot"].as_str() else {
            return Ok(());
        };
        let next = &pagination["next"]["query"];
        seed.page_length = next["responseLength"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .or(self.options.response_length)
            .unwrap_or(self.output.auto_page_chars);
        seed.page_scope = next["responseScope"]
            .as_str()
            .unwrap_or_else(|| match pagination["scope"].as_str() {
                Some("rows") => "rows",
                Some("structuredContent") => "structured",
                _ => "text",
            })
            .into();
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
            self.replays.0.insert(&self.replays.1, snapshot, seed)
        } else {
            (Arc::new(seed), false)
        };
        if !retained {
            let mut stage = self.stage(
                saved.structured.clone(),
                saved.response_query.clone(),
                false,
            );
            stage.options = ResponsePageOptions {
                render_text: self.options.render_text,
                ..ResponsePageOptions::default()
            };
            stage.auto_page_chars = 0;
            stage.failure = saved.failure;
            let warnings = stage.structured.as_object_mut().and_then(|map| {
                map.entry("warnings")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
            });
            if let Some(warnings) = warnings {
                warnings.push(json!("The response could not be retained with a stable access identity or within snapshot storage limits; the complete result is returned without display pagination."));
            }
            *outcome = stage::respond(stage, saved.remaining.clone(), None, None, context)?
                .map_err(|_| ExecutionError::WorkerFailed)?;
        }
        Ok(())
    }

    /// Executes the admitted rows, merges the `rejected` ones back in input
    /// order, and builds the sanitized, ranked envelope with its finalized
    /// continuations.
    fn shape_rows(
        &self,
        rejected: Vec<(usize, Value)>,
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> Result<ShapedRows, ExecutionError> {
        let EvaluatedRows { mut rows, failure } = self.evaluate_rows(context)?;
        let rejected_at: Vec<usize> = rejected.iter().map(|(index, _)| *index).collect();
        for (row, index) in rows.iter_mut().zip(&self.row_indices) {
            row["index"] = json!(index);
        }
        rows.extend(rejected.into_iter().map(|(_, row)| row));
        rows.sort_by_key(|row| row["index"].as_u64());
        let (structured, response_query) = self.envelope_of(rows, &rejected_at, scope, context)?;
        Ok(ShapedRows {
            structured,
            response_query,
            rejected_at,
            failure,
        })
    }

    /// Rows (input-ordered, `index` set) → the sanitized, ranked envelope
    /// with its finalized continuations, and the response query it answers.
    fn envelope_of(
        &self,
        rows: Vec<Value>,
        rejected_at: &[usize],
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> Result<(Value, Value), ExecutionError> {
        let by_row = crate::runtime::row_indices::by_position(&self.queries, rejected_at);
        let mut structured = rows::envelope_in(rows, self.id, &by_row, &self.dispatcher.paths);
        rows::sanitize_output(
            &mut structured,
            self.id,
            &self.dispatcher.security,
            context,
            self.output.redact_emails,
        )?;
        let mut response_query = self.response_query.clone();
        if self.id.output().selects_provider_page() {
            crate::tools::selection::apply_sanitized_rows(
                self.id,
                &mut structured,
                &self.queries,
                &self.row_indices,
                &mut response_query,
                &self.dispatcher.selection_runtime(context),
            )?;
        }
        // An invalid continuation is a runtime defect: it fails loudly in
        // debug builds; otherwise finalize dropped it and marked its row,
        // which keeps its evidence.
        if let Err(error) = continuations::finalize(
            &mut structured,
            self.id,
            &continuations::Sources::Rows(&by_row),
            scope,
        ) {
            debug_assert!(
                false,
                "invalid {} continuation: {:?}",
                self.id, error.issues
            );
        }
        Ok((structured, response_query))
    }

    /// Runs every row (sharing the response window) and shapes it: result
    /// row, diagnostics, cache mark, hint policy, verbose prune, minimize.
    fn evaluate_rows(&self, context: &ExecutionContext) -> Result<EvaluatedRows, ExecutionError> {
        let (id, queries, dispatcher) = (self.id, &self.queries, &self.dispatcher);
        let response_page = self
            .options
            .response_length
            .unwrap_or(self.output.auto_page_chars);
        // Source reads fit their share of the response page, the caller's or
        // the automatic.
        let evaluated =
            execute_ordinary_queries(id, queries, Some(response_page), dispatcher, context)?;
        let mut rows = Vec::with_capacity(queries.len());
        let mut failure = None;
        let mut evaluated = evaluated.into_iter();
        for (index, query) in queries.iter().enumerate() {
            context.check()?;
            let result = evaluated.next().ok_or(ExecutionError::WorkerFailed)?;
            context.check()?;
            failure = failure.or(result.failure);
            let mut row = rows::result_row(id, index, query, result.data, result.status);
            rows::attach_diagnostics(&mut row, result.diagnostics);
            if result.cache {
                row["cache"] = json!(1);
            }
            rows::apply_hint_policy(&mut row, id, query);
            crate::response::pages::note_remaining(&mut row, id, result.remaining);
            extras::prune_row(&mut row, id, query);
            rows::minimize_row(&mut row, id, query);
            rows.push(row);
        }
        Ok(EvaluatedRows { rows, failure })
    }
}

/// One call's public envelope before the response stage, with the input
/// positions of its rejected rows.
struct ShapedRows {
    structured: Value,
    response_query: Value,
    rejected_at: Vec<usize>,
    failure: Option<FailureKind>,
}

/// The settled facts every response of one call shares.
struct Settled<'a> {
    response_query: &'a Value,
    failure: Option<FailureKind>,
}

/// Shaped result rows of one call, with the batch's failure kind.
struct EvaluatedRows {
    rows: Vec<Value>,
    failure: Option<FailureKind>,
}

#[cfg(test)]
mod batch_budget_tests {
    use super::*;

    #[test]
    fn independent_batches_fan_out_one_thread_per_query_and_classification_stays_ordered() {
        for id in ToolId::ALL {
            let ordered = matches!(id, ToolId::Clasify);
            let width = BatchBudget::new(id, 5, 12).width;
            assert_eq!(width, if ordered { 1 } else { 5 }, "{id}");
            assert_eq!(BatchBudget::new(id, 1, 12).width, 1, "{id}");
        }
    }

    #[test]
    fn concurrent_walks_split_the_cores_and_a_single_walk_keeps_the_default() {
        // Serializing walks measured slower, so rows stay concurrent and each
        // walk gets its share of the cores (see `BatchBudget`).
        let five = BatchBudget::new(ToolId::LocalSearch, 5, 12);
        assert_eq!(five.width, 5);
        assert_eq!(five.walk_threads, Some(2));
        assert_eq!(
            BatchBudget::new(ToolId::LocalSearch, 1, 12).walk_threads,
            None
        );
        // Never below one worker, even with more walks than cores.
        assert_eq!(
            BatchBudget::new(ToolId::LocalSearch, 5, 2).walk_threads,
            Some(1)
        );
        // Ordered tools run one row at a time with the full default.
        assert_eq!(BatchBudget::new(ToolId::Clasify, 5, 12).walk_threads, None);
    }
}
