use super::{ChannelRun, execute_ordinary_queries};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::result::FailureKind;
use crate::response::rows;
use crate::security::{JudgedLeaves, Sanitized};
use serde_json::{Value, json};

impl ChannelRun {
    /// Executes the admitted rows, merges the `rejected` ones back in input
    /// order, and builds the sanitized, ranked envelope with its finalized
    /// continuations.
    pub(super) fn shape_rows(
        &self,
        mut rejected: Vec<(usize, Value)>,
        context: &ExecutionContext,
    ) -> Result<ShapedRows, ExecutionError> {
        let EvaluatedRows {
            mut rows,
            judged,
            failure,
            continues_page,
        } = self.evaluate_rows(context)?;
        let mut judged: std::collections::HashMap<usize, JudgedLeaves> =
            self.row_indices.iter().copied().zip(judged).collect();
        let rejected_at: Vec<usize> = rejected.iter().map(|(index, _)| *index).collect();
        for ((row, index), query) in rows.iter_mut().zip(&self.row_indices).zip(&self.queries) {
            if row["status"] == "error" {
                rows::require_restart(self.id, &mut row["data"], query);
                rows::attach_error_hint(self.id.output(), &mut row["data"], query);
            }
            row["index"] = json!(index);
        }
        for (_, row) in &mut rejected {
            rows::attach_error_hint(self.id.output(), &mut row["data"], &Value::Null);
        }
        rows.extend(rejected.into_iter().map(|(_, row)| row));
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
        let own = crate::security::Admitted::of(&self.queries);
        let mut structured = self.dispatcher.security.seal_judged(
            envelope,
            judged,
            &own,
            self.output.redact_emails && self.id.is_github(),
            || context.check(),
        )?;
        let mut response_query = self.response_query.clone();
        structured.edit(|structured| {
            rows::warn_redacted_rows(structured, &before);
            if self.id.selects_provider_page() {
                crate::tools::selection::apply_sanitized_rows(
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

    /// Runs every row (sharing the response window) and shapes it: result
    /// row, evidence, diagnostics and remaining-work notes.
    fn evaluate_rows(&self, context: &ExecutionContext) -> Result<EvaluatedRows, ExecutionError> {
        let (id, queries, dispatcher) = (self.id, &self.queries, &self.dispatcher);
        if self.clasify {
            let receipts = crate::tools::clasify::call(
                &dispatcher.evaluator,
                queries,
                dispatcher,
                context,
                crate::tools::clasify::sections::Runtime {
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
    pub(super) failure: Option<FailureKind>,
    /// A row's walk continues the page it answered
    /// ([`crate::response::result::ToolDiagnostics::continues_page`]).
    pub(super) continues_page: bool,
}

/// Shaped result rows of one call, with the batch's failure kind.
struct EvaluatedRows {
    rows: Vec<Value>,
    /// Each row's judged leaves, by row, relative to its `data`.
    judged: Vec<JudgedLeaves>,
    failure: Option<FailureKind>,
    continues_page: bool,
}
