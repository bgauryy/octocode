use super::{ChannelRun, execute_ordinary_queries};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::result::FailureKind;
use crate::response::rows;
use crate::security::Sanitized;
use serde_json::{Value, json};

impl ChannelRun {
    /// Executes the admitted rows, merges the `rejected` ones back in input
    /// order, and builds the sanitized, ranked envelope with its finalized
    /// continuations.
    pub(super) fn shape_rows(
        &self,
        rejected: Vec<(usize, Value)>,
        context: &ExecutionContext,
    ) -> Result<ShapedRows, ExecutionError> {
        let EvaluatedRows { mut rows, failure } = self.evaluate_rows(context)?;
        let rejected_at: Vec<usize> = rejected.iter().map(|(index, _)| *index).collect();
        for (row, index) in rows.iter_mut().zip(&self.row_indices) {
            row["index"] = json!(index);
        }
        rows.extend(rejected.into_iter().map(|(_, row)| row));
        rows.sort_by_key(|row| row["index"].as_u64());
        let (structured, response_query) = self.envelope_of(rows, context)?;
        Ok(ShapedRows {
            structured,
            response_query,
            rejected_at,
            failure,
        })
    }

    /// Rows (input-ordered, `index` set) → the envelope sealed by the
    /// output gate, ranked, with its finalized continuations, and the
    /// response query it answers.
    fn envelope_of(
        &self,
        rows: Vec<Value>,
        context: &ExecutionContext,
    ) -> Result<(Sanitized, Value), ExecutionError> {
        let mut structured = self.dispatcher.security.seal_output(
            rows::envelope(self.id, rows),
            self.output.redact_emails && self.id.is_github(),
            || context.check(),
        )?;
        let mut response_query = self.response_query.clone();
        structured.edit(|structured| {
            if self.id.output().selects_provider_page() {
                crate::tools::selection::apply_sanitized_rows(
                    self.id,
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
    /// row, diagnostics, cache mark, hint policy, verbose prune, minimize.
    fn evaluate_rows(&self, context: &ExecutionContext) -> Result<EvaluatedRows, ExecutionError> {
        let (id, queries, dispatcher) = (self.id, &self.queries, &self.dispatcher);
        let response_page = self
            .options
            .response_chars
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
            let row = rows::finish_row(
                id,
                index,
                query,
                result.data,
                result.status,
                rows::RowParts {
                    diagnostics: result.diagnostics,
                    cache: result.cache,
                    remaining: result.remaining,
                },
            );
            rows.push(row);
        }
        Ok(EvaluatedRows { rows, failure })
    }
}

/// One call's public envelope before the response stage, with the input
/// positions of its rejected rows.
pub(super) struct ShapedRows {
    pub(super) structured: Sanitized,
    pub(super) response_query: Value,
    pub(super) rejected_at: Vec<usize>,
    pub(super) failure: Option<FailureKind>,
}

/// Shaped result rows of one call, with the batch's failure kind.
struct EvaluatedRows {
    rows: Vec<Value>,
    failure: Option<FailureKind>,
}
