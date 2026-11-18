use super::{ChannelRun, execute_ordinary_queries};
use crate::runtime::tool_output::ToolOutputOf;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::response::result::ErrorClass;
use octocode_runtime_base::response::rows;
use octocode_runtime_base::security::{JudgedLeaves, Sanitized};
use serde_json::{Value, json};

impl ChannelRun {
    pub(super) fn prepare_page_shape(&mut self, rejected: &mut [(usize, Value)]) {
        for (_, row) in rejected.iter_mut() {
            rows::note_error_hint(self.id.output(), &mut row["data"], &Value::Null);
        }
        if self.pinned.is_none() {
            self.page_shape = super::super::cursors::PageShape {
                beside_rows: self.published_beside_rows(rejected),
                repairs: if self.fresh {
                    self.repairs.clone()
                } else {
                    Vec::new()
                },
            };
        }
    }

    /// Executes the admitted rows, merges the `rejected` ones back in input
    /// order, and builds the sanitized, ranked envelope with its finalized
    /// continuations.
    pub(super) fn shape_rows(
        &self,
        rejected: Vec<(usize, Value)>,
        context: &ExecutionContext,
    ) -> Result<ShapedRows, ExecutionError> {
        let EvaluatedRows {
            mut rows,
            judged,
            failure,
            continues_page,
        } = self.evaluate_rows(self.page_shape.beside_rows, context)?;
        let mut judged: std::collections::HashMap<usize, JudgedLeaves> =
            self.row_indices.iter().copied().zip(judged).collect();
        let rejected_at: Vec<usize> = rejected.iter().map(|(index, _)| *index).collect();
        for ((row, index), query) in rows.iter_mut().zip(&self.row_indices).zip(&self.queries) {
            if row["status"] == "error" {
                rows::require_restart(self.id, &mut row["data"], query);
                rows::note_error_hint(self.id.output(), &mut row["data"], query);
            }
            row["index"] = json!(index);
        }
        rows.extend(rejected.into_iter().map(|(_, row)| row));
        for row in &mut rows {
            if row["status"] == "error" {
                rows::finish_error_row(&mut row["data"]);
            }
        }
        rows.sort_by_key(|row| row["index"].as_u64());
        // Each evaluated row's marks, at the place its data takes in the
        // envelope ([`rows::envelope`]).
        let mut marks = JudgedLeaves::default();
        for (at, row) in rows.iter().enumerate() {
            let index = row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok());
            if let Some(leaves) = index.and_then(|index| judged.remove(&index)) {
                marks.extend(leaves.under(&format!("/results/{at}/data")));
            }
        }
        let (structured, response_query) = self.envelope_of(rows, &marks, context)?;
        Ok(ShapedRows {
            structured,
            response_query,
            rejected_at,
            failure,
            continues_page,
        })
    }

    /// Rows (input-ordered, `index` set) → the envelope sealed by the
    /// output gate, ranked, with its finalized continuations, and the
    /// response query it answers.
    fn envelope_of(
        &self,
        rows: Vec<Value>,
        judged: &JudgedLeaves,
        context: &ExecutionContext,
    ) -> Result<(Sanitized, Value), ExecutionError> {
        let envelope = rows::envelope(self.id, rows);
        let before = rows::placeholders_per_row(&envelope);
        // The rows' own text: a lead or an error that repeats a request
        // value repeats it as the caller sent it.
        let own = octocode_runtime_base::security::Admitted::of(&self.queries);
        let mut structured = self.dispatcher.security.seal_judged(
            envelope,
            judged,
            &own,
            octocode_runtime_base::security::Pii {
                emails: self.output.redact_emails && self.id.is_github(),
                uuids: self.output.redact_uuid,
            },
            || context.check(),
        )?;
        let mut response_query = self.response_query.clone();
        structured.edit(|structured| {
            rows::warn_redacted_rows(structured, &before);
            if self.id.selects_provider_page() {
                octocode_tools_shared::selection::apply_sanitized_rows(
                    self.id,
                    self.id.output(),
                    structured,
                    &self.queries,
                    &self.row_indices,
                    &mut response_query,
                    &self.dispatcher.selection_runtime(context),
                )?;
            }
            Ok::<_, ExecutionError>(())
        })?;
        Ok((structured, response_query))
    }

    /// Response chars the envelope spends beside the admitted rows, as
    /// [`ChannelRun::run_rows`] publishes it: each `rejected` row (and the
    /// comma before it) and a fresh request's input-repair notes under
    /// `warnings`. The admitted rows share what the page leaves.
    fn published_beside_rows(&self, rejected: &[(usize, Value)]) -> usize {
        let size = |value: &Value| octocode_engine::json_size::utf16_units(value).unwrap_or(0);
        let rows = rejected
            .iter()
            .map(|(_, row)| size(row).saturating_add(1))
            .sum::<usize>();
        let notes = if self.fresh && !self.repairs.is_empty() {
            // `"warnings":[…],` beside the envelope's `results`.
            size(&json!({ "warnings": self.repairs })).saturating_sub(1)
        } else {
            0
        };
        rows.saturating_add(notes)
    }

    /// Runs every row (sharing the response window less what the envelope
    /// publishes `beside` them) and shapes it: result row, evidence,
    /// diagnostics and remaining-work notes.
    fn evaluate_rows(
        &self,
        beside: usize,
        context: &ExecutionContext,
    ) -> Result<EvaluatedRows, ExecutionError> {
        let (id, queries, dispatcher) = (self.id, &self.queries, &self.dispatcher);
        if self.clasify {
            let receipts = octocode_tool_clasify::clasify::call(
                &dispatcher.evaluator,
                queries,
                dispatcher,
                context,
                octocode_tool_clasify::clasify::sections::Runtime {
                    sessions: &self.section_sessions,
                    access: self.access,
                    can_resume: self.can_resume,
                },
            )?;
            return Ok(EvaluatedRows {
                judged: Vec::new(),
                rows: receipts.rows,
                failure: receipts.failure,
                continues_page: false,
            });
        }
        let response_page = self
            .options
            .response_chars
            .unwrap_or(self.output.auto_page_chars);
        // Rows sized to their share of the response page (source reads, list
        // pages) fit beside what the envelope publishes besides them.
        let response_page = match response_page {
            0 => 0,
            page => page.saturating_sub(beside).max(1),
        };
        // Source reads fit their share of the response page, the caller's or
        // the automatic.
        let evaluated =
            execute_ordinary_queries(id, queries, Some(response_page), dispatcher, context)?;
        let mut rows = Vec::with_capacity(queries.len());
        let mut judged = Vec::with_capacity(queries.len());
        let mut failure = None;
        let mut continues_page = false;
        let mut evaluated = evaluated.into_iter();
        for index in 0..queries.len() {
            context.check()?;
            let result = evaluated.next().ok_or(ExecutionError::WorkerFailed)?;
            context.check()?;
            failure = failure.or(result.failure);
            judged.push(result.judged);
            continues_page |= result.diagnostics.continues_page;
            let row = rows::finish_row(
                index,
                result.data,
                result.status,
                rows::RowParts {
                    diagnostics: result.diagnostics,
                },
            );
            rows.push(row);
        }
        Ok(EvaluatedRows {
            rows,
            judged,
            failure,
            continues_page,
        })
    }
}

/// One call's public envelope before the response stage, with the input
/// positions of its rejected rows.
pub(super) struct ShapedRows {
    pub(super) structured: Sanitized,
    pub(super) response_query: Value,
    pub(super) rejected_at: Vec<usize>,
    pub(super) failure: Option<ErrorClass>,
    /// A row's walk continues the page it answered
    /// ([`octocode_runtime_base::response::result::ToolDiagnostics::continues_page`]).
    pub(super) continues_page: bool,
}

/// Shaped result rows of one call, with the batch's failure kind.
struct EvaluatedRows {
    rows: Vec<Value>,
    /// Each row's judged leaves, by row, relative to its `data`.
    judged: Vec<JudgedLeaves>,
    failure: Option<ErrorClass>,
    continues_page: bool,
}
