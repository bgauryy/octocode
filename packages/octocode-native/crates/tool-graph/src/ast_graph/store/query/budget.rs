//! The one paging rule of a list answer: a page ends at `limit` rows or at
//! the response budget, whichever comes first, on every surface. The budget
//! is the row's share of the response page: the answer is measured as the
//! response delivers it (inside its result row, each continuation as the
//! issued cursor call), so a page holds every whole row that fits, the cut
//! rows stay reachable through `next`, and `pagination.total` still counts
//! the whole set. The shared response pager delivers declared sibling
//! evidence arrays that keep the first row above its response budget.
use super::*;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::id::ToolId;

fn chars(value: &Value) -> usize {
    octocode_engine::json_size::utf16_units_or(value, usize::MAX)
}

/// Response chars around one row's `data`: the envelope and the row's own
/// wrapper (`{"index":…,"data":…}`), measured with empty data.
fn frame_chars() -> usize {
    let frame = octocode_runtime_base::response::rows::envelope(
        ToolId::GraphQuery,
        vec![octocode_runtime_base::response::rows::result_row(
            0,
            json!({}),
            None,
        )],
    );
    // The empty `data` is `{}`, which the answer replaces.
    chars(&frame).saturating_sub(2)
}

/// An answer's size as the response delivers it: inside its result row, with
/// each continuation it states as the call publication puts in its place
/// (`next.<channel>`, an issued cursor of the published width).
fn size(output: &GraphOutput) -> usize {
    let size = chars(&output.value).saturating_add(frame_chars());
    if output.next.is_empty() {
        return size;
    }
    let next = output
        .next
        .iter()
        .map(|(channel, _)| {
            let call = octocode_runtime_base::response::result::Continuation::cursor(
                ToolId::GraphQuery.as_str(),
                octocode_runtime_base::response::cursor::issued::placeholder(),
            );
            (channel.name(), call)
        })
        .collect::<Map<_, _>>();
    // `,"next":{…}` after the answer's last field.
    let next = chars(&json!({ "next": next })).saturating_sub(2) + 1;
    size.saturating_add(next)
}

impl Graph {
    /// Cuts a list answer's rows to the response budget: the largest prefix
    /// (at least one row, so a walk always moves) whose whole answer, its
    /// `sourceEvidence` closure and `next` included, fits.
    pub(super) fn fit_page(
        &self,
        output: &mut GraphOutput,
        options: &QueryOptions,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), GraphOutput> {
        let Some(budget) = self.page_budget else {
            return Ok(());
        };
        let Some(total) = output.value["pagination"]["total"].as_u64() else {
            return Ok(());
        };
        if size(output) <= budget {
            return Ok(());
        }
        let rows = match output.value.get_mut("rows") {
            Some(Value::Array(rows)) if rows.len() > 1 => std::mem::take(rows),
            _ => return Ok(()),
        };
        let total = octocode_engine::num::saturating_usize(total);
        let mut page = |keep: usize| -> Result<usize, GraphOutput> {
            output.value["rows"] = Value::Array(rows[..keep].to_vec());
            settle(output, options, total);
            if let Some(object) = output.value.as_object_mut() {
                object.remove("sourceEvidence");
            }
            self.close_sources(output, cancel)?;
            Ok(size(output))
        };
        // The answer grows with every kept row: bisect for the last fit.
        let (mut fits, mut over) = (1, rows.len());
        while fits + 1 < over {
            let middle = fits + (over - fits) / 2;
            if page(middle)? <= budget {
                fits = middle;
            } else {
                over = middle;
            }
        }
        page(fits)?;
        Ok(())
    }
}

/// A window's budget: the characters its rows may take, each row counted as
/// the answer delivers it. The lean view ([`super::lean::project`]) drops
/// plain provenance after the window, so a lean answer counts its rows lean;
/// counting them whole would stop the window at a fraction of the page.
#[derive(Clone, Copy, Debug)]
pub(super) struct RowBudget {
    pub(super) chars: usize,
    lean: bool,
}

impl RowBudget {
    /// Characters `row` takes in the delivered answer.
    pub(super) fn delivered(self, row: &impl serde::Serialize) -> usize {
        if !self.lean {
            return octocode_engine::json_size::utf16_units_or(row, usize::MAX);
        }
        let Ok(mut row) = serde_json::to_value(row) else {
            return usize::MAX;
        };
        super::lean::project(&mut row);
        chars(&row)
    }
}

impl Graph {
    /// The budget a list answer's window counts its rows against.
    pub(super) fn row_budget(&self) -> Option<RowBudget> {
        self.page_budget.map(|chars| RowBudget {
            chars,
            lean: self.compact_provenance,
        })
    }
}
