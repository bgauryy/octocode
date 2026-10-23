use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::read_share;
use crate::security::CheckedQuery;

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
pub(super) struct BatchBudget {
    /// Queries run at once.
    pub(super) width: usize,
    /// Walk workers per query; `None` keeps the engine default (all cores).
    pub(super) walk_threads: Option<u32>,
}

impl BatchBudget {
    pub(super) fn new(id: ToolId, queries: usize, cores: usize) -> Self {
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

pub(super) fn execute_ordinary_queries(
    tool: ToolId,
    queries: &[CheckedQuery],
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
