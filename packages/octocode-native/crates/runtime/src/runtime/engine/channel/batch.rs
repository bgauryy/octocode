use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::id::{ToolFamily, ToolId};
use octocode_runtime_base::response::outcome::DomainResult;
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
) -> Result<Vec<DomainResult>, ExecutionError> {
    let page = response_page.filter(|page| *page > 0);
    // Each row's share of the response page its output must fit.
    let share = page.map(|page| read_share::row_window(tool, page, queries.len()));
    let within = |share: Option<usize>| ExecutionContext {
        response_window: share,
        ..context.clone()
    };
    let context = &within(share);
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
        let execute = |query: &CheckedQuery, context: &ExecutionContext| {
            dispatcher.execute(tool, query, context, warm.as_ref())
        };
        let rows = run_batch(width, queries, |query| execute(query, context))?;
        match (page, share) {
            (Some(page), Some(share)) if read_share::fill_together(tool) && queries.len() > 1 => {
                let room = read_share::rows_room(tool, page, queries.len());
                fill_page(rows, queries, (room, share), |open, share| {
                    let context = &within(Some(share));
                    run_batch(batch_width(tool, open.len()), open, |query| {
                        execute(query, context)
                    })
                })
            }
            _ => Ok(rows),
        }
    })
}

/// Fills the `room` the rows of one call share like water, from `rows` read
/// with an equal `share` each: while the rows that finished leave room
/// ([`read_share::widened`]), the rows still reading are read again with the
/// wider share (`read`, which cuts each again from its kept view). Each round
/// either finishes a row or ends the fill, so it reads each row at most once
/// per row of the batch.
fn fill_page<R>(
    mut rows: Vec<DomainResult>,
    queries: &[CheckedQuery],
    (room, mut share): (usize, usize),
    read: R,
) -> Result<Vec<DomainResult>, ExecutionError>
where
    R: Fn(&[&CheckedQuery], usize) -> Result<Vec<DomainResult>, ExecutionError>,
{
    loop {
        let filled = rows
            .iter()
            .map(|row| read_share::Filled {
                chars: read_share::published_chars(&row.data).row,
                open: read_share::reads_on(&row.data),
            })
            .collect::<Vec<_>>();
        let Some(wider) = read_share::widened(room, share, &filled) else {
            return Ok(rows);
        };
        let open = filled
            .iter()
            .enumerate()
            .filter_map(|(at, row)| row.open.then_some(at))
            .collect::<Vec<_>>();
        let again = read(
            &open.iter().map(|at| &queries[*at]).collect::<Vec<_>>(),
            wider,
        )?;
        let mut finished = false;
        for (at, row) in open.into_iter().zip(again) {
            finished |= !read_share::reads_on(&row.data);
            rows[at] = row;
        }
        if !finished {
            return Ok(rows);
        }
        share = wider;
    }
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
mod fill_tests;
#[cfg(test)]
mod order_tests;
