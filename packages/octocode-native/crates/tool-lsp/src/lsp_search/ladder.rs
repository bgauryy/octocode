//! The resolution ladder of one page: which engine answers it.
//!
//! Each rung answers or hands its failure to the next one, never further:
//! the IDE rung ([`super::ide`]), the server rung ([`run_server`] and
//! [`super::page::run_target`]), the syntax rung ([`super::syntax`]) and
//! text (the failed row and its leads).
use super::*;

/// The resolution ladder: each rung answers or hands its failure to the
/// next one, never further. The IDE rung (the editor's own providers) goes
/// first; then the server; then syntax (tree-sitter facts) when no server
/// was found or started; then text (the failed row and its leads). A failed
/// IDE rung says so on the final row; one that does not apply leaves no trace.
pub(super) async fn run_page(
    query: &LspSearchQuery,
    path: String,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let fell_back = match (ide::Request {
        query,
        path: &path,
        paths,
        cancel,
        execution_config,
        response_window,
    })
    .answer()
    .await
    {
        Ok(Some(row)) => return Ok(row),
        Ok(None) => None,
        Err(ide::Fault::Stop(failure)) => return Err(Exit::Failed(failure)),
        Err(ide::Fault::Fallback(reason)) => Some(reason),
    };
    let served = run_server(
        query,
        path.clone(),
        cancel,
        pool,
        paths,
        execution_config,
        response_window,
    )
    .await;
    let mut answered = match served {
        Err(Exit::Row(failed)) if syntax::takes_over(&failed) => {
            syntax::Request {
                query,
                path: &path,
                paths,
                cancel,
                budget: octocode_runtime_base::response::page_windows::page_budget(response_window),
                security: &execution_config.security,
                discovery: &execution_config.discovery().for_one_request(),
            }
            .answer(failed)
            .await
        }
        served => served,
    };
    if let Some(reason) = fell_back
        && let Ok(row) | Err(Exit::Row(row)) = &mut answered
    {
        ide::stamp_fell_back(row, &reason);
    }
    answered
}

/// The server rung: discover the file's language server and run the page on it.
async fn run_server(
    query: &LspSearchQuery,
    path: String,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let target = discover_server(query, path, paths, execution_config)?;
    let outcome = run_target(
        query,
        &target,
        cancel,
        pool,
        paths,
        &execution_config.security,
        response_window,
    )
    .await;
    let notes = target
        .root_note
        .into_iter()
        .chain(target.config_notes)
        .collect::<Vec<_>>();
    if notes.is_empty() {
        return outcome;
    }
    match outcome {
        Ok(mut row) => {
            octocode_runtime_base::response::paging::push_warnings(&mut row, notes);
            Ok(row)
        }
        Err(Exit::Row(mut row)) => {
            octocode_runtime_base::response::paging::push_warnings(&mut row, notes);
            Err(Exit::Row(row))
        }
        failed => failed,
    }
}
