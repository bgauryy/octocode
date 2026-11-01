//! The one paging rule of a list answer: a page ends at `limit` rows or at
//! the response budget, whichever comes first, on every surface. The budget
//! is the row's share of the response page less the room its envelope takes
//! ([`page_budget`](crate::response::page_windows::page_budget)), so the
//! response pager never splits a graph page into parts; the cut rows stay
//! reachable through `next`, and `pagination.totalItems` still counts the
//! whole set.
use super::*;
use crate::execution::cancel::CancellationCheck;

fn chars(value: &Value) -> usize {
    octocode_engine::json_size::utf16_units_or(value, usize::MAX)
}

/// An answer's size with the room each of its continuations takes.
fn size(output: &GraphOutput, options: &QueryOptions, id: &str) -> usize {
    output.next.iter().fold(chars(&output.value), |size, (_, resume)| {
        size.saturating_add(chars(&Value::Object(resume_row(options, id, resume))))
    })
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
        let Some(total) = output.value["pagination"]["totalItems"].as_u64() else {
            return Ok(());
        };
        if size(output, options, &self.id) <= budget {
            return Ok(());
        }
        let rows = match output.value.get_mut("rows") {
            Some(Value::Array(rows)) if rows.len() > 1 => std::mem::take(rows),
            _ => return Ok(()),
        };
        let total = crate::num::usize_of(total);
        let mut page = |keep: usize| -> Result<usize, GraphOutput> {
            output.value["rows"] = Value::Array(rows[..keep].to_vec());
            settle(output, options, total);
            if let Some(object) = output.value.as_object_mut() {
                object.remove("sourceEvidence");
            }
            self.close_sources(output, cancel)?;
            Ok(size(output, options, &self.id))
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

/// Characters a page's rows may take before projection stops: rows past the
/// budget would be cut by [`Graph::fit_page`], so a window counts them
/// without projecting them.
pub(super) fn page_budget(window: Option<usize>) -> Option<usize> {
    window.map(|window| crate::response::page_windows::page_budget(Some(window)))
}
