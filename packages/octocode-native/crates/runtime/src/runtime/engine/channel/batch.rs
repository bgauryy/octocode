use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::id::{ToolFamily, ToolId};
use octocode_runtime_base::response::read_share;
use octocode_runtime_base::security::CheckedQuery;

/// How many queries of one batch run at once: read-only rows fan out one
/// thread per query; mutating and self-scheduled tools stay ordered. Each
/// row's own parallel work (its walk, its reads) sizes itself from the
/// process CPU budget ([`octocode_engine::cpu`]), which every local
/// row counts toward while it runs, so concurrent rows and concurrent calls
/// split the cores instead of stacking full pools. Measured on a 5-query
/// localSearch batch over the native crates (12 cores, optimized build): one
/// thread per query with full pools 114 ms and 86 peak threads, three workers
/// 116 ms / 58, two 118 ms / 44, one walk at a time 130 ms / 29, so the rows
/// stay concurrent and only the per-walk pools shrink. Splitting the cores
/// (2 walkers each) against the same build with full pools: 708 vs 710 ms
/// median, 22 vs 72 peak threads; rounding the share up changed nothing.
pub(super) fn batch_width(id: ToolId, queries: usize) -> usize {
    if id.supports_concurrent_queries() {
        queries.max(1)
    } else {
        1
    }
}

pub(super) fn execute_ordinary_queries(
    tool: ToolId,
    queries: &[CheckedQuery],
    response_page: Option<usize>,
    dispatcher: &crate::runtime::domain_dispatch::DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Vec<octocode_runtime_base::response::outcome::DomainResult>, ExecutionError> {
    // Each row's share of the response page its output must fit.
    let context = &ExecutionContext {
        response_window: response_page
            .filter(|page| *page > 0)
            .map(|page| read_share::row_window(tool, page, queries.len())),
        ..context.clone()
    };
    // Each local row running at once computes on this host: it counts toward
    // the process budget before any fan-out (here or in the row) is sized.
    let width = batch_width(tool, queries.len());
    let computing = if tool.family() == ToolFamily::Local {
        width
    } else {
        0
    };
    let _rows = (0..computing)
        .map(|_| octocode_engine::cpu::host().enter())
        .collect::<Vec<_>>();
    // The rows' shared GitHub reads are warmed beside them: a row waits for
    // the warm-up only where it reads what it warms.
    let warm = dispatcher.plan_warm(tool, queries);
    std::thread::scope(|scope| {
        if let Some(warm) = &warm {
            scope.spawn(octocode_io::notice::carried(|| {
                dispatcher.run_warm(warm, context);
            }));
        }
        run_batch(width, queries, |query| {
            dispatcher.execute(tool, query, context, warm.as_ref())
        })
    })
}

/// Runs every query of a batch, `width` at once; the rows come back in query
/// order, and a failed row fails the batch.
fn run_batch<Q: Sync, R: Send>(
    width: usize,
    queries: &[Q],
    execute: impl Fn(&Q) -> Result<R, ExecutionError> + Sync,
) -> Result<Vec<R>, ExecutionError> {
    if width <= 1 {
        queries.iter().map(&execute).collect()
    } else {
        let execute = &execute;
        std::thread::scope(|scope| {
            let tasks = queries
                .iter()
                .map(|query| scope.spawn(octocode_io::notice::carried(move || execute(query))))
                .collect::<Vec<_>>();
            tasks
                .into_iter()
                .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed)?)
                .collect()
        })
    }
}

#[cfg(test)]
mod order_tests;
