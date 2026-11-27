//! The `lspSearch` row of a dispatch.
use super::*;

impl DomainDispatcher {
    pub(super) fn execute_lsp(
        &self,
        query: &Value,
        context: &ExecutionContext,
        egress: bool,
    ) -> Result<DomainResult, ExecutionError> {
        let query: LspSearchQuery = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        let requested = lsp_search_query::path(&query).map(str::to_owned);
        let judged = std::sync::Arc::new(octocode_runtime_base::security::JudgedFiles::default());
        self.handle.block_on(async {
            match within_request(
                context,
                self.reply_budget,
                octocode_tool_lsp::lsp_search::execute_judged(
                    query,
                    context,
                    &self.lsp_pool,
                    self.local_paths(ToolId::LspSearch, egress),
                    (&self.lsp_execution_config, std::sync::Arc::clone(&judged)),
                    local_response_window(ToolId::LspSearch, context, self.limits.auto_page_chars),
                    &self.lsp_answers,
                ),
            )
            .await?
            {
                Ok(mut data) => {
                    octocode_tool_lsp::lsp_search::wire::lean(&mut data, requested.as_deref());
                    let mut row = dispatch::value_result(data);
                    row.judged = judged.leaves();
                    Ok(row)
                }
                Err(failure) => {
                    // A cancelled/expired request is a runtime outcome,
                    // not a provider failure.
                    context.check()?;
                    let missing = failure.code == ErrorCode::NotFound;
                    // No tip here: the response stage states the code's tip
                    // for lspSearch (`ToolOutput::error_hint`).
                    let mut row = DomainResult::provider(
                        failure.code,
                        failure.message,
                        Vec::new(),
                        None,
                        Upstream {
                            retryable: failure.retryable,
                            http_status: None,
                            rate_limit: None,
                        },
                    );
                    // The same recovery every local tool gives a missing path.
                    if missing
                        && let Some(next) = requested.as_deref().and_then(|requested| {
                            octocode_runtime_base::response::result::missing_path_lead(
                                &self.paths,
                                requested,
                            )
                        })
                    {
                        octocode_runtime_base::response::result::attach_calls(&mut row.data, next);
                    }
                    Ok(row)
                }
            }
        })
    }
}
