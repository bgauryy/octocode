//! Per-request failures inside one answer, and next-page offers.
use super::*;

/// Partial reason of a references or callers row whose alias recovery lost
/// a server request.
pub(in super::super) const ALIAS_RECOVERY_FAILED: &str = "aliasRecoveryFailed";

/// Partial reason of a references or callers row whose verified-importer
/// recovery lost a server request.
pub(in super::super) const IMPORTER_REQUEST_FAILED: &str = "importerRequestFailed";

/// Server requests a recovery stage could not complete. A recovery stage
/// asks the server again from other anchors (aliases, importers); a request
/// that fails there is a gap in the answer, so each one is recorded here and
/// disclosed on the row, never skipped silently. Cancellation is not a
/// failure: it ends the request.
#[derive(Debug, Default)]
pub(in super::super) struct RequestFailures {
    /// Distinct messages with how many requests hit each, in first-seen order.
    pub(super) distinct: Vec<(String, usize)>,
}

impl RequestFailures {
    /// Record `failure`, or return it when it is a cancellation.
    pub(in super::super) fn record(&mut self, failure: LspFailure) -> Result<(), LspFailure> {
        if failure.is_cancelled() {
            return Err(failure);
        }
        self.push(format!("{} ({})", failure.message, failure.code), 1);
        Ok(())
    }

    /// Record an engine request error.
    pub(in super::super) fn record_engine(
        &mut self,
        error: &octocode_engine::error::Error,
    ) -> Result<(), LspFailure> {
        self.record(LspFailure::from_engine(error))
    }

    pub(super) fn push(&mut self, message: String, hits: usize) {
        match self.distinct.iter_mut().find(|(seen, _)| *seen == message) {
            Some((_, count)) => *count += hits,
            None => self.distinct.push((message, hits)),
        }
    }

    pub(in super::super) fn extend(&mut self, other: Self) {
        for (message, hits) in other.distinct {
            self.push(message, hits);
        }
    }

    pub(super) fn total(&self) -> usize {
        self.distinct.iter().map(|(_, count)| count).sum()
    }

    /// Mark `row` partial with `reason`: how many `stage` requests failed
    /// and the most frequent error, plus a retry of the row's query.
    pub(in super::super) fn disclose(
        &self,
        row: &mut Value,
        query: &LspSearchQuery,
        reason: &str,
        stage: &str,
    ) {
        let Some((message, count)) =
            self.distinct
                .iter()
                .fold(None::<&(String, usize)>, |top, entry| match top {
                    Some(top) if top.1 >= entry.1 => Some(top),
                    _ => Some(entry),
                })
        else {
            return;
        };
        let total = self.total();
        let requests = if total == 1 { "request" } else { "requests" };
        let warning = if self.distinct.len() == 1 {
            format!(
                "{stage}: {total} server {requests} failed: {message}; the locations they would add are missing."
            )
        } else {
            format!(
                "{stage}: {total} server {requests} failed ({} distinct errors; most frequent, {count}×: {message}); the locations they would add are missing.",
                self.distinct.len()
            )
        };
        partial_with_retry(row, query, reason, &[warning]);
    }
}

/// `next.nextPage` for a row that has a page after it: this query at that
/// page, pinned by its `snapshot`. A page past the contract's `page` maximum
/// ends the row at a terminal limit instead, with no lead.
pub(in super::super) fn offer_next_page(
    query: &LspSearchQuery,
    row: &mut Value,
    next: Option<super::super::render::NextPage>,
) {
    let Some(next) = next else {
        return;
    };
    if let Some(call) = crate::response::paging::next_page_call(
        row,
        ToolId::LspSearch,
        None,
        lsp_search_query::to_row(query),
        "page",
        next.page,
        &[("snapshot", json!(next.snapshot))],
    ) {
        row["next"]["nextPage"] = call;
    }
}

pub(in super::super) fn with_next(query: &LspSearchQuery, mut value: Value) -> Value {
    if value.pointer("/kind").and_then(Value::as_str) == Some("empty") {
        value["status"] = json!("empty");
        // A workspaceRoot-only query has no file to fall back to reading, and
        // reading source cannot stand in for missing diagnostics.
        let category = value.pointer("/category").and_then(Value::as_str);
        if lsp_search_query::path(query).is_some()
            && !matches!(category, Some("noDiagnostics" | "diagnosticsNotPublished"))
        {
            attach_recovery_next(&mut value, query);
        }
    }
    if let Some(context) = &lsp_search_query::rust_context(query) {
        value["rustContext"] = json!({
            "context": context,
            "fingerprint": super::super::receipt::rust_fingerprint(context)
        });
    }
    value
}
